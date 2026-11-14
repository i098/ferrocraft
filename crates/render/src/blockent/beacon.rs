//! The beacon beam: `TileEntityBeacon` (segments, pyramid levels, beam fade-in) and
//! `TileEntityBeaconRenderer` (the rotating inner beam and the translucent outer beam).
//!
//! Not ported: the potion effects the server applies, and the sounds.

use super::{View, BEACON};
use crate::blocks::light_opacity;
use crate::mesh::{MeshData, Vertex};
use std::f64::consts::PI;

/// Jar path of the beam texture.
pub const BEAM_TEXTURE: &str = "textures/entity/beacon_beam.png";

/// `TileEntityBeacon.getMaxRenderDistanceSquared`: 256 blocks.
const RENDER_DISTANCE_SQ: f64 = 65536.0;
const BEDROCK: u16 = 7;
const STAINED_GLASS: u16 = 95;
const STAINED_GLASS_PANE: u16 = 160;
/// Iron, gold, diamond and emerald blocks, the blocks of the pyramid.
const BASE_BLOCKS: [u16; 4] = [42, 41, 57, 133];
use crate::entity::mobs::FLEECE;

/// One run of the beam with a single color (`TileEntityBeacon.BeamSegment`).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub color: [f32; 3],
    pub height: u32,
}

/// The client state of one `TileEntityBeacon`.
#[derive(Clone, Debug, Default)]
pub struct Beacon {
    pub segments: Vec<Segment>,
    pub complete: bool,
    /// `levels` (`field_146012_l`); 0 until the pyramid is scanned.
    pub levels: u32,
    /// The `getTotalWorldTime() / 80` of the last scan; `None` before the first frame.
    period: Option<i64>,
    /// Set by `receiveClientEvent(1)`: rescan on the next frame.
    pub(super) dirty: bool,
    /// `field_146016_i`: world time of the last `shouldBeamRender`.
    last_time: i64,
    /// `field_146014_j`: fade-in of the beam, 0 to 1.
    scale: f32,
}

impl Beacon {
    /// `TileEntityBeacon.updateSegmentColors`; `block` reads the block state at a world position
    /// (0 outside the loaded chunks).
    pub fn scan(&mut self, pos: [i32; 3], block: &impl Fn(i32, i32, i32) -> u16) {
        let [x, y, z] = pos;
        self.levels = 0;
        self.complete = true;
        let mut segments = vec![Segment {
            color: FLEECE[0],
            height: 1,
        }];
        let mut first = true;
        for h in y + 1..256 {
            let state = block(x, h, z);
            let id = state >> 4;
            let color = if matches!(id, STAINED_GLASS | STAINED_GLASS_PANE) {
                FLEECE[usize::from(state & 15)]
            } else {
                if light_opacity(state) >= 15 && id != BEDROCK {
                    self.complete = false;
                    segments.clear();
                    break;
                }
                last(&mut segments).height += 1;
                continue;
            };
            let current = last(&mut segments).color;
            let color = if first {
                color
            } else {
                [0, 1, 2].map(|i| (current[i] + color[i]) / 2.0)
            };
            if color == current {
                last(&mut segments).height += 1;
            } else {
                segments.push(Segment { color, height: 1 });
            }
            first = false;
        }
        self.segments = segments;
        if self.complete {
            for level in 1..=4 {
                let below = y - level;
                let full = below >= 0
                    && (x - level..=x + level).all(|bx| {
                        (z - level..=z + level)
                            .all(|bz| BASE_BLOCKS.contains(&(block(bx, below, bz) >> 4)))
                    });
                if !full {
                    break;
                }
                self.levels = level as u32;
            }
            self.complete = self.levels > 0;
        }
    }

    /// `TileEntityBeacon.shouldBeamRender`: the fade-in factor of the beam this frame.
    fn beam_scale(&mut self, world_time: i64) -> f32 {
        if !self.complete {
            return 0.0;
        }
        let gap = world_time - self.last_time;
        self.last_time = world_time;
        if gap > 1 {
            self.scale = (self.scale - gap as f32 / 40.0).max(0.0);
        }
        self.scale = (self.scale + 0.025).min(1.0);
        self.scale
    }
}

fn last(segments: &mut [Segment]) -> &mut Segment {
    let n = segments.len();
    &mut segments[n - 1]
}

