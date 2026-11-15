//! The rabbit, ported from `ModelRabbit` (box layout, texture offsets, `setRotationAngles` with the
//! jump) and `RenderRabbit.getEntityTexture`. Every constant was read from the 1.8.9 classes with
//! `harness/mobmodels.sh`, which runs the model at rest and at three points of a jump.
//!
//! The jump completion (`EntityRabbit.getJumpCompletion`) comes from the entity: see [`super::Look::jump`].

use super::mobs::{head_rot, model};
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const RABBIT: u8 = 101;

/// The rabbit type of metadata byte 18 that makes the killer bunny.
const KILLER: u8 = 99;

const DEG: f32 = 0.017_453_292;

/// `RenderRabbit.getEntityTexture`: a rabbit named "Toast" wears its own skin, any other one the skin
/// of its type; an unknown type is brown.
fn skin_of(d: &EntityDraw) -> Skin {
    if d.name
        .as_deref()
        .is_some_and(|n| strip_formatting(n) == "Toast")
    {
        return Skin::RabbitToast;
    }
    match d.look.collar {
        1 => Skin::RabbitWhite,
        2 => Skin::RabbitBlack,
        3 => Skin::RabbitWhiteSplotched,
        4 => Skin::RabbitGold,
        5 => Skin::RabbitSalt,
        KILLER => Skin::RabbitCaerbannog,
        _ => Skin::RabbitBrown,
    }
}

/// `Formatting.strip`: drops every `§` and the code character after it.
fn strip_formatting(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == RABBIT).then(|| rabbit(d))
}

