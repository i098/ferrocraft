//! The snow golem (mob 97), ported from the 1.8.9 `ModelSnowMan`, `RenderSnowMan` and
//! `LayerSnowmanHead`. Every constant was read from the `SnowGolemModel` bytecode and checked
//! against `harness/mobmodels.sh`.

use super::mobs::{head_rot, model};
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const SNOW_GOLEM: u8 = 97;
/// `Blocks.pumpkin` as the item `LayerSnowmanHead` renders on the head.
pub(super) const PUMPKIN: (i16, i16) = (86, 0);

/// How far a worn block sits below the head pivot, in blocks: `LayerSnowmanHead` uses -0.34375,
/// `LayerCustomHead` (armor stand) -0.25.
pub(super) fn head_drop(d: &EntityDraw) -> f32 {
    if d.class == super::Class::Mob(SNOW_GOLEM) {
        -0.343_75
    } else {
        -0.25
    }
}

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == SNOW_GOLEM).then(|| snow_golem(d))
}

/// The pumpkin `LayerSnowmanHead` draws, unless the golem is invisible.
pub fn wanted(d: &EntityDraw) -> Option<(i16, i16)> {
    (d.class == super::Class::Mob(SNOW_GOLEM) && !d.invisible).then_some(PUMPKIN)
}

/// `ModelSnowMan`: head, body (`top`), base (`bottom`) and two stick arms. All boxes shrink by 0.5
/// and the texture is 64x64. The body turns a quarter of the head yaw; the arms swing out from its
/// sides. The head is first, as the pumpkin layer reads its pivot and rotation.
fn snow_golem(d: &EntityDraw) -> Model {
    let [pitch, yaw, _] = head_rot(d);
    let body_yaw = yaw * 0.25;
    let (s, c) = (body_yaw.sin(), body_yaw.cos());
    let part = |pivot, min, size, uv| {
        let mut p = Part::new(pivot, min, size, uv);
        p.inflate = -0.5;
        p
    };
    let arm = |x: f32, z: f32, rot: [f32; 3]| {
        part(
            [x, 6.0, z],
            [-1.0, 0.0, -1.0],
            [12.0, 2.0, 2.0],
            (32.0, 0.0),
        )
        .rot(rot)
    };
    model(
        Skin::Snowman,
        vec![
            part([0.0, 4.0, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0)).rot([pitch, yaw, 0.0]),
            part(
                [0.0, 13.0, 0.0],
                [-5.0, -10.0, -5.0],
                [10.0; 3],
                (0.0, 16.0),
            )
            .rot([0.0, body_yaw, 0.0]),
            part(
                [0.0, 24.0, 0.0],
                [-6.0, -12.0, -6.0],
                [12.0; 3],
                (0.0, 36.0),
            ),
            arm(c * 5.0, -s * 5.0, [0.0, body_yaw, 1.0]),
            arm(-c * 5.0, s * 5.0, [0.0, PI + body_yaw, -1.0]),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    fn golem() -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(SNOW_GOLEM);
        d
    }

    /// Oracle: `SnowGolemModel` at rest, from `harness/mobmodels.sh`.
    #[test]
    fn the_parts_match_the_oracle_layout() {
        let m = drawn(&golem());
        assert_eq!(m.skin, Skin::Snowman);
        let got: Vec<_> = m
            .parts
            .iter()
            .map(|p| (p.pivot, p.min, p.size, p.uv))
            .collect();
        assert_eq!(
            got,
            [
                ([0.0, 4.0, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0)),
                (
                    [0.0, 13.0, 0.0],
                    [-5.0, -10.0, -5.0],
                    [10.0; 3],
                    (0.0, 16.0)
                ),
                (
                    [0.0, 24.0, 0.0],
                    [-6.0, -12.0, -6.0],
                    [12.0; 3],
                    (0.0, 36.0)
                ),
                (
                    [5.0, 6.0, 0.0],
                    [-1.0, 0.0, -1.0],
                    [12.0, 2.0, 2.0],
                    (32.0, 0.0)
                ),
                (
                    [-5.0, 6.0, 0.0],
                    [-1.0, 0.0, -1.0],
                    [12.0, 2.0, 2.0],
                    (32.0, 0.0)
                ),
            ]
        );
        assert!(m.parts.iter().all(|p| p.inflate == -0.5));
        assert_eq!(m.parts[3].rot, [0.0, 0.0, 1.0]);
        assert_eq!(m.parts[4].rot, [0.0, PI, -1.0]);
    }

    /// Oracle (`harness/mobmodels.sh`, head yaw 30, pitch 15): the head turns fully, the body a
    /// quarter, and the arms swing round the body.
    #[test]
    fn the_body_follows_a_quarter_of_the_head_yaw() {
        let mut d = golem();
        d.head_yaw = 30.0;
        d.pitch = 15.0;
        let m = drawn(&d);
        let near = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        near(m.parts[0].rot[0], 0.261_799_4);
        near(m.parts[0].rot[1], std::f32::consts::FRAC_PI_6);
        near(m.parts[1].rot[1], 0.130_899_7);
        near(m.parts[3].pivot[0], 4.957_245_3);
        near(m.parts[3].pivot[2], -0.652_472_5);
        near(m.parts[4].pivot[0], -4.957_245_3);
        near(m.parts[4].pivot[2], 0.652_472_5);
        near(m.parts[4].rot[1], 3.272_492_4);
    }

    #[test]
    fn an_invisible_golem_wears_no_pumpkin() {
        let mut d = golem();
        assert_eq!(wanted(&d), Some(PUMPKIN));
        d.invisible = true;
        assert_eq!(wanted(&d), None);
        d.invisible = false;
        d.class = Class::Mob(99);
        assert_eq!(wanted(&d), None);
    }
}
