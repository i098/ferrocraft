//! Leashes. The knot (object 77) is `LeadKnotRenderer` with `LeashModel`, read from the bytecode of
//! the real 1.8.9 client: one 6x8x6 box at `(-3, -6, -3)` of a 32x32 texture, drawn under
//! `scale(-1, -1, 1)` with culling off. The rope is `MobRenderer.renderRiders` (`RenderLiving.renderLeash`):
//! two 25-point triangle strips of flat brown, unlit, from the mob to its holder.

use super::{add_box, skin_dims, EntityDraw, Skin, Skins, V3};
use crate::mesh::{MeshData, Vertex};

/// Object type of a leash knot.
pub const KNOT: i8 = 77;

/// The entity a mob is leashed to, as `renderLeash` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Holder {
    /// Interpolated position of the holder's feet.
    pub pos: [f64; 3],
    /// `Entity.getEyeHeight`.
    pub eye_height: f32,
    /// Yaw and pitch in degrees, interpolated by half the partial tick (`renderLeash` quirk).
    pub look: [f64; 2],
    /// `instanceof EntityHanging`: the rope hangs from the top of the knot.
    pub hanging: bool,
}

/// `renderLeash.lerp`: no wrapping of angles.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

impl Holder {
    /// `prev` and `cur` are `[yaw, pitch]` of the last and the current tick.
    pub fn new(
        pos: [f64; 3],
        eye_height: f32,
        prev: [f32; 2],
        cur: [f32; 2],
        partial: f32,
        hanging: bool,
    ) -> Holder {
        let t = f64::from(partial * 0.5);
        Holder {
            pos,
            eye_height,
            look: [0, 1].map(|i| lerp(f64::from(prev[i]), f64::from(cur[i]), t)),
            hanging,
        }
    }
}

/// The two ends of the rope in world space: where it leaves the mob and the offset to the holder end.
pub fn rope_ends(d: &EntityDraw, h: &Holder) -> ([f64; 3], [f64; 3]) {
    const RAD: f64 = 0.017_453_292_384_743_69;
    let (yaw, pitch) = (h.look[0] * RAD, h.look[1] * RAD);
    let (mut cos, mut sin, mut sin_pitch) = (yaw.cos(), yaw.sin(), pitch.sin());
    if h.hanging {
        (cos, sin, sin_pitch) = (0.0, 0.0, -1.0);
    }
    let cos_pitch = pitch.cos();
    let hx = h.pos[0] - cos * 0.7 - sin * 0.5 * cos_pitch;
    let hy = h.pos[1] + f64::from(h.eye_height) * 0.7 - sin_pitch * 0.5 - 0.25;
    let hz = h.pos[2] - sin * 0.7 + cos * 0.5 * cos_pitch;
    let body = f64::from(d.yaw) * RAD + std::f64::consts::FRAC_PI_2;
    let width = f64::from(d.size.0);
    let (ox, oz) = (body.cos() * width * 0.4, body.sin() * width * 0.4);
    let start = [
        d.pos[0] + ox,
        d.pos[1] - (1.6 - f64::from(d.size.1)) * 0.5,
        d.pos[2] + oz,
    ];
    // The deltas are floats in vanilla.
    let delta = [
        f64::from((hx - (d.pos[0] + ox)) as f32),
        f64::from((hy - d.pos[1]) as f32),
        f64::from((hz - (d.pos[2] + oz)) as f32),
    ];
    (start, delta)
}

/// The 25 `(a, b)` points of strip `strip` (0 or 1) with their shade, in `renderLeash` order.
fn strip_points(start: [f64; 3], delta: [f64; 3], strip: u8) -> Vec<(V3, V3, [f32; 3])> {
    const W: f64 = 0.025;
    (0..=24)
        .map(|j| {
            let f = j as f32 / 24.0;
            let shade = if j % 2 == 0 { 0.7 } else { 1.0 };
            let c = [0.5 * shade, 0.4 * shade, 0.3 * shade];
            let x = start[0] + delta[0] * f64::from(f);
            let y = start[1]
                + delta[1] * f64::from(f * f + f) * 0.5
                + f64::from((24.0 - j as f32) / 18.0 + 0.125);
            let z = start[2] + delta[2] * f64::from(f);
            let p = |dx: f64, dy: f64, dz: f64| [(x + dx) as f32, (y + dy) as f32, (z + dz) as f32];
            if strip == 0 {
                (p(0.0, 0.0, 0.0), p(W, W, 0.0), c)
            } else {
                (p(0.0, W, 0.0), p(W, 0.0, W), c)
            }
        })
        .collect()
}

/// One strip as quads, `(a0, b0, b1, a1)` per step, with both windings as the culling is off.
fn push_strip(out: &mut MeshData, pts: &[(V3, V3, [f32; 3])], uv: [f32; 2]) {
    const ORDER: [u32; 12] = [0, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0];
    for w in pts.windows(2) {
        let base = out.vertices.len() as u32;
        for (pos, c) in [
            (w[0].0, w[0].2),
            (w[0].1, w[0].2),
            (w[1].1, w[1].2),
            (w[1].0, w[1].2),
        ] {
            let color = [c[0], c[1], c[2], 1.0];
            out.vertices.push(Vertex {
                pos,
                uv,
                color,
                light: [1.0, 1.0],
            });
        }
        out.indices.extend(ORDER.map(|i| base + i));
    }
}

