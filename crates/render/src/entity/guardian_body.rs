//! The guardian (mob 68), ported from `GuardianModel` (head, eye, 12 spikes, 3 tail parts,
//! `setupAnimation`) and `GuardianRenderer` (texture, `applyScale`). The box layout and the pose were
//! read from the 1.8.9 classes: `GuardianModel.setupAnimation` was run on a `GuardianEntity` with a
//! camera entity set in `Minecraft`.
//!
//! Live guardians draw with spikes retracted and a still tail until `mobTick` state is tracked.

use super::guardian::ELDER_SCALE;
use super::mobs::{head_rot, model};
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const GUARDIAN: u8 = 68;

/// Metadata int 16 bit 4: `GuardianEntity.isElder`.
pub(super) const ELDER: u8 = 4;

/// `GuardianRenderer.applyScale`: an elder guardian is drawn at [`ELDER_SCALE`].
pub(super) fn scale(variant: u8) -> f32 {
    if variant & ELDER != 0 {
        ELDER_SCALE
    } else {
        1.0
    }
}

/// Spike rotation about x, y and z in half turns.
const SPIKE_ROT: [[f32; 12]; 3] = [
    [
        1.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0.0, 0.0,
    ],
    [
        0.0, 0.0, 0.0, 0.0, 0.25, 1.75, 1.25, 0.75, 0.0, 0.0, 0.0, 0.0,
    ],
    [
        0.0, 0.0, 0.25, 1.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.25,
    ],
];
/// Spike direction from the head center (0, 16, 0), in model pixels when fully out.
const SPIKE_AT: [[f32; 12]; 3] = [
    [
        0.0, 0.0, 8.0, -8.0, -8.0, 8.0, 8.0, -8.0, 0.0, 0.0, 8.0, -8.0,
    ],
    [
        -8.0, -8.0, -8.0, -8.0, 0.0, 0.0, 0.0, 0.0, 8.0, 8.0, 8.0, 8.0,
    ],
    [
        8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
    ],
];

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == GUARDIAN).then(|| guardian(d))
}

/// `GuardianModel.setupAnimation`, then the head with all its children.
fn guardian(d: &EntityDraw) -> Model {
    let head = ([0.0; 3], head_rot(d));
    let body = |min, size, uv| Part::new([0.0; 3], min, size, uv).rot(head.1);
    let mut parts = vec![
        body([-6.0, 10.0, -8.0], [12.0, 12.0, 16.0], (0.0, 0.0)),
        body([-8.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0)),
        body([6.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0)).mirrored(true),
        body([-6.0, 8.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0)),
        body([-6.0, 22.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0)),
    ];
    // The spikes pull in by 0.55 of their distance when retracted, and wobble with the age.
    let pull = (1.0 - d.look.guardian_spikes) * 0.55;
    parts.extend((0..12).map(|i| {
        let out = 1.0 + (d.age * 1.5 + i as f32).cos() * 0.01 - pull;
        let at = [
            SPIKE_AT[0][i] * out,
            16.0 + SPIKE_AT[1][i] * out,
            SPIKE_AT[2][i] * out,
        ];
        Part::new(at, [-1.0, -4.5, -1.0], [2.0, 9.0, 2.0], (0.0, 0.0))
            .rot([0, 1, 2].map(|a| PI * SPIKE_ROT[a][i]))
            .under(vec![head])
    }));
    parts.push(Part::new(eye(d), [-1.0, 15.0, 0.0], [2.0, 2.0, 1.0], (8.0, 0.0)).under(vec![head]));
    let wag = d.look.guardian_tail.sin() * PI;
    let tail = [
        ([0.0; 3], [0.0, wag * 0.05, 0.0]),
        ([-1.5, 0.5, 14.0], [0.0, wag * 0.1, 0.0]),
        ([0.5, 0.5, 6.0], [0.0, wag * 0.15, 0.0]),
    ];
    // Each tail part hangs off the one before it, the first off the head.
    let chain = |n: usize| tail[..n].iter().rev().copied().chain([head]).collect();
    let segment = |n: usize, min, size, uv| {
        let (pivot, rot) = tail[n];
        Part::new(pivot, min, size, uv).rot(rot).under(chain(n))
    };
    parts.extend([
        segment(0, [-2.0, 14.0, 7.0], [4.0, 4.0, 8.0], (40.0, 0.0)),
        segment(1, [0.0, 14.0, 0.0], [3.0, 3.0, 7.0], (0.0, 54.0)),
        segment(2, [0.0, 14.0, 0.0], [2.0, 2.0, 6.0], (41.0, 32.0)),
        segment(2, [1.0, 10.5, 3.0], [1.0, 9.0, 9.0], (25.0, 19.0)),
    ]);
    // `GuardianRenderer.getTextureLocation`.
    let skin = if d.look.variant & ELDER != 0 {
        Skin::GuardianElder
    } else {
        Skin::Guardian
    };
    model(skin, parts)
}

