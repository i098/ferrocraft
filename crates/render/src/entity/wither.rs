//! The wither (mob 64), ported from the 1.8.9 `WitherModel` (`ModelWither`), `WitherRenderer`
//! (`RenderWither`) and `WitherArmorLayer` (`LayerWitherAura`).
//!
//! shortcut: the two side heads face along the body (`EntityWither.func_82207_a` with no target);
//! vanilla turns them to the targets of metadata 18 and 19, which the client does not track yet.

use super::mobs::{head_rot, model};
use super::{cos, emit_part, skin_dims, EntityDraw, Model, Part, Skin, Xf};
use crate::mesh::MeshData;
use std::f32::consts::PI;

pub(super) const WITHER: u8 = 64;
/// `EntityWither.getMaxHealth`; the armor aura shows from half of it down (`isArmored`).
pub(super) const MAX_HEALTH: f32 = 300.0;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == WITHER).then(|| wither(d))
}

/// `RenderWither.getEntityTexture`: the blue skin while the spawn charge runs, flashing in the last 80 ticks.
fn skin(invul: i32) -> Skin {
    if invul > 0 && (invul > 80 || invul / 5 % 2 != 1) {
        Skin::WitherInvulnerable
    } else {
        Skin::Wither
    }
}

/// `RenderWither.preRenderCallback`: scale 2, shrunk to 1.75 over the 220 ticks of the spawn charge.
pub(super) fn scale(d: &EntityDraw) -> f32 {
    let invul = d.look.wither_invul;
    if invul > 0 {
        2.0 - (invul as f32 - d.look.partial) / 220.0 * 0.5
    } else {
        2.0
    }
}

/// `ModelWither.setRotationAngles`: the spine tilt and the tail it carries, from the flap `cos(age / 10)`.
fn spine_rot(age: f32) -> f32 {
    (0.065 + 0.05 * cos(age * 0.1)) * PI
}

/// The shoulder bar, the spine with its three ribs, the tail, then the three heads.
fn parts(d: &EntityDraw) -> Vec<Part> {
    let spine = spine_rot(d.age);
    let spine_pivot = [-2.0, 6.9, -0.5];
    let tail_pivot = [-2.0, 6.9 + cos(spine) * 10.0, -0.5 + spine.sin() * 10.0];
    let rib = |y| {
        Part::new(spine_pivot, [-4.0, y, 0.5], [11.0, 2.0, 2.0], (24.0, 22.0))
            .rot([spine, 0.0, 0.0])
    };
    vec![
        Part::new([0.0; 3], [-10.0, 3.9, -0.5], [20.0, 3.0, 3.0], (0.0, 16.0)),
        Part::new(spine_pivot, [0.0; 3], [3.0, 10.0, 3.0], (0.0, 22.0)).rot([spine, 0.0, 0.0]),
        rib(1.5),
        rib(4.0),
        rib(6.5),
        Part::new(tail_pivot, [0.0; 3], [3.0, 6.0, 3.0], (12.0, 22.0)).rot([
            (0.265 + 0.1 * cos(d.age * 0.1)) * PI,
            0.0,
            0.0,
        ]),
        Part::new([0.0; 3], [-4.0, -4.0, -4.0], [8.0; 3], (0.0, 0.0)).rot(head_rot(d)),
        Part::new([-8.0, 4.0, 0.0], [-4.0, -4.0, -4.0], [6.0; 3], (32.0, 0.0)),
        Part::new([10.0, 4.0, 0.0], [-4.0, -4.0, -4.0], [6.0; 3], (32.0, 0.0)),
    ]
}

fn wither(d: &EntityDraw) -> Model {
    model(skin(d.look.wither_invul), parts(d))
}

