//! Models of the wolf and the ocelot, ported from `ModelWolf` and `ModelOcelot` (box layout, texture
//! offsets, `setLivingAnimations` and `setRotationAngles`), plus the `LayerWolfCollar` pass. Every
//! constant was read from the 1.8.9 classes with `harness/mobmodels.sh`, which runs both models in
//! each pose.
//!
//! Not ported: the wet shading (it needs client-side water tracking), the begging head tilt (`getInterestedAngle` eases a timer), and the cat sit-on-block pose is the sit flag only.

use super::mobs::{head_rot, model, Layer, FLEECE};
use super::{cos, EntityDraw, Model, Part, Skin};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

pub(super) const WOLF: u8 = 95;
pub(super) const OCELOT: u8 = 98;

/// Bits of metadata byte 16 of a tameable mob (`EntityTameable`).
const SITTING: u8 = 1;
const ANGRY: u8 = 2;
pub(super) const TAMED: u8 = 4;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    Some(match id {
        WOLF => wolf(d),
        OCELOT => ocelot(d),
        _ => return None,
    })
}

/// `LayerWolfCollar`: the collar texture over a tame wolf, tinted with the dye of metadata 20.
pub(super) fn layers(d: &EntityDraw, m: &Model) -> Vec<Layer> {
    if d.class != super::Class::Mob(WOLF) || d.look.variant & TAMED == 0 || d.invisible {
        return Vec::new();
    }
    vec![Layer {
        skin: Skin::WolfCollar,
        // The byte is a dye damage value (15 for white); `FLEECE` is indexed by dye metadata.
        tint: FLEECE[15 - usize::from(d.look.collar & 15)],
        parts: m.parts.clone(),
        bright: false,
    }]
}

/// `EntityWolf.getTailRotation`: the tail angle that `RenderWolf.handleRotationFloat` passes as age.
fn wolf_tail_angle(d: &EntityDraw) -> f32 {
    let flags = d.look.variant;
    if flags & ANGRY != 0 {
        1.539_380_4
    } else if flags & TAMED != 0 {
        (0.55 - (20.0 - d.look.health) * 0.02) * PI
    } else {
        PI / 5.0
    }
}

/// `RenderWolf.doRender`: a wet wolf is drawn with the color `getBrightness * getShadingWhileWet`; `light` is
/// the `[sky, block]` level at the wolf. `None` for every other entity and for a dry wolf.
pub(super) fn wet_color(d: &EntityDraw, light: [f32; 2]) -> Option<f32> {
    let shade = d
        .look
        .wolf_wet
        .filter(|_| d.class == super::Class::Mob(WOLF))?;
    let level = (light[0].max(light[1]) * 15.0).round() as i32;
    Some(super::shadow::light_brightness(level) * shade)
}

/// The color factor of the main layer: [`wet_color`] as [`super::build`] stored it, 1 otherwise.
pub(super) fn wet_factor(d: &EntityDraw) -> f32 {
    d.look
        .wolf_wet
        .filter(|_| d.class == super::Class::Mob(WOLF))
        .unwrap_or(1.0)
}

