//! Animated block textures: `TextureAtlasSprite.tick` driven by the `animation` block of `<name>.png.mcmeta`
//! (`frametime`, `frames`, `interpolate`). The atlas keeps every frame resampled to tile size; [`Animator`]
//! steps one client tick and returns only the cells whose pixels changed.

use crate::assets::{resample, Image};
use crate::mip;
use serde_json::Value;
use std::borrow::Cow;

/// `AnimationMetadataSerializer`: `frames` is the display order, each entry a frame index and its time
/// (`None` = the default `frametime`).
struct Meta {
    default_time: u32,
    frames: Vec<(usize, Option<u32>)>,
    interpolate: bool,
}

/// `None` when there is no `animation` object or it is invalid (vanilla rejects the sprite).
fn parse(bytes: &[u8]) -> Option<Meta> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    let animation = root.get("animation")?.as_object()?;
    let time = |v: &Value| v.as_i64().filter(|t| (1..=i32::MAX as i64).contains(t));
    let default_time = match animation.get("frametime") {
        None => 1,
        Some(v) => time(v)? as u32,
    };
    let mut frames = Vec::new();
    let list: &[Value] = match animation.get("frames") {
        None => &[],
        Some(f) => f.as_array()?,
    };
    for frame in list {
        frames.push(match frame {
            Value::Object(o) => {
                let t = match o.get("time") {
                    None => None,
                    Some(v) => Some(time(v)? as u32),
                };
                (o.get("index")?.as_u64()? as usize, t)
            }
            other => (other.as_u64()? as usize, None),
        });
    }
    Some(Meta {
        default_time,
        frames,
        interpolate: animation
            .get("interpolate")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// One animated atlas cell: all frames at tile size, each with its mip levels (`applyMipmaps`), and the order
/// and timing they play in.
pub(crate) struct Animation {
    cell: u32,
    /// Level 0 of every frame: the tile itself.
    base: Vec<Vec<u8>>,
    /// `frames[n][l]` is level `l` of frame `n`; see [`Animation::set_levels`].
    frames: Vec<Vec<Vec<u8>>>,
    sequence: Vec<(usize, u32)>,
    interpolate: bool,
}

impl Animation {
    /// `image` is a strip of `height / width` square frames. `None` for a single frame or invalid metadata.
    pub(crate) fn new(cell: u32, image: &Image, tile: usize, mcmeta: &[u8]) -> Option<Animation> {
        let meta = parse(mcmeta)?;
        let side = image.width;
        let count = image.height.checked_div(side)?;
        if count < 2 {
            return None;
        }
        let base: Vec<Vec<u8>> = (0..count)
            .map(|n| {
                let rows = &image.rgba[n * side * side * 4..(n + 1) * side * side * 4];
                let frame = Image {
                    width: side,
                    height: side,
                    rgba: rows.to_vec(),
                };
                resample(&frame, tile)
            })
            .collect();
        // `TextureAtlasSprite.loadSprite`: no `frames` list plays every frame in order.
        let sequence = if meta.frames.is_empty() {
            (0..count).map(|n| (n, meta.default_time)).collect()
        } else {
            let time = |t: Option<u32>| t.unwrap_or(meta.default_time);
            meta.frames.iter().map(|&(i, t)| (i, time(t))).collect()
        };
        let frames = base.iter().map(|f| vec![f.clone()]).collect();
        Some(Animation {
            cell,
            base,
            frames,
            sequence,
            interpolate: meta.interpolate,
        })
    }

    /// `TextureAtlasSprite.applyMipmaps`: builds levels 1..=`levels` of every frame.
    pub(crate) fn set_levels(&mut self, tile: usize, levels: u32) {
        self.frames = self
            .base
            .iter()
            .map(|b| {
                let mut chain = vec![b.clone()];
                chain.extend(mip::chain(b, tile, levels));
                chain
            })
            .collect();
    }

    /// `animatePrismarine`: frame `index` blended towards the next frame in the sequence, level by level.
    fn blend(&self, active: usize, ticks: u32) -> Option<Vec<Vec<u8>>> {
        let (from, time) = self.sequence[active];
        let (to, _) = self.sequence[(active + 1) % self.sequence.len()];
        let (a, b) = (self.frames.get(from)?, self.frames.get(to)?);
        if from == to {
            return None;
        }
        let keep = 1.0 - f64::from(ticks) / f64::from(time);
        let mix = |x: u8, y: u8| (f64::from(x) * keep + f64::from(y) * (1.0 - keep)) as u8;
        Some(
            a.iter()
                .zip(b)
                .map(|(la, lb)| {
                    let mut out = la.clone();
                    for (o, p) in out
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(lb.as_chunks::<4>().0)
                    {
                        for c in 0..3 {
                            o[c] = mix(o[c], p[c]);
                        }
                    }
                    out
                })
                .collect(),
        )
    }
}

/// Per-sprite `frameCounter` and `tickCounter`.
#[derive(Default)]
pub(crate) struct Animator {
    state: Vec<(usize, u32)>,
}

impl Animator {
    /// Advances every animation one client tick; yields `(cell, mip levels of the tile)` for each cell that changed.
    pub(crate) fn tick<'a>(
        &mut self,
        animations: &'a [Animation],
    ) -> Vec<(u32, Cow<'a, [Vec<u8>]>)> {
        self.state.resize(animations.len(), (0, 0));
        let mut changed = Vec::new();
        for (anim, (active, ticks)) in animations.iter().zip(&mut self.state) {
            *ticks += 1;
            if *ticks >= anim.sequence[*active].1 {
                let old = anim.sequence[*active].0;
                *active = (*active + 1) % anim.sequence.len();
                *ticks = 0;
                let new = anim.sequence[*active].0;
                if old != new {
                    if let Some(frame) = anim.frames.get(new) {
                        changed.push((anim.cell, Cow::Borrowed(frame.as_slice())));
                    }
                }
            } else if anim.interpolate {
                if let Some(frame) = anim.blend(*active, *ticks) {
                    changed.push((anim.cell, Cow::Owned(frame)));
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frames` solid 2x2 frames, frame `n` has red channel `10 * (n + 1)`.
    fn strip(frames: usize) -> Image {
        let rgba = (0..frames * 4)
            .flat_map(|p| [10 * (p / 4 + 1) as u8, 0, 0, 255])
            .collect();
        Image {
            width: 2,
            height: 2 * frames,
            rgba,
        }
    }

    fn red(changed: &[(u32, Cow<[Vec<u8>]>)]) -> Vec<(u32, u8)> {
        changed.iter().map(|(c, px)| (*c, px[0][0])).collect()
    }

    #[test]
    fn frame_changes_at_frametime() {
        let anim = Animation::new(7, &strip(3), 2, br#"{"animation":{"frametime":2}}"#).unwrap();
        let anims = [anim];
        let mut clock = Animator::default();
        assert!(clock.tick(&anims).is_empty());
        assert_eq!(red(&clock.tick(&anims)), [(7, 20)]);
        assert!(clock.tick(&anims).is_empty());
        assert_eq!(red(&clock.tick(&anims)), [(7, 30)]);
        clock.tick(&anims);
        assert_eq!(red(&clock.tick(&anims)), [(7, 10)]);
    }

    #[test]
    fn frames_list_orders_and_times_frames() {
        let meta = br#"{"animation":{"frametime":5,"frames":[2,{"index":0,"time":1},1]}}"#;
        let anims = [Animation::new(1, &strip(3), 2, meta).unwrap()];
        let mut clock = Animator::default();
        let mut seen = Vec::new();
        for _ in 0..12 {
            seen.extend(red(&clock.tick(&anims)).into_iter().map(|(_, r)| r));
        }
        // Slot 0 (frame 2) lasts 5 ticks, slot 1 (frame 0) 1 tick, slot 2 (frame 1) 5 ticks, then it loops.
        assert_eq!(seen, [10, 20, 30]);
    }

    #[test]
    fn interpolation_blends_towards_the_next_frame() {
        let meta = br#"{"animation":{"frametime":4,"interpolate":true}}"#;
        let anims = [Animation::new(1, &strip(2), 2, meta).unwrap()];
        let mut clock = Animator::default();
        // tick 1: 1 - 1/4 of frame 0 (10) + 1/4 of frame 1 (20) = 12.5 -> 12
        assert_eq!(red(&clock.tick(&anims)), [(1, 12)]);
        assert_eq!(red(&clock.tick(&anims)), [(1, 15)]);
        assert_eq!(red(&clock.tick(&anims)), [(1, 17)]);
        assert_eq!(red(&clock.tick(&anims)), [(1, 20)]);
    }

    #[test]
    fn changed_cell_carries_every_mip_level() {
        let meta = br#"{"animation":{"frametime":1,"interpolate":true}}"#;
        let mut anim = Animation::new(1, &strip(2), 2, meta).unwrap();
        anim.set_levels(2, 1);
        let mut clock = Animator::default();
        let sizes: Vec<usize> = clock.tick(&[anim])[0].1.iter().map(Vec::len).collect();
        assert_eq!(sizes, [2 * 2 * 4, 4]);
    }

    #[test]
    fn invalid_or_missing_animation_is_static() {
        assert!(Animation::new(1, &strip(2), 2, b"{}").is_none());
        assert!(Animation::new(1, &strip(2), 2, br#"{"animation":{"frametime":0}}"#).is_none());
        assert!(Animation::new(1, &strip(1), 2, br#"{"animation":{}}"#).is_none());
    }
}
