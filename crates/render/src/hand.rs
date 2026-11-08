//! First-person hand and held block, ported from vanilla 1.8.9 `ItemRenderer.renderItemInFirstPerson`
//! (`transformFirstPersonItem`, the empty-hand arm of `RenderPlayer.renderRightArm`).
//! Flat items need the item atlas and the held stack, which the engine does not track yet, so only blocks and the bare arm are drawn.

use crate::assets::Atlas;
use crate::mesh::{mesh_chunk, MeshData, Neighborhood, Vertex, HEIGHT};
use std::f32::consts::PI;

pub type Mat = [[f32; 4]; 4];

/// What the engine tells the renderer about the player's hand for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    /// Block state id in the hand (`id << 4 | meta`); `None` draws the bare arm.
    pub held: Option<u16>,
    /// `getSwingProgress(partial)`, `0..1`.
    pub swing: f32,
    /// `1 - equippedProgress`: 0 with the item fully raised, 1 fully lowered.
    pub lowered: f32,
}

/// One frame of hand geometry: vertices in model space and the matrix that places them in eye space.
pub struct Draw {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub model: Mat,
}

fn ident() -> Mat {
    let mut m = [[0.0; 4]; 4];
    for (i, c) in m.iter_mut().enumerate() {
        c[i] = 1.0;
    }
    m
}

