//! `MobAppearanceParticle` of 1.8.9: the elder guardian that Change Game State reason 10 shows in
//! front of the camera. Ported from `GuardianModel`, `GuardianRenderer.applyScale` and
//! `MobAppearanceParticle.renderParticle`. The guardian is a fresh `setGhost` entity at the world
//! origin that never ticks, so its animation inputs are constants: spikes fully out, `ticksExisted` 0.

use super::{add_box, rotate, skin_dims, Skin, Skins, V3};
use crate::mesh::MeshData;
use std::f32::consts::PI;

/// Lifetime of the particle in ticks.
pub const LIFETIME: u32 = 30;
/// `MobAppearanceParticle` scale of the whole model.
const SCALE: f32 = 0.425_531_92;
/// `GuardianRenderer.applyScale` of an elder guardian.
const ELDER_SCALE: f32 = 2.35;
/// `RendererLivingEntity.doRender` lifts the model by this much (`BODY_OFFSET` of the other models).
const BODY_OFFSET: f32 = 1.507_812_5;
/// `EntityPlayer.getEyeHeight`, the camera above the feet.
const PLAYER_EYE: f32 = 1.62;
/// `Entity.getEyeHeight` of an elder guardian: half of its 1.9975 height.
const EYE_HEIGHT: f32 = 1.9975 * 0.5;

/// One curse particle ready to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curse {
    /// `(age + partial) / lifetime`.
    pub progress: f32,
    /// Tail phase: the entity constructor draws it once with `nextFloat`.
    pub tail: f32,
    /// Tick fraction of the frame, the whole `ticksExisted + partial` of the frozen entity.
    pub partial: f32,
    /// Camera yaw and pitch in degrees.
    pub view: [f32; 2],
    /// World position of the viewer's eyes, the target the guardian's eye looks at.
    pub target: [f64; 3],
}

/// Spike orientation in half turns, and offset from the head in model pixels.
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

/// A `ModelRenderer` box: corner, size, texture origin and mirror.
type Cube = (V3, V3, (f32, f32), bool);

/// A model part with its pose relative to its parent, as `(rotation point, rotation)` from the root.
struct Part {
    chain: Vec<(V3, V3)>,
    cubes: Vec<Cube>,
}

fn spikes(partial: f32) -> Vec<Part> {
    (0..12)
        .map(|i| {
            // `getSpikesExtension` is 1 for a ghost, so the `0.55 * (1 - extension)` pull-in is 0.
            let wiggle = 1.0 + (partial * 1.5 + i as f32).cos() * 0.01;
            let at = [
                SPIKE_AT[0][i] * wiggle,
                16.0 + SPIKE_AT[1][i] * wiggle,
                SPIKE_AT[2][i] * wiggle,
            ];
            let rot = [0, 1, 2].map(|a| PI * SPIKE_ROT[a][i]);
            Part {
                chain: vec![(at, rot)],
                cubes: vec![([-1.0, -4.5, -1.0], [2.0, 9.0, 2.0], (0.0, 0.0), false)],
            }
        })
        .collect()
}

/// `GuardianModel.setupAnimation`: where the eye sits for a viewer at `target`.
fn eye(target: [f64; 3]) -> V3 {
    let y = if target[1] - f64::from(EYE_HEIGHT) > 0.0 {
        0.0
    } else {
        1.0
    };
    // `Vec3d.normalize` returns the zero vector under 1e-4. The guardian looks along +z, and its
    // direction turned a quarter round about y is -x, so the dot product is minus the viewer's x share.
    let len = target[0].hypot(target[2]);
    let dot = if len < 1.0e-4 { 0.0 } else { -target[0] / len };
    let x = if dot == 0.0 {
        0.0
    } else {
        (dot.abs() as f32).sqrt() * 2.0 * dot.signum() as f32
    };
    [x, y, -8.25]
}

fn parts(c: &Curse) -> Vec<Part> {
    let head = (([0.0; 3]), [0.0; 3]);
    let tail = [
        ([0.0; 3], [0.0, c.tail.sin() * PI * 0.05, 0.0]),
        ([-1.5, 0.5, 14.0], [0.0, c.tail.sin() * PI * 0.1, 0.0]),
        ([0.5, 0.5, 6.0], [0.0, c.tail.sin() * PI * 0.15, 0.0]),
    ];
    let mut out = vec![Part {
        chain: vec![head],
        cubes: vec![
            ([-6.0, 10.0, -8.0], [12.0, 12.0, 16.0], (0.0, 0.0), false),
            ([-8.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0), false),
            ([6.0, 10.0, -6.0], [2.0, 12.0, 12.0], (0.0, 28.0), true),
            ([-6.0, 8.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0), false),
            ([-6.0, 22.0, -6.0], [12.0, 2.0, 12.0], (16.0, 40.0), false),
        ],
    }];
    out.extend(spikes(c.partial).into_iter().map(|mut s| {
        s.chain.push(head);
        s
    }));
    out.push(Part {
        chain: vec![(eye(c.target), [0.0; 3]), head],
        cubes: vec![([-1.0, 15.0, 0.0], [2.0, 2.0, 1.0], (8.0, 0.0), false)],
    });
    let tail_cubes: [Vec<Cube>; 3] = [
        vec![([-2.0, 14.0, 7.0], [4.0, 4.0, 8.0], (40.0, 0.0), false)],
        vec![([0.0, 14.0, 0.0], [3.0, 3.0, 7.0], (0.0, 54.0), false)],
        vec![
            ([0.0, 14.0, 0.0], [2.0, 2.0, 6.0], (41.0, 32.0), false),
            ([1.0, 10.5, 3.0], [1.0, 9.0, 9.0], (25.0, 19.0), false),
        ],
    ];
    for (n, cubes) in tail_cubes.into_iter().enumerate() {
        // Each tail segment hangs off the previous one, the first off the head.
        let mut chain: Vec<(V3, V3)> = tail[..=n].iter().rev().copied().collect();
        chain.push(head);
        out.push(Part { chain, cubes });
    }
    out
}