/// `GuardianModel.setupAnimation`: the pupil turns to the side of the viewer and drops when the
/// viewer is not above the guardian's eye. The guardian has no beam target here, so the target is
/// the camera entity (`Look::viewer`).
fn eye(d: &EntityDraw) -> [f32; 3] {
    // `GuardianEntity.getEyeHeight` is half the height.
    let eye_y = d.pos[1] + f64::from(d.size.1 * 0.5);
    let [tx, ty, tz] = d.look.viewer;
    let y = if ty - eye_y > 0.0 { 0.0 } else { 1.0 };
    // `getRotationVector` with y dropped, against the guardian-to-viewer offset turned a quarter
    // round about y (`Vec3d.rotateY(PI / 2)` maps (x, z) to (z, -x)).
    let (yaw, pitch) = (d.head_yaw.to_radians(), d.pitch.to_radians());
    let look = [-yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos()].map(f64::from);
    let (vx, vz) = (d.pos[0] - tx, d.pos[2] - tz);
    let len = vx.hypot(vz);
    // `Vec3d.normalize` returns the zero vector under 1e-4.
    let dot = if len < 1.0e-4 {
        0.0
    } else {
        (look[0] * vz - look[1] * vx) / len
    };
    let x = (dot.abs() as f32).sqrt() * 2.0 * dot.signum() as f32;
    [if dot == 0.0 { 0.0 } else { x }, y, -8.25]
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    /// The oracle run: guardian at (1, 0, 2), `headYaw` 50, pitch 15, body yaw 20, ticks 10 and
    /// age 10.5, spikes 0.4 out, tail angle 1.5.
    fn posed(viewer: [f64; 3]) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(GUARDIAN);
        d.pos = [1.0, 0.0, 2.0];
        d.size = (0.85, 0.85);
        d.yaw = 20.0;
        d.head_yaw = 50.0;
        d.pitch = 15.0;
        d.age = 10.5;
        d.look.guardian_spikes = 0.4;
        d.look.guardian_tail = 1.5;
        d.look.viewer = viewer;
        d
    }

    fn close(a: [f32; 3], b: [f32; 3], what: &str) {
        // The oracle reads `MathHelper.sin`, a 65536-entry table, a little off `f32::sin`.
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 2e-3, "{what}: {a:?} != {b:?}");
        }
    }

    /// Oracle: the pivot and rotation of each spike, the tail and the head after `setupAnimation`.
    #[test]
    fn animation_matches_the_oracle() {
        let spikes = [
            [0.0, 10.719929, 5.280071],
            [0.0, 10.680363, -5.319638],
            [5.396318, 10.603682, 0.0],
            [-5.4396033, 10.560396, 0.0],
            [-5.4097066, 16.0, -5.4097066],
            [5.334106, 16.0, -5.334106],
            [5.282316, 16.0, 5.282316],
            [-5.3019495, 16.0, 5.3019495],
            [0.0, 21.374952, 5.374953],
            [0.0, 21.43421, -5.4342093],
            [5.425239, 21.42524, 0.0],
            [-5.3562894, 21.356289, 0.0],
        ];
        let m = drawn(&posed([3.0, 5.0, 1.0]));
        assert_eq!(m.skin, Skin::Guardian);
        // 5 head boxes, 12 spikes, the eye and 4 tail boxes.
        assert_eq!(m.parts.len(), 22);
        // Oracle: (0.2617994, 0.5235988), the 15 degree pitch and the 50 - 20 degree head yaw.
        let head = [PI / 12.0, PI / 6.0, 0.0];
        close(m.parts[0].rot, head, "head");
        for (i, want) in spikes.iter().enumerate() {
            let p = &m.parts[5 + i];
            close(p.pivot, *want, "spike");
            assert_eq!(p.parents, vec![([0.0; 3], head_rot(&posed([0.0; 3])))]);
        }
        close(m.parts[11].rot, [PI * 0.5, PI * 1.25, 0.0], "spike 6 rot");
        let tail = [0.15668555, 0.3133711, 0.47005665];
        for (n, part) in [18, 19, 20].into_iter().enumerate() {
            close(m.parts[part].rot, [0.0, tail[n], 0.0], "tail");
            assert_eq!(m.parts[part].parents.len(), n + 1);
        }
        close(m.parts[20].pivot, [0.5, 0.5, 6.0], "tail 2 pivot");
    }

    /// Oracle: the eye pivot for three camera eye positions.
    #[test]
    fn eye_follows_the_viewer_like_the_oracle() {
        let cases = [
            ([3.0, 5.0, 1.0], [0.9475072, 0.0, -8.25]),
            ([-2.0, -1.0, 4.0], [-0.651709, 1.0, -8.25]),
            ([0.0, 0.425, 0.0], [-1.9385353, 1.0, -8.25]),
        ];
        for (viewer, want) in cases {
            close(drawn(&posed(viewer)).parts[17].pivot, want, "eye");
        }
    }

    /// Oracle box dump of `GuardianModel` (corner, size, texture origin).
    #[test]
    fn box_layout_matches_the_oracle() {
        let m = drawn(&posed([0.0; 3]));
        let want = [
            ([-6.0, 10.0, -8.0], [12.0, 12.0, 16.0], (0.0, 0.0)),
            ([-8.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0)),
            ([6.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0)),
            ([-6.0, 8.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0)),
            ([-6.0, 22.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0)),
            ([-1.0, 15.0, 0.0], [2.0, 2.0, 1.0], (8.0, 0.0)),
            ([-2.0, 14.0, 7.0], [4.0, 4.0, 8.0], (40.0, 0.0)),
            ([0.0, 14.0, 0.0], [3.0, 3.0, 7.0], (0.0, 54.0)),
            ([0.0, 14.0, 0.0], [2.0, 2.0, 6.0], (41.0, 32.0)),
            ([1.0, 10.5, 3.0], [1.0, 9.0, 9.0], (25.0, 19.0)),
        ];
        let picked = [0, 1, 2, 3, 4, 17, 18, 19, 20, 21].map(|i| &m.parts[i]);
        for (p, (min, size, uv)) in picked.iter().zip(want) {
            assert_eq!((p.min, p.size, p.uv), (min, size, uv));
        }
        assert!(m.parts[2].mirror);
        let spike = &m.parts[5];
        assert_eq!(
            (spike.min, spike.size, spike.uv),
            ([-1.0, -4.5, -1.0], [2.0, 9.0, 2.0], (0.0, 0.0))
        );
    }

    #[test]
    fn elder_flag_picks_the_elder_texture() {
        let mut d = posed([0.0; 3]);
        d.look.variant = ELDER;
        assert_eq!(drawn(&d).skin, Skin::GuardianElder);
    }
}