/// `ModelRabbit`: head with nose and two ears, body, tail, two haunches, two back feet and two front legs.
fn rabbit(d: &EntityDraw) -> Model {
    let hr = head_rot(d);
    // `jumpRotation = sin(getJumpCompletion(partial) * PI)`.
    let jump = (d.look.jump * PI).sin();
    let head_at = [0.0, 16.0, -1.0];
    let part = |pivot, min, size, uv| Part::new(pivot, min, size, uv).mirrored(true);
    let ear_turn = |dy: f32| [hr[0], hr[1] + dy, 0.0];
    let haunch = (jump * 50.0 - 21.0) * DEG;
    let back = jump * 50.0 * DEG;
    let front = (jump * -40.0 - 11.0) * DEG;
    // The head group comes first: `baby` moves the first four parts.
    let parts = vec![
        part(head_at, [-2.5, -4.0, -5.0], [5.0, 4.0, 5.0], (32.0, 0.0)).rot(hr),
        part(head_at, [-2.5, -9.0, -1.0], [2.0, 5.0, 1.0], (52.0, 0.0)).rot(ear_turn(-15.0 * DEG)),
        part(head_at, [0.5, -9.0, -1.0], [2.0, 5.0, 1.0], (58.0, 0.0)).rot(ear_turn(15.0 * DEG)),
        part(head_at, [-0.5, -2.5, -5.5], [1.0, 1.0, 1.0], (32.0, 9.0)).rot(hr),
        part(
            [3.0, 17.5, 3.7],
            [-1.0, 5.5, -3.7],
            [2.0, 1.0, 7.0],
            (26.0, 24.0),
        )
        .rot([back, 0.0, 0.0]),
        part(
            [-3.0, 17.5, 3.7],
            [-1.0, 5.5, -3.7],
            [2.0, 1.0, 7.0],
            (8.0, 24.0),
        )
        .rot([back, 0.0, 0.0]),
        part(
            [3.0, 17.5, 3.7],
            [-1.0, 0.0, 0.0],
            [2.0, 4.0, 5.0],
            (30.0, 15.0),
        )
        .rot([haunch, 0.0, 0.0]),
        part(
            [-3.0, 17.5, 3.7],
            [-1.0, 0.0, 0.0],
            [2.0, 4.0, 5.0],
            (16.0, 15.0),
        )
        .rot([haunch, 0.0, 0.0]),
        part(
            [0.0, 19.0, 8.0],
            [-3.0, -2.0, -10.0],
            [6.0, 5.0, 10.0],
            (0.0, 0.0),
        )
        .rot([-20.0 * DEG, 0.0, 0.0]),
        part(
            [3.0, 17.0, -1.0],
            [-1.0, 0.0, -1.0],
            [2.0, 7.0, 2.0],
            (8.0, 15.0),
        )
        .rot([front, 0.0, 0.0]),
        part(
            [-3.0, 17.0, -1.0],
            [-1.0, 0.0, -1.0],
            [2.0, 7.0, 2.0],
            (0.0, 15.0),
        )
        .rot([front, 0.0, 0.0]),
        part(
            [0.0, 20.0, 7.0],
            [-1.5, -1.5, 0.0],
            [3.0, 3.0, 2.0],
            (52.0, 6.0),
        )
        .rot([-20.0 * DEG, 0.0, 0.0]),
    ];
    model(skin_of(d), parts)
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;
    use std::f32::consts::{FRAC_PI_4, FRAC_PI_6};

    /// (uv, pivot, min, size) of one box.
    type BoxSpec = ([f32; 2], [f32; 3], [f32; 3], [f32; 3]);

    fn bunny(jump: f32) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(RABBIT);
        // The pose of the oracle run: head yaw 30 and pitch 15 degrees.
        d.head_yaw = 30.0;
        d.pitch = 15.0;
        d.look.jump = jump;
        d
    }

    /// Parts by the order of the oracle dump: back left foot, back right foot, left haunch, right
    /// haunch, body, front left, front right, head, right ear, left ear, tail, nose.
    fn by_dump_order(m: &Model) -> Vec<&Part> {
        [4, 5, 6, 7, 8, 9, 10, 0, 1, 2, 11, 3]
            .iter()
            .map(|&i| &m.parts[i])
            .collect()
    }

    fn assert_rots(d: &EntityDraw, oracle: [[f32; 3]; 12]) {
        let m = drawn(d);
        for (i, (p, want)) in by_dump_order(&m).iter().zip(oracle).enumerate() {
            for a in 0..3 {
                // The oracle reads `MathHelper.sin`, a 65536-entry table, 5e-5 off `f32::sin`.
                assert!(
                    (p.rot[a] - want[a]).abs() < 1e-4,
                    "part {i} axis {a}: {:?} vs {want:?}",
                    p.rot
                );
            }
        }
    }

    #[test]
    fn box_layout_matches_the_oracle() {
        let m = drawn(&bunny(0.0));
        // (uv, pivot, min, size) in oracle dump order; every box is mirrored.
        let want: [BoxSpec; 12] = [
            (
                [26.0, 24.0],
                [3.0, 17.5, 3.7],
                [-1.0, 5.5, -3.7],
                [2.0, 1.0, 7.0],
            ),
            (
                [8.0, 24.0],
                [-3.0, 17.5, 3.7],
                [-1.0, 5.5, -3.7],
                [2.0, 1.0, 7.0],
            ),
            (
                [30.0, 15.0],
                [3.0, 17.5, 3.7],
                [-1.0, 0.0, 0.0],
                [2.0, 4.0, 5.0],
            ),
            (
                [16.0, 15.0],
                [-3.0, 17.5, 3.7],
                [-1.0, 0.0, 0.0],
                [2.0, 4.0, 5.0],
            ),
            (
                [0.0, 0.0],
                [0.0, 19.0, 8.0],
                [-3.0, -2.0, -10.0],
                [6.0, 5.0, 10.0],
            ),
            (
                [8.0, 15.0],
                [3.0, 17.0, -1.0],
                [-1.0, 0.0, -1.0],
                [2.0, 7.0, 2.0],
            ),
            (
                [0.0, 15.0],
                [-3.0, 17.0, -1.0],
                [-1.0, 0.0, -1.0],
                [2.0, 7.0, 2.0],
            ),
            (
                [32.0, 0.0],
                [0.0, 16.0, -1.0],
                [-2.5, -4.0, -5.0],
                [5.0, 4.0, 5.0],
            ),
            (
                [52.0, 0.0],
                [0.0, 16.0, -1.0],
                [-2.5, -9.0, -1.0],
                [2.0, 5.0, 1.0],
            ),
            (
                [58.0, 0.0],
                [0.0, 16.0, -1.0],
                [0.5, -9.0, -1.0],
                [2.0, 5.0, 1.0],
            ),
            (
                [52.0, 6.0],
                [0.0, 20.0, 7.0],
                [-1.5, -1.5, 0.0],
                [3.0, 3.0, 2.0],
            ),
            (
                [32.0, 9.0],
                [0.0, 16.0, -1.0],
                [-0.5, -2.5, -5.5],
                [1.0, 1.0, 1.0],
            ),
        ];
        assert_eq!(m.parts.len(), 12);
        for (p, (uv, pivot, min, size)) in by_dump_order(&m).iter().zip(want) {
            assert_eq!(
                ([p.uv.0, p.uv.1], p.pivot, p.min, p.size),
                (uv, pivot, min, size)
            );
            assert!(p.mirror);
        }
    }

    #[test]
    fn resting_rabbit_matches_the_oracle() {
        let (h, e) = (FRAC_PI_6, FRAC_PI_6 / 2.0);
        assert_rots(
            &bunny(0.0),
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [-0.366_519_15, 0.0, 0.0],
                [-0.366_519_15, 0.0, 0.0],
                [-0.349_065_84, 0.0, 0.0],
                [-0.191_986_22, 0.0, 0.0],
                [-0.191_986_22, 0.0, 0.0],
                [e, h, 0.0],
                [e, e, 0.0],
                [e, FRAC_PI_4, 0.0],
                [-0.349_065_9, 0.0, 0.0],
                [e, h, 0.0],
            ],
        );
    }

    #[test]
    fn jumping_rabbit_matches_the_oracle_at_ticks_2_5_and_8() {
        // Rows are (haunch, back foot, front leg) of the oracle at jump completion 0.2, 0.5 and 0.8.
        for (jump, back, haunch, front) in [
            (0.2, 0.512_898_74, 0.146_379_62, -0.602_305_23),
            (0.5, 0.872_664_6, 0.506_145_5, -0.890_117_9),
            (0.8, 0.512_966_5, 0.146_447_36, -0.602_359_35),
        ] {
            let m = drawn(&bunny(jump));
            for (i, want) in [
                (4, back),
                (5, back),
                (6, haunch),
                (7, haunch),
                (9, front),
                (10, front),
            ] {
                assert!(
                    (m.parts[i].rot[0] - want).abs() < 1e-4,
                    "jump {jump} part {i}"
                );
            }
        }
    }

    #[test]
    fn skin_follows_the_type_byte_and_the_name_toast() {
        let skin = |kind: u8, name: Option<&str>| {
            let mut d = bunny(0.0);
            d.look.collar = kind;
            d.name = name.map(str::to_string);
            drawn(&d).skin
        };
        assert_eq!(skin(0, None), Skin::RabbitBrown);
        assert_eq!(skin(3, None), Skin::RabbitWhiteSplotched);
        assert_eq!(skin(99, None), Skin::RabbitCaerbannog);
        assert_eq!(skin(77, None), Skin::RabbitBrown);
        assert_eq!(skin(2, Some("§aToast")), Skin::RabbitToast);
    }
}