/// `renderLeash`: the rope of a leashed mob, appended in world space. It is unlit and unculled.
pub fn rope(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let Some(h) = d.look.leash else { return };
    let (start, delta) = rope_ends(d, &h);
    let at = Skins::uv_offset(skins.v_offset(Skin::White));
    let uv = [
        at[0] + 0.5 / Skins::width() as f32,
        at[1] + 0.5 / Skins::height() as f32,
    ];
    for strip in 0..2 {
        push_strip(out, &strip_points(start, delta, strip), uv);
    }
}

/// The knot: the box of `LeashModel` at the centre of its block, under `scale(-1, -1, 1)`.
pub fn knot(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let v_off = skins.v_offset(Skin::LeadKnot);
    let color = if skins.has(Skin::LeadKnot) {
        [1.0; 3]
    } else {
        [0.55, 0.4, 0.25]
    };
    // `EntityLeashKnot.updateBoundingBox`: the entity stands at the centre of its block.
    let centre = d.pos.map(|p| p.floor() + 0.5);
    let tf = |p: V3| -> V3 {
        [
            (centre[0] - f64::from(p[0]) / 16.0) as f32,
            (centre[1] - f64::from(p[1]) / 16.0) as f32,
            (centre[2] + f64::from(p[2]) / 16.0) as f32,
        ]
    };
    add_box(
        out,
        &tf,
        [-3.0, -6.0, -3.0],
        [6.0, 8.0, 6.0],
        [6.0, 8.0, 6.0],
        (0.0, 0.0),
        false,
        v_off,
        color,
        skin_dims(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::tests::steve;
    use crate::entity::Class;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }

    fn cow() -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(92);
        d.pos = [0.0, 64.0, 0.0];
        d.yaw = 0.0;
        d.size = (0.9, 1.3);
        d
    }

    /// Hand-computed from `renderRiders`: a knot holder drops the look (`d2 = d3 = 0`, `d4 = -1`), so the
    /// holder end is the knot centre plus 0.5 minus 0.25, i.e. 0.25 over its eye line `0.7 * -0.0625`.
    #[test]
    fn a_knot_holder_ends_the_rope_above_its_centre() {
        let h = Holder::new([4.5, 65.5, 0.5], -0.0625, [0.0; 2], [0.0; 2], 0.0, true);
        let (start, delta) = rope_ends(&cow(), &h);
        // Body yaw 0: the start is pushed 0.9 * 0.4 along (cos 90deg, sin 90deg) = +z.
        close(start[0], 0.0);
        close(start[1], 64.0 - (1.6 - 1.3) * 0.5);
        close(start[2], 0.36);
        close(start[0] + delta[0], 4.5);
        close(start[2] + delta[2], 0.5);
        close(64.0 + delta[1], 65.5 + -0.0625 * 0.7 + 0.5 - 0.25);
    }

    /// A player holder at yaw 90 turns `d2 = cos = 0`, `d3 = sin = 1`: the hand end is 0.5 behind the
    /// player along x and 0.7 along z.
    #[test]
    fn a_player_holder_ends_the_rope_at_the_hand() {
        let h = Holder::new([0.0, 64.0, 5.0], 1.62, [0.0; 2], [90.0, 0.0], 2.0, false);
        // partial 2 * 0.5 = 1: the current look, yaw 90 and pitch 0.
        close(h.look[0], 90.0);
        let (start, delta) = rope_ends(&cow(), &h);
        close(start[0] + delta[0], 0.0 - 0.0 * 0.7 - 1.0 * 0.5);
        close(start[2] + delta[2], 5.0 - 1.0 * 0.7 + 0.0);
    }

    #[test]
    fn the_rope_is_two_strips_of_25_segment_pairs_and_sags_in_the_middle() {
        let mut out = MeshData::default();
        let mut d = cow();
        d.look.leash = Some(Holder::new(
            [3.0, 64.0, 0.0],
            1.62,
            [0.0; 2],
            [0.0; 2],
            0.0,
            false,
        ));
        rope(&mut out, &d, &Skins::flat());
        assert_eq!(out.vertices.len(), 2 * 24 * 4);
        // The first point of strip 0 is at the mob end, `(24 - 0) / 18 + 0.125` over the start height.
        let first = out.vertices[0].pos;
        close(f64::from(first[1]), 64.0 - 0.15 + 24.0 / 18.0 + 0.125);
    }

    #[test]
    fn the_knot_is_a_six_by_eight_by_six_box_from_two_sixteenths_below_to_six_above_the_centre() {
        let mut out = MeshData::default();
        let mut d = cow();
        d.class = Class::Object(KNOT);
        d.pos = [10.5, 70.5, -3.5];
        knot(&mut out, &d, &Skins::flat());
        let ys = out.vertices.iter().map(|v| f64::from(v.pos[1]));
        close(ys.clone().fold(f64::MAX, f64::min), 70.5 - 2.0 / 16.0);
        close(ys.fold(f64::MIN, f64::max), 70.5 + 6.0 / 16.0);
        let xs = out.vertices.iter().map(|v| f64::from(v.pos[0]));
        close(xs.clone().fold(f64::MAX, f64::min), 10.5 - 3.0 / 16.0);
        close(xs.fold(f64::MIN, f64::max), 10.5 + 3.0 / 16.0);
    }
}