/// `ModelWolf`: head with two ears and a nose, body, mane, four legs and the tail.
fn wolf(d: &EntityDraw) -> Model {
    let flags = d.look.variant;
    let [head_roll, mane_roll, body_roll, tail_roll] = d.look.wolf_shake;
    let hr = head_rot(d);
    let hr = [hr[0], hr[1], d.look.wolf_interested + head_roll];
    let (f, amount) = (d.limb_swing, d.limb_swing_amount);
    let walk = |phase: f32| cos(f * 0.6662 + phase) * 1.4 * amount;
    let head_at = [-1.0, 13.5, -7.0];
    let leg = |pivot: [f32; 3], rx: f32| {
        Part::new(pivot, [-1.0, 0.0, -1.0], [2.0, 8.0, 2.0], (0.0, 18.0)).rot([rx, 0.0, 0.0])
    };
    let sitting = flags & SITTING != 0;
    // Pivots and angles of body, mane, tail and the four legs; a sitting wolf crouches.
    let (body, mane, tail, legs, rx) = if sitting {
        (
            ([0.0, 18.0, 0.0], PI / 4.0),
            ([-1.0, 16.0, -3.0], PI * 2.0 / 5.0),
            [-1.0, 21.0, 6.0],
            [
                [-2.5, 22.0, 2.0],
                [0.5, 22.0, 2.0],
                [-2.49, 17.0, -4.0],
                [0.51, 17.0, -4.0],
            ],
            [PI * 3.0 / 2.0, PI * 3.0 / 2.0, 5.811_947, 5.811_947],
        )
    } else {
        (
            ([0.0, 14.0, 2.0], FRAC_PI_2),
            ([-1.0, 14.0, -3.0], FRAC_PI_2),
            [-1.0, 12.0, 8.0],
            [
                [-2.5, 16.0, 7.0],
                [0.5, 16.0, 7.0],
                [-2.5, 16.0, -4.0],
                [0.5, 16.0, -4.0],
            ],
            [walk(0.0), walk(PI), walk(PI), walk(0.0)],
        )
    };
    let tail_sway = if flags & ANGRY != 0 { 0.0 } else { walk(0.0) };
    let parts = vec![
        Part::new(head_at, [-3.0, -3.0, -2.0], [6.0, 6.0, 4.0], (0.0, 0.0)).rot(hr),
        Part::new(head_at, [-3.0, -5.0, 0.0], [2.0, 2.0, 1.0], (16.0, 14.0)).rot(hr),
        Part::new(head_at, [1.0, -5.0, 0.0], [2.0, 2.0, 1.0], (16.0, 14.0)).rot(hr),
        Part::new(head_at, [-1.5, 0.0, -5.0], [3.0, 3.0, 4.0], (0.0, 10.0)).rot(hr),
        Part::new(body.0, [-4.0, -2.0, -3.0], [6.0, 9.0, 6.0], (18.0, 14.0))
            .rot([body.1, 0.0, body_roll]),
        Part::new(mane.0, [-4.0, -3.0, -3.0], [8.0, 6.0, 7.0], (21.0, 0.0))
            .rot([mane.1, 0.0, mane_roll]),
        leg(legs[0], rx[0]),
        leg(legs[1], rx[1]),
        leg(legs[2], rx[2]),
        leg(legs[3], rx[3]),
        Part::new(tail, [-1.0, 0.0, -1.0], [2.0, 8.0, 2.0], (9.0, 18.0)).rot([
            wolf_tail_angle(d),
            tail_sway,
            tail_roll,
        ]),
    ];
    let skin = if flags & TAMED != 0 {
        Skin::WolfTame
    } else if flags & ANGRY != 0 {
        Skin::WolfAngry
    } else {
        Skin::Wolf
    };
    model(skin, parts)
}

/// `OcelotModel` poses (`state` of `ModelOcelot`): 0 sneaking, 1 walking, 2 sprinting, 3 sitting.
#[derive(Clone, Copy, PartialEq)]
enum Stance {
    Sneak,
    Walk,
    Sprint,
    Sit,
}

fn stance(d: &EntityDraw) -> Stance {
    if d.sneaking {
        Stance::Sneak
    } else if d.look.sprinting {
        Stance::Sprint
    } else if d.look.variant & SITTING != 0 {
        Stance::Sit
    } else {
        Stance::Walk
    }
}

