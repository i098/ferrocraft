//! Entity shadows: `EntityRenderer.renderShadow` and `renderShadowOnBlock` of 1.8.9, with the
//! `shadowSize` and `shadowOpaque` each renderer class sets in its constructor.

use super::{Class, EntityDraw};
use crate::blocks::{look, Kind};
use crate::mesh::{MeshData, Vertex};

pub const TEXTURE: &str = "textures/misc/shadow.png";

/// One shadow to cast: feet position, `shadowSize` (already halved for a child) and `shadowOpaque`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caster {
    pub pos: [f64; 3],
    pub size: f32,
    pub opaque: f32,
}

/// `shadowSize` and `shadowOpaque` of the renderer of `d`; 0 where the renderer sets no shadow.
fn shadow_of(d: &EntityDraw) -> (f32, f32) {
    match d.class {
        Class::Player { .. } => (0.5, 1.0),
        // `ExperienceOrbRenderer` and `ItemEntityRenderer`.
        Class::Orb | Class::Object(2) => (0.15, 0.75),
        // Boat, minecart, primed TNT, ender crystal, falling block.
        Class::Object(1 | 10 | 50 | 51 | 70) => (0.5, 1.0),
        Class::Mob(id) => {
            let size = match id {
                52 | 64 => 1.0,
                53 => 3.0,
                54 | 56 | 57 | 58 | 61 | 63 | 66 | 68 | 95 | 97 | 99 => 0.5,
                55 => 0.25 * (d.size.0 / 0.51).round(),
                62 => 0.25,
                120 => {
                    if d.child {
                        0.25
                    } else {
                        0.5
                    }
                }
                59 | 90..=92 | 94 | 96 => 0.7,
                60 | 67 | 93 | 101 => 0.3,
                65 => 0.25,
                98 => 0.4,
                100 => 0.75,
                // Unmodelled mobs: the renderer default.
                _ => 0.5,
            };
            // `renderShadow`: `shadowSize * getShadowScale()`, halved for a baby.
            (if d.child { size * 0.5 } else { size }, 1.0)
        }
        _ => (0.0, 1.0),
    }
}

/// `doRenderShadowAndFire`: a visible entity whose renderer has a shadow size casts one.
pub fn caster(d: &EntityDraw) -> Option<Caster> {
    let (size, opaque) = shadow_of(d);
    (size > 0.0 && !d.invisible).then_some(Caster {
        pos: d.pos,
        size,
        opaque,
    })
}

/// `World.calculateSkylightSubtracted(1.0)`: how many sky levels the time of day and the weather remove.
pub fn skylight_subtracted(world_time: i64, rain: f32, thunder: f32) -> i32 {
    let angle = crate::sky::celestial_angle(world_time, 1.0);
    let mut f = (1.0 - ((angle * std::f32::consts::TAU).cos() * 2.0 + 0.5)).clamp(0.0, 1.0);
    f = 1.0 - f;
    f *= 1.0 - rain * 5.0 / 16.0;
    f *= 1.0 - thunder * 5.0 / 16.0;
    ((1.0 - f) * 11.0) as i32
}

/// `OverworldProvider.lightBrightnessTable` at a light level `0..=15`.
fn brightness(level: i32) -> f64 {
    let f = 1.0 - level.clamp(0, 15) as f32 / 15.0;
    f64::from((1.0 - f) / (f * 3.0 + 1.0))
}

/// The alpha of the shadow on the top face of the block at height `block_y`, or `None` below zero:
/// it fades with the entity's height above the block and with the light of the block above it.
pub fn alpha(shadow_alpha: f32, entity_y: f64, block_y: i32, light: i32) -> Option<f32> {
    let a =
        (f64::from(shadow_alpha) - (entity_y - f64::from(block_y)) / 2.0) * 0.5 * brightness(light);
    (a >= 0.0).then_some(a.min(1.0) as f32)
}

/// `Block.isFullCube`: the opaque cubes, and leaves and ice, which keep the default of the base class.
fn full_cube(state: u16) -> bool {
    look(state).kind == Kind::Opaque || matches!(state >> 4, 18 | 79 | 161 | 174)
}