/// Model pixel to camera-relative world point, `MobAppearanceParticle.renderParticle` over
/// `RendererLivingEntity.doRender` (yaw 0, mirrored, elder scale).
fn place(p: V3, c: &Curse) -> V3 {
    let m = [p[0] / 16.0, p[1] / 16.0 - BODY_OFFSET, p[2] / 16.0].map(|v| v * ELDER_SCALE);
    let m = rotate([-m[0], -m[1], m[2]], [0.0, PI, 0.0]).map(|v| v * SCALE);
    let m = [m[0], m[1] - 0.4, m[2] - 1.5];
    let tilt = (60.0 - 150.0 * c.progress - c.view[1]).to_radians();
    let m = rotate(
        rotate(m, [tilt, 0.0, 0.0]),
        [0.0, (180.0 - c.view[0]).to_radians(), 0.0],
    );
    // The particle pass starts at the viewer's feet, `RenderManager` draws entities relative to them;
    // the result is the offset from the camera, which sits one eye height higher.
    [m[0], m[1] + 1.8 - PLAYER_EYE, m[2]]
}

/// The quads of the guardian, camera-relative, full bright and faded by `0.05 + 0.5 sin(pi t)`.
pub fn build(curses: &[Curse], skins: &Skins) -> MeshData {
    let mut out = MeshData::default();
    let v_off = skins.v_offset(Skin::GuardianElder);
    for c in curses {
        let start = out.vertices.len();
        for part in parts(c) {
            let tf = |p: V3| {
                let world = part.chain.iter().fold(p, |q, &(at, rot)| {
                    let r = rotate(q, rot);
                    [r[0] + at[0], r[1] + at[1], r[2] + at[2]]
                });
                place(world, c)
            };
            for (min, size, uv, mirror) in &part.cubes {
                add_box(
                    &mut out,
                    &tf,
                    *min,
                    *size,
                    *size,
                    *uv,
                    *mirror,
                    v_off,
                    [1.0; 3],
                    skin_dims(),
                );
            }
        }
        let alpha = 0.05 + 0.5 * (c.progress * PI).sin();
        for v in &mut out.vertices[start..] {
            v.color = [1.0, 1.0, 1.0, alpha];
            v.light = [1.0, 1.0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_follows_the_viewers_side_and_height() {
        // Viewer along +x: the pupil moves to x -2; below the guardian's eye the pupil drops to y 1.
        assert_eq!(eye([10.0, 70.0, 0.0]), [-2.0, 0.0, -8.25]);
        assert_eq!(eye([-4.0, 0.5, 0.0]), [2.0, 1.0, -8.25]);
        // Straight ahead (or at the origin) the eye stays centered.
        assert_eq!(eye([0.0, 70.0, 5.0])[0], 0.0);
        assert_eq!(eye([0.0, 70.0, 0.0])[0], 0.0);
    }

    #[test]
    fn model_sits_in_front_of_the_camera_and_fades_in_then_out() {
        let skins = Skins::flat();
        let at = |progress| Curse {
            progress,
            tail: 0.5,
            partial: 0.0,
            view: [0.0, 0.0],
            target: [0.0, 70.0, 0.0],
        };
        let mesh = build(&[at(0.5)], &skins);
        // 5 head boxes, 12 spikes, eye and 4 tail boxes make 22 boxes of 24 vertices.
        assert_eq!(mesh.vertices.len(), 22 * 24);
        let alpha = |m: &MeshData| m.vertices[0].color[3];
        assert!((alpha(&mesh) - 0.55).abs() < 1e-6);
        assert!(alpha(&build(&[at(0.0)], &skins)) < 0.06);
        // Yaw 0 looks along +z; the head ends up ahead of the camera.
        let mean_z: f32 =
            mesh.vertices.iter().map(|v| v.pos[2]).sum::<f32>() / mesh.vertices.len() as f32;
        assert!(mean_z > 0.5, "{mean_z}");
    }
}