/// `ModelOcelot`: head with nose and ears, body, two tail segments and four legs.
fn ocelot(d: &EntityDraw) -> Model {
    let stance = stance(d);
    let hr = head_rot(d);
    let (f, amount) = (d.limb_swing, d.limb_swing_amount);
    let step = |phase: f32| cos(f * 0.6662 + phase) * amount;
    let (mut body, mut head) = ([0.0, 12.0, -10.0], [0.0, 15.0, -9.0]);
    let (mut tail, mut tail2) = ([0.0, 15.0, 8.0], [0.0, 20.0, 14.0]);
    let mut front = [[1.2, 13.8, -5.0], [-1.2, 13.8, -5.0]];
    let mut back = [[1.1, 18.0, 5.0], [-1.1, 18.0, 5.0]];
    let (mut body_rx, mut tail_rx, mut tail2_rx) = (FRAC_PI_2, 0.9, 1.727_876_1);
    // Order of the legs: back left, back right, front left, front right.
    let mut legs = [step(0.0), step(PI), step(PI), step(0.0)];
    match stance {
        Stance::Sneak => {
            body[1] += 1.0;
            head[1] += 2.0;
            tail[1] += 1.0;
            tail2[1] -= 4.0;
            tail2[2] += 2.0;
            tail_rx = FRAC_PI_2;
            tail2_rx += 0.471_238_94 * cos(f) * amount;
        }
        Stance::Sprint => {
            tail2[1] = tail[1];
            tail2[2] += 2.0;
            tail_rx = FRAC_PI_2;
            legs = [step(0.0), step(0.3), step(PI + 0.3), step(PI)];
            tail2_rx += 0.314_159_27 * cos(f) * amount;
        }
        Stance::Walk => tail2_rx += FRAC_PI_4 * cos(f) * amount,
        Stance::Sit => {
            body_rx = FRAC_PI_4;
            body[1] -= 4.0;
            body[2] += 5.0;
            head[1] -= 3.3;
            head[2] += 1.0;
            tail[1] += 8.0;
            tail[2] -= 2.0;
            tail2[1] += 2.0;
            tail2[2] -= 0.8;
            tail_rx = 1.727_876_1;
            tail2_rx = 2.670_354;
            legs = [-FRAC_PI_2, -FRAC_PI_2, -0.157_079_64, -0.157_079_64];
            for f in &mut front {
                f[1] = 15.8;
                f[2] = -7.0;
            }
            for b in &mut back {
                b[1] = 21.0;
                b[2] = 1.0;
            }
        }
    }
    let cat = |pivot: [f32; 3], min: [f32; 3], size: [f32; 3], uv: (f32, f32), rx: f32| {
        Part::new(pivot, min, size, uv).rot([rx, 0.0, 0.0])
    };
    let back_leg = |p: [f32; 3], rx| cat(p, [-1.0, 0.0, 1.0], [2.0, 6.0, 2.0], (8.0, 13.0), rx);
    let front_leg = |p: [f32; 3], rx| cat(p, [-1.0, 0.0, 0.0], [2.0, 10.0, 2.0], (40.0, 0.0), rx);
    let tail_box = |p: [f32; 3], uv, rx| cat(p, [-0.5, 0.0, 0.0], [1.0, 8.0, 1.0], uv, rx);
    let parts = vec![
        Part::new(head, [-2.5, -2.0, -3.0], [5.0, 4.0, 5.0], (0.0, 0.0)).rot(hr),
        Part::new(head, [-1.5, 0.0, -4.0], [3.0, 2.0, 2.0], (0.0, 24.0)).rot(hr),
        Part::new(head, [-2.0, -3.0, 0.0], [1.0, 1.0, 2.0], (0.0, 10.0)).rot(hr),
        Part::new(head, [1.0, -3.0, 0.0], [1.0, 1.0, 2.0], (6.0, 10.0)).rot(hr),
        cat(
            body,
            [-2.0, 3.0, -8.0],
            [4.0, 16.0, 6.0],
            (20.0, 0.0),
            body_rx,
        ),
        tail_box(tail, (0.0, 15.0), tail_rx),
        tail_box(tail2, (4.0, 15.0), tail2_rx),
        back_leg(back[0], legs[0]),
        back_leg(back[1], legs[1]),
        front_leg(front[0], legs[2]),
        front_leg(front[1], legs[3]),
    ];
    // `RenderOcelot.getEntityTexture` by `getTameSkin`; any other value is the wild ocelot.
    let skin = match d.look.collar {
        1 => Skin::CatBlack,
        2 => Skin::CatRed,
        3 => Skin::CatSiamese,
        _ => Skin::Ocelot,
    };
    model(skin, parts)
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    fn mob(id: u8) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(id);
        // The pose of the oracle run: swing 1.3, amount 0.6, head yaw 30 and pitch 15 degrees.
        d.yaw = 0.0;
        d.head_yaw = 30.0;
        d.pitch = 15.0;
        d.limb_swing = 1.3;
        d.limb_swing_amount = 0.6;
        d
    }

    fn rots(m: &Model) -> Vec<[f32; 3]> {
        m.parts.iter().map(|p| p.rot).collect()
    }

    fn near(a: f32, b: f32) -> bool {
        // The oracle reads `MathHelper.cos`, a 65536-entry table, which is off by up to 5e-5 from `f32::cos`.
        (a - b).abs() < 1e-4
    }

    #[test]
    fn wild_wolf_walking_matches_the_oracle() {
        let m = build(&mob(WOLF), WOLF).unwrap();
        assert_eq!(m.skin, Skin::Wolf);
        assert_eq!(m.parts.len(), 11);
        let r = rots(&m);
        assert!(near(r[0][0], 0.261_799_4) && near(r[0][1], std::f32::consts::FRAC_PI_6));
        // Body and mane lie flat; the mane pivot is 5 pixels ahead of the body.
        assert_eq!(m.parts[4].pivot, [0.0, 14.0, 2.0]);
        assert_eq!(m.parts[5].pivot, [-1.0, 14.0, -3.0]);
        assert!(near(r[4][0], FRAC_PI_2) && near(r[5][0], FRAC_PI_2));
        let legs: Vec<f32> = r[6..10].iter().map(|v| v[0]).collect();
        for (got, want) in legs
            .iter()
            .zip([0.544_200_2, -0.544_200_2, -0.544_200_2, 0.544_200_2])
        {
            assert!(near(*got, want), "{legs:?}");
        }
        // The tail pitch is the wild 0.62831855 and it sways with the stride.
        assert!(near(r[10][0], 0.628_318_55) && near(r[10][1], 0.544_200_2));
    }

    #[test]
    fn begging_rolls_the_head_and_a_wet_wolf_is_shaded() {
        let mut d = mob(WOLF);
        let dry = build(&d, WOLF).unwrap();
        d.look.wolf_interested = 0.348_716_83;
        d.look.wolf_shake[0] = 0.1;
        let m = build(&d, WOLF).unwrap();
        // Ears and nose roll with the head.
        assert!((0..4).all(|i| near(m.parts[i].rot[2], 0.448_716_83)));
        assert_eq!(dry.parts[0].rot[2], 0.0);
        assert_eq!(wet_color(&d, [1.0, 1.0]), None);
        d.look.wolf_wet = Some(0.853_125);
        assert!(near(wet_color(&d, [1.0, 0.0]).unwrap(), 0.853_125));
        // Light level 7 reads 0.179 in the brightness table.
        assert!(wet_color(&d, [0.0, 7.0 / 15.0]).unwrap() < 0.2);
        d.class = Class::Mob(98);
        assert_eq!(wet_color(&d, [1.0, 1.0]), None);
    }

    #[test]
    fn angry_wolf_holds_its_tail_up_and_still() {
        let mut d = mob(WOLF);
        d.look.variant = ANGRY;
        let m = build(&d, WOLF).unwrap();
        assert_eq!(m.skin, Skin::WolfAngry);
        assert!(near(m.parts[10].rot[0], 1.539_380_4) && m.parts[10].rot[1] == 0.0);
    }

    #[test]
    fn sitting_tame_wolf_matches_the_oracle_and_the_tail_follows_health() {
        let mut d = mob(WOLF);
        d.look.variant = SITTING | TAMED;
        let m = build(&d, WOLF).unwrap();
        assert_eq!(m.skin, Skin::WolfTame);
        assert_eq!(
            (m.parts[4].pivot, m.parts[4].rot[0]),
            ([0.0, 18.0, 0.0], FRAC_PI_4)
        );
        assert_eq!(m.parts[5].pivot, [-1.0, 16.0, -3.0]);
        assert!(near(m.parts[5].rot[0], 1.256_637_1));
        assert_eq!(m.parts[6].pivot, [-2.5, 22.0, 2.0]);
        assert!(near(m.parts[6].rot[0], 4.712_389) && near(m.parts[8].rot[0], 5.811_947));
        assert_eq!(m.parts[8].pivot, [-2.49, 17.0, -4.0]);
        assert_eq!(m.parts[10].pivot, [-1.0, 21.0, 6.0]);
        // Health 1 (the metadata default) reads 0.5340708 in the oracle; full health is 0.55 pi.
        assert!(near(m.parts[10].rot[0], 0.534_070_8));
        d.look.health = 20.0;
        assert!(near(build(&d, WOLF).unwrap().parts[10].rot[0], 0.55 * PI));
    }

    #[test]
    fn shaking_wolf_rolls_head_mane_body_and_tail_like_the_oracle() {
        let mut d = mob(WOLF);
        d.look.variant = TAMED;
        // `getShakeAngle` at partial 1.0 for offsets 0, -0.08, -0.16, -0.2 in the oracle run.
        d.look.wolf_shake = [0.158_727_78, 0.426_234_28, -0.116_751_96, -0.353_422_64];
        let m = build(&d, WOLF).unwrap();
        for head in &m.parts[0..4] {
            assert!(near(head.rot[2], 0.158_727_78));
        }
        assert!(near(m.parts[5].rot[2], 0.426_234_28));
        assert!(near(m.parts[4].rot[2], -0.116_751_96));
        assert!(near(m.parts[10].rot[2], -0.353_422_64));
    }

    #[test]
    fn only_a_visible_tame_wolf_wears_its_collar_in_the_dye_color() {
        let mut d = mob(WOLF);
        let m = build(&d, WOLF).unwrap();
        assert!(layers(&d, &m).is_empty());
        d.look.variant = TAMED;
        // Dye damage 1 is red (0.6, 0.2, 0.2 in the oracle table).
        d.look.collar = 1;
        let collar = layers(&d, &m);
        assert_eq!(collar[0].skin, Skin::WolfCollar);
        assert_eq!(collar[0].tint, [0.6, 0.2, 0.2]);
        d.look.collar = 15;
        assert_eq!(layers(&d, &m)[0].tint, [1.0; 3]);
        d.invisible = true;
        assert!(layers(&d, &m).is_empty());
    }

    #[test]
    fn ocelot_poses_match_the_oracle() {
        let walk = build(&mob(OCELOT), OCELOT).unwrap();
        assert_eq!(walk.parts.len(), 11);
        let r = rots(&walk);
        assert!(near(r[4][0], FRAC_PI_2) && near(r[5][0], 0.9) && near(r[6][0], 1.853_953_4));
        for (i, want) in [
            (7, 0.388_714_43),
            (8, -0.388_714_43),
            (9, -0.388_714_43),
            (10, 0.388_714_43),
        ] {
            assert!(near(r[i][0], want), "leg {i}: {}", r[i][0]);
        }

        let mut d = mob(OCELOT);
        d.sneaking = true;
        let sneak = build(&d, OCELOT).unwrap();
        assert_eq!(sneak.parts[0].pivot, [0.0, 17.0, -9.0]);
        assert_eq!(sneak.parts[4].pivot, [0.0, 13.0, -10.0]);
        assert_eq!(sneak.parts[6].pivot, [0.0, 16.0, 16.0]);
        assert!(near(sneak.parts[5].rot[0], FRAC_PI_2) && near(sneak.parts[6].rot[0], 1.803_522_5));

        d.sneaking = false;
        d.look.sprinting = true;
        let run = build(&d, OCELOT).unwrap();
        assert_eq!(run.parts[6].pivot, [0.0, 15.0, 16.0]);
        assert!(near(run.parts[6].rot[0], 1.778_307));
        let legs: Vec<f32> = run.parts[7..].iter().map(|p| p.rot[0]).collect();
        for (got, want) in
            legs.iter()
                .zip([0.388_714_43, 0.236_289_49, -0.236_289_49, -0.388_714_43])
        {
            assert!(near(*got, want), "{legs:?}");
        }
    }

    #[test]
    fn sitting_ocelot_matches_the_oracle_and_sneaking_wins_over_sitting() {
        let mut d = mob(OCELOT);
        d.look.variant = SITTING;
        let sit = build(&d, OCELOT).unwrap();
        assert_eq!(
            (sit.parts[0].pivot, sit.parts[4].pivot),
            ([0.0, 11.7, -8.0], [0.0, 8.0, -5.0])
        );
        assert_eq!(sit.parts[7].pivot, [1.1, 21.0, 1.0]);
        assert_eq!(sit.parts[9].pivot, [1.2, 15.8, -7.0]);
        assert!(near(sit.parts[4].rot[0], FRAC_PI_4) && near(sit.parts[6].rot[0], 2.670_354));
        assert!(near(sit.parts[7].rot[0], -FRAC_PI_2) && near(sit.parts[9].rot[0], -0.157_079_64));
        d.sneaking = true;
        assert_eq!(
            build(&d, OCELOT).unwrap().parts[4].pivot,
            [0.0, 13.0, -10.0]
        );
    }

    #[test]
    fn the_cat_type_picks_the_skin_and_a_tame_cat_draws_at_four_fifths() {
        let skin = |kind: u8| {
            let mut d = mob(OCELOT);
            d.look.collar = kind;
            build(&d, OCELOT).unwrap().skin
        };
        assert_eq!(
            [0, 1, 2, 3].map(skin),
            [Skin::Ocelot, Skin::CatBlack, Skin::CatRed, Skin::CatSiamese]
        );
        let mut d = mob(OCELOT);
        assert_eq!(super::super::model_scale(&d), [1.0; 2]);
        d.look.variant = TAMED;
        assert_eq!(super::super::model_scale(&d), [0.8; 2]);
    }

    #[test]
    fn a_baby_wolf_moves_its_head_group_and_halves_the_rest() {
        let mut d = mob(WOLF);
        d.child = true;
        let m = drawn(&d);
        // Head, two ears and nose share the (0, 5, 2) move; body, mane, legs and tail halve.
        assert!(m.parts[..4]
            .iter()
            .all(|p| p.scale == 1.0 && p.shift == [0.0, 5.0, 2.0]));
        assert!(m.parts[4..]
            .iter()
            .all(|p| p.scale == 0.5 && p.shift == [0.0, 24.0, 0.0]));
        d.class = Class::Mob(OCELOT);
        let m = drawn(&d);
        assert!(m.parts[..4]
            .iter()
            .all(|p| p.scale == 0.75 && p.shift == [0.0, 10.0, 4.0]));
        assert!(m.parts[4..].iter().all(|p| p.scale == 0.5));
    }
}