/// Meshes the shadows of `casters` in world space. `view` is the view entity's feet position, which sets the
/// distance fade; `block(x, y, z)` is a block state; `light(x, y, z)` is `[sky, block]` light `0..=15`
/// without the time-of-day subtraction; `subtracted` is [`skylight_subtracted`].
pub fn build(
    casters: &[Caster],
    view: [f64; 3],
    block: impl Fn(i32, i32, i32) -> u16,
    light: impl Fn(i32, i32, i32) -> [i32; 2],
    subtracted: i32,
) -> MeshData {
    let mut mesh = MeshData::default();
    for c in casters {
        // `EntityRenderer.squaredDistanceToCamera` removes the fade over 16 blocks.
        let dist: f64 = (0..3).map(|i| (view[i] - c.pos[i]).powi(2)).sum();
        let shadow_alpha = ((1.0 - dist / 256.0) * f64::from(c.opaque)) as f32;
        if shadow_alpha <= 0.0 {
            continue;
        }
        let [x, y, z] = c.pos;
        let s = f64::from(c.size);
        let (x0, x1) = ((x - s).floor() as i32, (x + s).floor() as i32);
        let (y0, y1) = ((y - s).floor() as i32, y.floor() as i32);
        let (z0, z1) = ((z - s).floor() as i32, (z + s).floor() as i32);
        for by in y0..=y1 {
            for bz in z0..=z1 {
                for bx in x0..=x1 {
                    let [sky, lit] = light(bx, by, bz);
                    let level = (sky - subtracted).max(lit);
                    // Both the light gate and the brightness table use the darkened sky light.
                    if level <= 3 || !full_cube(block(bx, by - 1, bz)) {
                        continue;
                    }
                    let Some(a) = alpha(shadow_alpha, y, by, level) else {
                        continue;
                    };
                    // A full cube spans 0 to 1 on each axis.
                    let (px0, px1) = (f64::from(bx), f64::from(bx) + 1.0);
                    let (pz0, pz1) = (f64::from(bz), f64::from(bz) + 1.0);
                    let py = f64::from(by) + 0.015625;
                    let u = |edge: f64| ((x - edge) / 2.0 / s + 0.5) as f32;
                    let v = |edge: f64| ((z - edge) / 2.0 / s + 0.5) as f32;
                    let base = mesh.vertices.len() as u32;
                    for (px, pz) in [(px0, pz0), (px0, pz1), (px1, pz1), (px1, pz0)] {
                        mesh.vertices.push(Vertex {
                            pos: [px as f32, py as f32, pz as f32],
                            uv: [u(px), v(pz)],
                            color: [1.0, 1.0, 1.0, a],
                            light: [1.0; 2],
                        });
                    }
                    mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
                }
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_fades_with_height_and_light() {
        // Standing on the block: (1 - 0) * 0.5 * 1 in full light.
        assert!((alpha(1.0, 64.0, 64, 15).unwrap() - 0.5).abs() < 1e-6);
        // One block up: (1 - 0.5) * 0.5.
        assert!((alpha(1.0, 65.0, 64, 15).unwrap() - 0.25).abs() < 1e-6);
        // Light 7 is darker: half of the brightness table value at 7.
        let table = (7.0 / 15.0) / ((1.0 - 7.0 / 15.0) * 3.0 + 1.0);
        let dim = alpha(1.0, 64.0, 64, 7).unwrap();
        assert!((f64::from(dim) - 0.5 * table).abs() < 1e-6);
        // Past the end of the fade the shadow is gone.
        assert_eq!(alpha(1.0, 66.5, 64, 15), None);
        // An item's 0.75 opacity darkens less than a player's 1.0 opacity.
        assert!(alpha(0.75, 64.0, 64, 15).unwrap() < alpha(1.0, 64.0, 64, 15).unwrap());
    }

    #[test]
    fn shadow_lands_on_the_block_below_the_lit_air() {
        let floor = |_: i32, y: i32, _: i32| if y < 64 { 1 << 4 } else { 0 };
        // Inside the stone there is no light, as in the world.
        let light = |_: i32, y: i32, _: i32| if y < 64 { [0, 0] } else { [15, 0] };
        let player = Caster {
            pos: [0.5, 64.0, 0.5],
            size: 0.5,
            opaque: 1.0,
        };
        let lit = build(&[player], [0.5, 64.0, 0.5], floor, light, 0);
        // A size-0.5 player covers blocks 0 and 1 on both axes.
        assert_eq!(lit.indices.len(), 4 * 6);
        assert!(lit.vertices.iter().all(|v| v.pos[1] == 64.015625));
        // The entity stands at the middle of the texture.
        assert!(lit.vertices.iter().any(|v| v.uv == [0.0, 0.0]));
        // Light 3 is too dark for a shadow, and so is a block that is not a full cube.
        let dark = |_: i32, _: i32, _: i32| [3, 0];
        assert!(build(&[player], [0.5, 64.0, 0.5], floor, dark, 0).is_empty());
        let air = |_: i32, _: i32, _: i32| 0;
        assert!(build(&[player], [0.5, 64.0, 0.5], air, light, 0).is_empty());
        // Past 16 blocks the distance fade removes it.
        assert!(build(&[player], [0.5, 64.0, 17.0], floor, light, 0).is_empty());
    }

    #[test]
    fn alpha_matches_the_real_renderer_bytecode() {
        let mut script = String::from("start 0 64 0 0\n");
        let mut expected = Vec::new();
        for opacity in [0.25, 0.75, 1.0] {
            for height in [63.0, 64.0, 64.5, 65.0, 66.5] {
                for light in 0..=15 {
                    script.push_str(&format!("shadow {opacity} {height} 64 {light}\n"));
                    expected.push(alpha(opacity, height, 64, light));
                }
            }
        }
        let output = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(output.len(), expected.len());
        for (line, expected) in output.iter().zip(expected) {
            let vanilla: f64 = line.parse().unwrap();
            let vanilla = (vanilla >= 0.0).then_some(vanilla.min(1.0) as f32);
            assert_eq!(expected, vanilla);
        }
    }

    #[test]
    fn midnight_subtracts_eleven_sky_levels() {
        assert_eq!(skylight_subtracted(6000, 0.0, 0.0), 0);
        assert_eq!(skylight_subtracted(18000, 0.0, 0.0), 11);
        assert!(skylight_subtracted(6000, 1.0, 0.0) > 0);
        assert!(skylight_subtracted(6000, 1.0, 1.0) > skylight_subtracted(6000, 0.0, 1.0));
    }
}