/// The beam geometry of one frame: the opaque inner beam and the translucent outer beam.
#[derive(Default)]
pub struct Beams {
    pub inner: MeshData,
    pub outer: MeshData,
}

impl super::BlockEntities {
    /// Advances the beacons (`TileEntityBeacon.update` scans every 80 ticks) and builds the beams of
    /// those in range. A beacon first seen waits for the next scan like a freshly loaded tile.
    pub fn beams(&mut self, view: &View, block: impl Fn(i32, i32, i32) -> u16) -> Beams {
        let mut out = Beams::default();
        let period = view.world_time.div_euclid(80);
        for (&pos, &state) in &self.blocks {
            if state >> 4 != BEACON {
                continue;
            }
            let beacon = self.beacons.entry(pos).or_default();
            if std::mem::take(&mut beacon.dirty) || beacon.period.is_some_and(|p| p != period) {
                beacon.scan(pos, &block);
            }
            beacon.period = Some(period);
            let centre = [0, 1, 2].map(|i| f64::from(pos[i]) + 0.5 - view.eye[i]);
            if centre.iter().map(|d| d * d).sum::<f64>() >= RENDER_DISTANCE_SQ {
                continue;
            }
            let scale = beacon.beam_scale(view.world_time);
            if scale > 0.0 {
                draw(&mut out, pos, &beacon.segments, scale, view);
            }
        }
        out
    }
}

/// The two passes of `TileEntityBeaconRenderer.renderTileEntityAt` for one beacon.
fn draw(out: &mut Beams, pos: [i32; 3], segments: &[Segment], scale: f32, view: &View) {
    let time = view.world_time as f32 + view.partial;
    let scroll = -time * 0.2 - (-time * 0.1).floor();
    let angle = f64::from(time) * 0.025 * -1.5;
    let [x, y, z] = pos.map(f64::from);
    // The corners of the inner beam, a square of radius 0.2 turning around the block centre.
    let corner = |a: f64| {
        [
            x + 0.5 + (angle + a).cos() * 0.2,
            z + 0.5 + (angle + a).sin() * 0.2,
        ]
    };
    let inner = [
        corner(2.356_194_490_192_345),
        corner(PI / 4.0),
        corner(3.926_990_816_987_241_4),
        corner(5.497_787_143_782_138),
    ];
    // The outer beam is the fixed square from 0.2 to 0.8.
    let outer = [[0.2, 0.2], [0.8, 0.2], [0.2, 0.8], [0.8, 0.8]].map(|[dx, dz]| [x + dx, z + dz]);
    let v0 = f64::from(-1.0 + scroll);
    let mut bottom = 0.0;
    for seg in segments {
        let top = bottom + f64::from(seg.height);
        let rise = f64::from(seg.height as f32 * scale);
        let (yb, yt) = (y + bottom, y + top);
        let [r, g, b] = seg.color;
        beam(
            &mut out.inner,
            &inner,
            [yb, yt],
            [v0, rise * (0.5 / 0.2) + v0],
            [r, g, b, 1.0],
        );
        beam(
            &mut out.outer,
            &outer,
            [yb, yt],
            [v0, rise + v0],
            [r, g, b, 0.125],
        );
        bottom = top;
    }
}