fn mul(a: Mat, b: Mat) -> Mat {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

fn translate(x: f32, y: f32, z: f32) -> Mat {
    let mut m = ident();
    m[3] = [x, y, z, 1.0];
    m
}

fn scale(s: f32) -> Mat {
    let mut m = ident();
    for (i, c) in m.iter_mut().enumerate().take(3) {
        c[i] = s;
    }
    m
}

/// `glRotatef(deg, axis)` for `axis` 0, 1 or 2.
fn rotate(deg: f32, axis: usize) -> Mat {
    let (s, c) = deg.to_radians().sin_cos();
    let mut m = ident();
    let (a, b) = [(1, 2), (2, 0), (0, 1)][axis];
    m[a][a] = c;
    m[b][b] = c;
    m[a][b] = s;
    m[b][a] = -s;
    m
}

fn chain(ms: &[Mat]) -> Mat {
    ms.iter().fold(ident(), |acc, m| mul(acc, *m))
}

/// `ItemRenderer.transformFirstPersonItem`.
fn item_transform(swing: f32, lowered: f32) -> Mat {
    let root = swing.sqrt();
    chain(&[
        translate(
            -0.4 * (root * PI).sin(),
            0.2 * (root * 2.0 * PI).sin(),
            -0.2 * (swing * PI).sin(),
        ),
        translate(0.56, -0.52, -0.72),
        translate(0.0, lowered * -0.6, 0.0),
        rotate(45.0, 1),
        rotate((swing * swing * PI).sin() * -20.0, 1),
        rotate((root * PI).sin() * -20.0, 2),
        rotate((root * PI).sin() * -80.0, 0),
        scale(0.4),
    ])
}

/// A single block centred on the origin, as `RenderItem.renderItem` draws it (`scale(0.5)` follows in the model matrix).
pub fn held_mesh(state: u16, atlas: &Atlas) -> MeshData {
    let mut blocks = vec![0u16; 16 * HEIGHT * 16];
    blocks[crate::mesh::index(8, 128, 8)] = state;
    let hood = Neighborhood {
        origin: (0, 0),
        center: &blocks,
        neighbors: [None; 4],
    };
    let m = mesh_chunk(&hood, atlas);
    let mut out = m.opaque;
    let base = out.vertices.len() as u32;
    out.vertices.extend(m.water.vertices);
    out.indices.extend(m.water.indices.iter().map(|i| i + base));
    for v in &mut out.vertices {
        v.pos = [v.pos[0] - 8.5, v.pos[1] - 128.5, v.pos[2] - 8.5];
    }
    out
}

pub fn block(hand: &Hand, mesh: &MeshData) -> Draw {
    Draw {
        vertices: mesh.vertices.clone(),
        indices: mesh.indices.clone(),
        model: mul(item_transform(hand.swing, hand.lowered), scale(0.5)),
    }
}

/// Right arm box `addBox(-3, -2, -2, 4, 12, 4)` at skin pixel (40, 16); model y points down.
const BOX: [[f32; 2]; 3] = [[-3.0, 1.0], [-2.0, 10.0], [-2.0, 2.0]];

/// Eye-space placement of the right arm: `ItemRenderer` swing, `ModelRenderer` rotation point and z rotation.
fn arm_model(hand: &Hand) -> Mat {
    let (swing, root) = (hand.swing, hand.swing.sqrt());
    let place = chain(&[
        translate(
            -0.3 * (root * PI).sin(),
            0.4 * (root * 2.0 * PI).sin(),
            -0.4 * (swing * PI).sin(),
        ),
        translate(0.64, -0.6 + hand.lowered * -0.6, -0.72),
        rotate(45.0, 1),
        rotate((root * PI).sin() * 70.0, 1),
        rotate((swing * swing * PI).sin() * -20.0, 2),
        translate(-1.0, 3.6, 3.5),
        rotate(120.0, 2),
        rotate(200.0, 0),
        rotate(-135.0, 1),
        translate(5.6, 0.0, 0.0),
    ]);
    // Rotation point (-5, 2, 0) px, then z rotation 0.05 rad from `ModelBiped.setRotationAngles`.
    chain(&[
        place,
        translate(-5.0 / 16.0, 2.0 / 16.0, 0.0),
        rotate(0.05f32.to_degrees(), 2),
    ])
}

/// `RenderPlayer.renderRightArm` seen through `ItemRenderer`: the arm of the default skin.
pub fn arm(hand: &Hand, skin_height: u32) -> Draw {
    let model = arm_model(hand);
    let [[x0, x1], [y0, y1], [z0, z1]] = BOX;
    let (tw, th) = (64.0, skin_height.max(32) as f32);
    // Each face: normal, then four corners as (position, u, v) in skin pixels.
    type Corner = ([f32; 3], f32, f32);
    let face = |normal: [f32; 3], axes: [[f32; 2]; 2], f: &dyn Fn(f32, f32) -> Corner| {
        let [[a0, a1], [b0, b1]] = axes;
        (
            normal,
            [(a0, b0), (a1, b0), (a1, b1), (a0, b1)].map(|(a, b)| f(a, b)),
        )
    };
    let faces = [
        // front (z min), back (z max)
        face([0.0, 0.0, -1.0], [[x0, x1], [y0, y1]], &|x, y| {
            ([x, y, z0], 44.0 + (x - x0), 20.0 + (y - y0))
        }),
        face([0.0, 0.0, 1.0], [[x0, x1], [y0, y1]], &|x, y| {
            ([x, y, z1], 56.0 - (x - x0), 20.0 + (y - y0))
        }),
        // outer (x min), inner (x max)
        face([-1.0, 0.0, 0.0], [[z0, z1], [y0, y1]], &|z, y| {
            ([x0, y, z], 44.0 - (z - z0), 20.0 + (y - y0))
        }),
        face([1.0, 0.0, 0.0], [[z0, z1], [y0, y1]], &|z, y| {
            ([x1, y, z], 48.0 + (z - z0), 20.0 + (y - y0))
        }),
        // top (y min), bottom (y max)
        face([0.0, -1.0, 0.0], [[x0, x1], [z0, z1]], &|x, z| {
            ([x, y0, z], 44.0 + (x - x0), 20.0 - (z - z0))
        }),
        face([0.0, 1.0, 0.0], [[x0, x1], [z0, z1]], &|x, z| {
            ([x, y1, z], 48.0 + (x - x0), 16.0 + (z - z0))
        }),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let shade = item_light(&model, normal);
        let base = vertices.len() as u32;
        for (p, u, v) in corners {
            vertices.push(Vertex {
                pos: p.map(|c| c / 16.0),
                uv: [u / tw, v / th],
                color: [shade, shade, shade, 1.0],
                light: [1.0, 0.0],
            });
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    Draw {
        vertices,
        indices,
        model,
    }
}

/// `RenderHelper.enableStandardItemLighting`: two directional lights of 0.6 plus 0.4 ambient, lights fixed in eye space.
fn item_light(model: &Mat, normal: [f32; 3]) -> f32 {
    let n: [f32; 3] = std::array::from_fn(|r| (0..3).map(|c| model[c][r] * normal[c]).sum());
    let len = n.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let diffuse = |l: [f32; 3]| {
        let ll = l.iter().map(|v| v * v).sum::<f32>().sqrt();
        ((0..3).map(|i| n[i] * l[i]).sum::<f32>() / (len * ll)).max(0.0)
    };
    (0.4 + 0.6 * (diffuse([0.2, 1.0, -0.7]) + diffuse([-0.2, 1.0, 0.7]))).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no swing and the arm fully raised the hand sits at the lower right of the view, in front of the camera.
    #[test]
    fn resting_arm_is_lower_right_in_front_of_the_camera() {
        let d = arm(
            &Hand {
                held: None,
                swing: 0.0,
                lowered: 0.0,
            },
            64,
        );
        let eye: Vec<[f32; 3]> = d
            .vertices
            .iter()
            .map(|v| {
                std::array::from_fn(|r| {
                    (0..3).map(|c| d.model[c][r] * v.pos[c]).sum::<f32>() + d.model[3][r]
                })
            })
            .collect();
        let n = eye.len() as f32;
        let mean: [f32; 3] = std::array::from_fn(|i| eye.iter().map(|p| p[i]).sum::<f32>() / n);
        assert!(mean[0] > 0.2 && mean[1] < 0.0 && mean[2] < -0.2, "{mean:?}");
    }
}
