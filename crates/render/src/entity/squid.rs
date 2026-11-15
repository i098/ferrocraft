//! The squid, ported from `ModelSquid` (the body box, eight tentacles on a ring, and `setRotationAngles`,
//! which tips every tentacle by the tentacle angle) and `RenderSquid.rotateCorpse` (the body roll and
//! pitch). Every constant was read from the 1.8.9 classes with `harness/mobmodels.sh`. The animation
//! state is in `world::squid`.

use super::mobs::model;
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::FRAC_PI_2;
use world::squid::Pose;

pub(super) const SQUID: u8 = 94;

/// Tentacles around the body (`ModelSquid.tentacles.length`).
const TENTACLES: usize = 8;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == SQUID).then(|| squid(d))
}

/// `ModelSquid`: the body (the model shifts it by 24 - 16 = 8) and the tentacles, 5 blocks-sixteenths
/// out on a ring at height 31 - 16 = 15, each turned to face outward.
fn squid(d: &EntityDraw) -> Model {
    let mut parts = vec![Part::new(
        [0.0, 8.0, 0.0],
        [-6.0, -8.0, -6.0],
        [12.0, 16.0, 12.0],
        (0.0, 0.0),
    )];
    for i in 0..TENTACLES {
        let ring = i as f64 * std::f64::consts::PI * 2.0 / TENTACLES as f64;
        let turn = i as f64 * std::f64::consts::PI * -2.0 / TENTACLES as f64 + FRAC_PI_2 as f64;
        parts.push(
            Part::new(
                [ring.cos() as f32 * 5.0, 15.0, ring.sin() as f32 * 5.0],
                [-1.0, 0.0, -1.0],
                [2.0, 18.0, 2.0],
                (48.0, 0.0),
            )
            .rot([d.look.squid.tentacle, turn as f32, 0.0]),
        );
    }
    model(Skin::Squid, parts)
}

/// `RenderSquid.rotateCorpse` on a point of the model in entity-local blocks (y up from the feet):
/// `translate(0, 0.5, 0)`, `rotate(180 - yaw, y)`, `rotate(pitch, x)`, `rotate(roll, y)`,
/// `translate(0, -1.2, 0)`. The result is relative to the entity position.
pub(super) fn place(pose: &Pose, p: [f32; 3]) -> [f32; 3] {
    let spin = |angle: f32, [x, y, z]: [f32; 3]| {
        let (s, c) = angle.to_radians().sin_cos();
        [x * c + z * s, y, -x * s + z * c]
    };
    let [x, y, z] = spin(pose.roll, [p[0], p[1] - 1.2, p[2]]);
    let (s, c) = pose.pitch.to_radians().sin_cos();
    let [x, y, z] = spin(180.0 - pose.body_yaw, [x, y * c - z * s, y * s + z * c]);
    [x, y + 0.5, z]
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;
    use std::f32::consts::{FRAC_PI_4, PI};

    fn squid_draw(tentacle: f32) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(SQUID);
        d.look.squid.tentacle = tentacle;
        d
    }

    /// The part tree of `harness/mobmodels.sh` (`SquidModel`, then after `setupAnimation(age 0.4)`):
    /// the body, then the eight tentacles as (pivot x, pivot z, y rotation).
    #[test]
    fn part_layout_matches_the_oracle() {
        let m = drawn(&squid_draw(0.4));
        assert_eq!((m.skin, m.parts.len()), (Skin::Squid, 9));
        let body = &m.parts[0];
        assert_eq!(
            (body.uv, body.pivot, body.min, body.size),
            (
                (0.0, 0.0),
                [0.0, 8.0, 0.0],
                [-6.0, -8.0, -6.0],
                [12.0, 16.0, 12.0]
            )
        );
        let ring = [
            (5.0, 0.0, 1.570_796_4),
            (3.535_534, 3.535_534, FRAC_PI_4),
            (0.0, 5.0, 0.0),
            (-3.535_534, 3.535_534, -FRAC_PI_4),
            (-5.0, 0.0, -1.570_796_4),
            (-3.535_534, -3.535_534, -3.0 * FRAC_PI_4),
            (0.0, -5.0, -PI),
            (3.535_534, -3.535_534, -5.0 * FRAC_PI_4),
        ];
        for (t, (x, z, turn)) in m.parts[1..].iter().zip(ring) {
            assert_eq!(
                (t.uv, t.min, t.size),
                ((48.0, 0.0), [-1.0, 0.0, -1.0], [2.0, 18.0, 2.0])
            );
            assert!(
                (t.pivot[0] - x).abs() < 1e-5
                    && t.pivot[1] == 15.0
                    && (t.pivot[2] - z).abs() < 1e-5
            );
            assert!(t.rot[0] == 0.4 && (t.rot[1] - turn).abs() < 1e-5 && t.rot[2] == 0.0);
        }
    }

    /// A squid pitched 90 degrees lies down: a point above its middle moves to the side.
    #[test]
    fn pitch_lays_the_body_down_about_its_middle() {
        let pose = Pose {
            pitch: 90.0,
            ..Pose::default()
        };
        let q = place(&pose, [0.0, 1.7, 0.0]);
        assert!(
            q[0].abs() < 1e-5 && (q[1] - 0.5).abs() < 1e-5 && (q[2] + 0.5).abs() < 1e-5,
            "{q:?}"
        );
    }

    /// Hand-derived from the vanilla call sequence for p = (1, 2, 0), roll 90, pitch 90, yaw 90:
    /// (1, 0.8, 0) -> roll (0, 0.8, -1) -> pitch (0, 1, 0.8) -> yaw (0.8, 1, 0) -> up 0.5.
    /// Rolling after pitching would give (-1, 0.5, -0.8).
    #[test]
    fn corpse_transform_composes_roll_pitch_yaw_in_vanilla_order() {
        let pose = Pose {
            roll: 90.0,
            pitch: 90.0,
            body_yaw: 90.0,
            ..Pose::default()
        };
        let q = place(&pose, [1.0, 2.0, 0.0]);
        assert!(
            (q[0] - 0.8).abs() < 1e-5 && (q[1] - 1.5).abs() < 1e-5 && q[2].abs() < 1e-5,
            "{q:?}"
        );
    }
}
