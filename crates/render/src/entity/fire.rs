use crate::mesh::{MeshData, Vertex};
use crate::{Blend, Quad};

pub const TEXTURES: [&str; 2] = [
    "textures/blocks/fire_layer_0.png",
    "textures/blocks/fire_layer_1.png",
];

#[derive(Clone, Copy)]
pub struct Fire {
    pub pos: [f64; 3],
    pub size: (f32, f32),
}

fn uv(frame: u64, frames: u32, mirrored: bool) -> [[f32; 2]; 4] {
    let frames = frames.max(1);
    let v = (frame % u64::from(frames)) as f32 / frames as f32;
    let bottom = v + 1.0 / frames as f32;
    let (left, right) = if mirrored { (1.0, 0.0) } else { (0.0, 1.0) };
    [[right, bottom], [left, bottom], [left, v], [right, v]]
}

/// `Render.renderEntityOnFire`: alternating, tapered billboards, independent of body visibility.
pub fn build(fires: &[Fire], yaw: f32, tick: u64, frames: [u32; 2]) -> [MeshData; 2] {
    let mut meshes = [MeshData::default(), MeshData::default()];
    let (sin, cos) = yaw.to_radians().sin_cos();
    for fire in fires {
        let scale = fire.size.0 * 1.4;
        if scale <= 0.0 || fire.size.1 <= 0.0 {
            continue;
        }
        let mut remaining = fire.size.1 / scale;
        let z_offset = -0.3 + remaining as i32 as f32 * 0.02;
        let mut half_width = 0.5;
        let mut layer = 0;
        while remaining > 0.0 {
            let mesh = &mut meshes[layer % 2];
            let base = mesh.vertices.len() as u32;
            let y = layer as f32 * 0.45;
            let z = z_offset + layer as f32 * 0.03;
            let coords = [
                [half_width, y],
                [-half_width, y],
                [-half_width, y + 1.4],
                [half_width, y + 1.4],
            ];
            for ([x, y], uv) in
                coords
                    .into_iter()
                    .zip(uv(tick, frames[layer % 2], layer / 2 % 2 == 0))
            {
                mesh.vertices.push(Vertex {
                    pos: [
                        (fire.pos[0] + f64::from((x * cos - z * sin) * scale)) as f32,
                        (fire.pos[1] + f64::from(y * scale)) as f32,
                        (fire.pos[2] + f64::from((x * sin + z * cos) * scale)) as f32,
                    ],
                    uv,
                    color: [1.0; 4],
                    light: [1.0; 2],
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
            remaining -= 0.45;
            half_width *= 0.9;
            layer += 1;
        }
    }
    meshes
}

/// `ItemRenderer.renderFireInFirstPerson`: two eye-space quads, after the hand and before the HUD.
pub fn overlay(width: f32, height: f32, tick: u64, frames: u32) -> [Quad; 2] {
    std::array::from_fn(|i| {
        let side = i as f32 * 2.0 - 1.0;
        let (sin, cos) = (side * 10.0).to_radians().sin_cos();
        let tan = 35_f32.to_radians().tan();
        let pos = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|[x, y]| {
            let z = -x * sin - 0.5 * cos;
            let x = x * cos - 0.5 * sin - side * 0.24;
            [
                width * 0.5 + x / -z / tan * height * 0.5,
                height * 0.5 - (y - 0.3) / -z / tan * height * 0.5,
            ]
        });
        Quad {
            tex: TEXTURES[1].into(),
            blend: Blend::Alpha,
            pos,
            uv: uv(tick, frames, false),
            color: [0xe6ffffff; 4],
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_scales_tapers_and_turns_with_the_camera() {
        let fires = [Fire {
            pos: [0.0; 3],
            size: (0.6, 1.8),
        }];
        let a = build(&fires, 0.0, 0, [32; 2]);
        let b = build(&fires, 90.0, 1, [32; 2]);
        assert_eq!(a.iter().map(|m| m.vertices.len()).sum::<usize>(), 20);
        assert!((a[0].vertices[0].pos[0].abs() - 0.42).abs() < 0.00001);
        assert!((a[0].vertices[0].pos[2] - (-0.3 + 2.0 * 0.02) * 0.84).abs() < 0.00001);
        assert!((a[0].vertices[4].pos[0].abs() - 0.42 * 0.9 * 0.9).abs() < 0.00001);
        assert!((a[0].vertices[0].pos[0] - b[0].vertices[0].pos[2]).abs() < 0.00001);
        assert_ne!(a[0].vertices[0].uv, b[0].vertices[0].uv);
        let c = build(&fires, 0.0, 5, [32; 2]);
        assert_eq!(c[0].vertices[0].uv[1], c[1].vertices[0].uv[1]);
        assert_eq!(uv(32, 32, false), uv(0, 32, false));
        assert!(build(
            &[Fire {
                size: (0.0, 0.0),
                ..fires[0]
            }],
            0.0,
            0,
            [32; 2]
        )[0]
        .is_empty());
    }
}