/// `LayerWitherAura`: below half health the model draws again, grown by half a pixel, over the
/// scrolling armor texture. Vanilla blends it `ONE, ONE` at color 0.5 with the light off.
pub(super) fn aura(out: &mut MeshData, d: &EntityDraw) {
    if d.class != super::Class::Mob(WITHER) || d.look.health > MAX_HEALTH / 2.0 {
        return;
    }
    let xf = Xf::new(d, false);
    let start = out.vertices.len();
    for part in parts(d) {
        emit_part(out, &xf, &part, 0.5, [1.0; 3], 0.0);
    }
    // The model declares a 64x64 texture, the skin atlas is larger; the texture matrix then
    // translates by `(3 cos(0.02 t), 0.01 t)`.
    let [sw, sh] = skin_dims();
    let (du, dv) = (cos(d.age * 0.02) * 3.0, d.age * 0.01);
    for v in &mut out.vertices[start..] {
        v.uv = [v.uv[0] * sw / 64.0 + du, v.uv[1] * sh / 64.0 + dv];
        (v.color, v.light) = ([0.5, 0.5, 0.5, 1.0], [1.0; 2]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    fn wither_draw() -> EntityDraw {
        EntityDraw {
            class: Class::Mob(WITHER),
            ..steve()
        }
    }

    #[test]
    fn the_spine_and_tail_tilt_follow_the_vanilla_formulas_at_three_ages() {
        for (age, spine, tail) in [
            (0.0f32, 0.115, 0.365),
            (15.7, 0.065, 0.265),
            (31.4, 0.015, 0.165),
        ] {
            let mut d = wither_draw();
            d.age = age;
            let m = drawn(&d);
            assert!(
                (m.parts[1].rot[0] / PI - spine).abs() < 1e-3,
                "spine at {age}"
            );
            assert!(
                (m.parts[5].rot[0] / PI - tail).abs() < 1e-3,
                "tail at {age}"
            );
            // The tail hangs from the spine's end: `6.9 + cos(rx) * 10`, `-0.5 + sin(rx) * 10`.
            let rx = m.parts[1].rot[0];
            assert!((m.parts[5].pivot[1] - (6.9 + rx.cos() * 10.0)).abs() < 1e-4);
            assert!((m.parts[5].pivot[2] - (-0.5 + rx.sin() * 10.0)).abs() < 1e-4);
        }
    }

    #[test]
    fn the_tail_matches_the_real_model_after_set_rotation_angles() {
        // Printed by `harness/mobmodels.sh` from `WitherModel.setupAnimation` at these ages. The real
        // model reads the `MathHelper` sine table, which is off from `f32::cos` by up to 1e-4 rad.
        for (age, pivot, rot) in [
            (15.7f32, [-2.0, 16.692_019, 1.528_886_8], 0.832_793_06),
            (31.4, [-2.0, 16.888_922, -0.029_433_489], 0.518_363_2),
        ] {
            let mut d = wither_draw();
            d.age = age;
            let tail = &drawn(&d).parts[5];
            assert!(
                (0..3).all(|i| (tail.pivot[i] - pivot[i]).abs() < 2e-3),
                "pivot at {age}"
            );
            assert!((tail.rot[0] - rot).abs() < 2e-3, "rot at {age}");
        }
    }

    #[test]
    fn the_spawn_charge_flashes_between_the_skins_and_grows_the_wither() {
        assert_eq!(skin(0), Skin::Wither);
        assert_eq!(skin(200), Skin::WitherInvulnerable);
        assert_eq!(skin(80), Skin::WitherInvulnerable);
        assert_eq!(skin(79), Skin::Wither);
        assert_eq!(skin(70), Skin::WitherInvulnerable);
        let mut d = wither_draw();
        assert_eq!(scale(&d), 2.0);
        d.look.wither_invul = 220;
        assert!((scale(&d) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn the_armor_aura_shows_from_half_health_down() {
        let mut d = wither_draw();
        let count = |d: &EntityDraw| {
            let mut out = MeshData::default();
            aura(&mut out, d);
            out.vertices.len()
        };
        d.look.health = 150.1;
        assert_eq!(count(&d), 0);
        d.look.health = 150.0;
        assert_eq!(count(&d), 9 * 24);
    }
}