/// The four faces of a beam: corners 0-1, 3-2, 1-3 and 2-0, each `u` 1 at its first corner and 0
/// at its second.
fn beam(
    out: &mut MeshData,
    c: &[[f64; 2]; 4],
    [yb, yt]: [f64; 2],
    [vb, vt]: [f64; 2],
    color: [f32; 4],
) {
    for (a, b) in [(0, 1), (3, 2), (1, 3), (2, 0)] {
        let base = out.vertices.len() as u32;
        for (corner, u, y, v) in [
            (a, 1.0, yt, vt),
            (a, 1.0, yb, vb),
            (b, 0.0, yb, vb),
            (b, 0.0, yt, vt),
        ] {
            out.vertices.push(Vertex {
                pos: [c[corner][0] as f32, y as f32, c[corner][1] as f32],
                uv: [u, v as f32],
                color,
                light: [1.0; 2],
            });
        }
        out.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1-level iron pyramid under the beacon at (0, 64, 0), `glass` blocks above.
    fn world(glass: &[(i32, u16)]) -> impl Fn(i32, i32, i32) -> u16 + '_ {
        move |x, y, z| match (x, y, z) {
            (0, 64, 0) => BEACON << 4,
            (-1..=1, 63, -1..=1) => 42 << 4,
            (0, h, 0) => glass.iter().find(|(gy, _)| *gy == h).map_or(0, |g| g.1),
            _ => 0,
        }
    }

    #[test]
    fn open_sky_gives_one_white_segment_to_the_height_limit() {
        let mut b = Beacon::default();
        b.scan([0, 64, 0], &world(&[]));
        assert!(b.complete);
        assert_eq!(b.levels, 1);
        // The beacon block itself plus the blocks up to y 255.
        assert_eq!(
            b.segments,
            [Segment {
                color: FLEECE[0],
                height: 192
            }]
        );
    }

    #[test]
    fn glass_tints_and_averages_with_the_color_below() {
        let red = 14usize;
        let blue = 11usize;
        let mut b = Beacon::default();
        b.scan(
            [0, 64, 0],
            &world(&[(70, (95 << 4) | red as u16), (71, (95 << 4) | blue as u16)]),
        );
        let [r, g, bl] = FLEECE[red];
        let [r2, g2, b2] = FLEECE[blue];
        let heights: Vec<u32> = b.segments.iter().map(|s| s.height).collect();
        assert_eq!(heights, [6, 1, 185]);
        assert_eq!(b.segments[1].color, [0.6, 0.2, 0.2]);
        assert_eq!(b.segments[1].color, [r, g, bl]);
        assert_eq!(
            b.segments[2].color,
            [(r + r2) / 2.0, (g + g2) / 2.0, (bl + b2) / 2.0]
        );
    }

    #[test]
    fn an_opaque_block_above_ends_the_beam() {
        let mut b = Beacon::default();
        b.scan([0, 64, 0], &world(&[(80, 1 << 4)]));
        assert!(!b.complete);
        assert!(b.segments.is_empty());
    }

    #[test]
    fn no_pyramid_means_no_beam() {
        let mut b = Beacon::default();
        b.scan([0, 64, 0], &|x, y, z| {
            if (x, y, z) == (0, 64, 0) {
                BEACON << 4
            } else {
                0
            }
        });
        assert_eq!((b.levels, b.complete), (0, false));
    }

    #[test]
    fn beam_fades_in_by_a_40th_per_frame_and_out_after_a_gap() {
        let mut b = Beacon {
            complete: true,
            ..Beacon::default()
        };
        assert_eq!(b.beam_scale(10), 0.025);
        assert_eq!(b.beam_scale(10), 0.05);
        // 20 ticks without a frame: minus 0.5, plus 0.025.
        let after = b.beam_scale(30);
        assert!((after - 0.025).abs() < 1e-6, "{after}");
    }
}

#[cfg(test)]
mod event_tests {
    use super::super::{BlockEntities, View};
    use super::*;

    fn frame(be: &mut BlockEntities, time: i64, glass: bool) {
        let view = View {
            eye: [0.0, 64.0, 0.0],
            world_time: time,
            partial: 0.0,
        };
        be.beams(&view, |x, y, z| match (x, y, z) {
            (-1..=1, 63, -1..=1) => 42 << 4,
            (0, 70, 0) if glass => (95 << 4) | 14,
            _ => 0,
        });
    }

    fn beacon_with_period_set() -> BlockEntities {
        let mut be = BlockEntities::default();
        be.set_state([0, 64, 0], BEACON << 4);
        frame(&mut be, 0, false);
        be
    }

    #[test]
    fn glass_is_seen_at_once_after_beacon_event_1() {
        let mut be = beacon_with_period_set();
        be.block_action([0, 64, 0], BEACON, 1, 0);
        frame(&mut be, 1, true);
        assert_eq!(be.beacons[&[0, 64, 0]].segments[1].color, [0.6, 0.2, 0.2]);
    }

    #[test]
    fn glass_waits_for_the_next_period_without_the_event() {
        let mut be = beacon_with_period_set();
        frame(&mut be, 1, true);
        assert!(be.beacons[&[0, 64, 0]].segments.is_empty());
    }
}
