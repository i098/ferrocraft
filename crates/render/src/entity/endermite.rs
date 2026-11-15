//! The endermite, ported from `ModelEndermite` (four body segments, `setupAnimation`) and
//! `EndermiteRenderer` (texture, `getDeathYaw` 180). Every constant was read from the 1.8.9
//! classes with `harness/mobmodels.sh`, which runs the model at rest and after `setupAnimation`.

use super::mobs::model;
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const ENDERMITE: u8 = 67;

/// `EndermiteRenderer.getDeathYaw`: a dead endermite falls over 180 degrees, not 90.
pub(super) const DEATH_YAW: f32 = 180.0;

/// Box sizes of the four segments, front to back.
const SIZES: [[f32; 3]; 4] = [
    [4.0, 3.0, 2.0],
    [6.0, 4.0, 5.0],
    [3.0, 3.0, 1.0],
    [1.0, 2.0, 1.0],
];
/// Texture offsets of the four segments.
const UV: [(f32, f32); 4] = [(0.0, 0.0), (0.0, 5.0), (0.0, 14.0), (0.0, 18.0)];

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == ENDERMITE).then(|| endermite(d))
}

/// Pivot z of each segment: -3.5, then the half depths of the segment and the next one added.
const ZS: [f32; 4] = [-3.5, 0.0, 3.0, 4.0];

/// `ModelEndermite`: each segment wiggles about its own pivot with the age.
fn endermite(d: &EntityDraw) -> Model {
    let segment = |i: usize| {
        let [w, h, depth] = SIZES[i];
        let phase = d.age * 0.9 + i as f32 * 0.15 * PI;
        let spread = (i as f32 - 2.0).abs();
        Part::new(
            [phase.sin() * PI * 0.1 * spread, 24.0 - h, ZS[i]],
            [w * -0.5, 0.0, depth * -0.5],
            SIZES[i],
            UV[i],
        )
        .rot([0.0, phase.cos() * PI * 0.01 * (1.0 + spread), 0.0])
    };
    model(Skin::Endermite, (0..SIZES.len()).map(segment).collect())
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    fn mite(age: f32) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(ENDERMITE);
        d.age = age;
        d
    }

    /// Oracle dump of `EndermiteModel` at rest: (pivot, min, max).
    #[test]
    fn box_layout_matches_the_oracle() {
        let want = [
            ([0.0, 21.0, -3.5], [-2.0, 0.0, -1.0], [2.0, 3.0, 1.0]),
            ([0.0, 20.0, 0.0], [-3.0, 0.0, -2.5], [3.0, 4.0, 2.5]),
            ([0.0, 21.0, 3.0], [-1.5, 0.0, -0.5], [1.5, 3.0, 0.5]),
            ([0.0, 22.0, 4.0], [-0.5, 0.0, -0.5], [0.5, 2.0, 0.5]),
        ];
        let uv = [(0.0, 0.0), (0.0, 5.0), (0.0, 14.0), (0.0, 18.0)];
        // At age 0 sin(phase) is not 0 for i > 0; the oracle's rest dump has no animation, so
        // compare the y and z pivots and the boxes only.
        let m = drawn(&mite(0.0));
        assert_eq!(m.parts.len(), 4);
        for (i, (p, (pivot, min, max))) in m.parts.iter().zip(want).enumerate() {
            assert_eq!(p.pivot[1..], pivot[1..], "pivot {i}");
            assert_eq!(p.min, min, "min {i}");
            let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
            assert_eq!(p.size, size, "size {i}");
            assert_eq!(p.uv, uv[i], "uv {i}");
        }
    }

    /// Oracle: `setupAnimation(age 10)`: pivot x and y rotation of each segment.
    #[test]
    fn animation_matches_the_oracle() {
        let want = [
            (0.258_963_9, -0.085_870_5),
            (-0.014_572_677, -0.062_764_22),
            (0.0, -0.027_300_376),
            (-0.262_462_14, -0.034_530_956),
        ];
        let m = drawn(&mite(10.0));
        for (i, (p, (x, rot_y))) in m.parts.iter().zip(want).enumerate() {
            // The oracle reads `MathHelper.sin`, a 65536-entry table, 5e-5 off `f32::sin`.
            assert!((p.pivot[0] - x).abs() < 1e-3, "x {i}: {}", p.pivot[0]);
            assert!((p.rot[1] - rot_y).abs() < 1e-3, "rot {i}: {}", p.rot[1]);
        }
    }
}
