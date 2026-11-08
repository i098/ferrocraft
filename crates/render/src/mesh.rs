//! CPU chunk meshing: one quad per visible block face (faces next to an opaque block are culled,
//! including across chunk borders).

use crate::assets::Atlas;
use crate::blocks::{look, Kind, Tint};
use bytemuck::{Pod, Zeroable};

pub const HEIGHT: usize = 256;
/// Blocks of one chunk, index `(y * 16 + z) * 16 + x`, 1.8 block state ids.
pub type Blocks = [u16];

pub fn index(x: usize, y: usize, z: usize) -> usize {
    (y * 16 + z) * 16 + x
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

#[derive(Default)]
pub struct Meshes {
    pub opaque: MeshData,
    pub water: MeshData,
}

/// The chunk to mesh plus the neighbours in -x, +x, -z, +z order (when loaded).
pub struct Neighborhood<'a> {
    pub center: &'a Blocks,
    pub neighbors: [Option<&'a Blocks>; 4],
}

impl Neighborhood<'_> {
    /// Block state id at chunk-local `x`, `z` in `-1..=16` and any `y`; 0 (air) outside the loaded data.
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(0..HEIGHT as i32).contains(&y) {
            return 0;
        }
        let (data, lx, lz) = match (x, z) {
            (0..=15, 0..=15) => (Some(self.center), x, z),
            (-1, 0..=15) => (self.neighbors[0], 15, z),
            (16, 0..=15) => (self.neighbors[1], 0, z),
            (0..=15, -1) => (self.neighbors[2], x, 15),
            (0..=15, 16) => (self.neighbors[3], x, 0),
            _ => (None, 0, 0),
        };
        data.map_or(0, |d| d[index(lx as usize, y as usize, lz as usize)])
    }
}

struct Face {
    dir: [i32; 3],
    shade: f32,
    /// Corner offsets and texture coordinates in loop order.
    corners: [([f32; 3], [f32; 2]); 4],
}

const FACES: [Face; 6] = [
    Face {
        dir: [0, 1, 0],
        shade: 1.0,
        corners: [
            ([0., 1., 0.], [0., 0.]),
            ([0., 1., 1.], [0., 1.]),
            ([1., 1., 1.], [1., 1.]),
            ([1., 1., 0.], [1., 0.]),
        ],
    },
    Face {
        dir: [0, -1, 0],
        shade: 0.5,
        corners: [
            ([0., 0., 0.], [0., 0.]),
            ([1., 0., 0.], [1., 0.]),
            ([1., 0., 1.], [1., 1.]),
            ([0., 0., 1.], [0., 1.]),
        ],
    },
    Face {
        dir: [1, 0, 0],
        shade: 0.6,
        corners: [
            ([1., 0., 1.], [0., 1.]),
            ([1., 0., 0.], [1., 1.]),
            ([1., 1., 0.], [1., 0.]),
            ([1., 1., 1.], [0., 0.]),
        ],
    },
    Face {
        dir: [-1, 0, 0],
        shade: 0.6,
        corners: [
            ([0., 0., 0.], [0., 1.]),
            ([0., 0., 1.], [1., 1.]),
            ([0., 1., 1.], [1., 0.]),
            ([0., 1., 0.], [0., 0.]),
        ],
    },
    Face {
        dir: [0, 0, 1],
        shade: 0.8,
        corners: [
            ([0., 0., 1.], [0., 1.]),
            ([1., 0., 1.], [1., 1.]),
            ([1., 1., 1.], [1., 0.]),
            ([0., 1., 1.], [0., 0.]),
        ],
    },
    Face {
        dir: [0, 0, -1],
        shade: 0.8,
        corners: [
            ([1., 0., 0.], [0., 1.]),
            ([0., 0., 0.], [1., 1.]),
            ([0., 1., 0.], [1., 0.]),
            ([1., 1., 0.], [0., 0.]),
        ],
    },
];

const WATER_TINT: [f32; 3] = [0.25, 0.45, 0.9];
const GRASS_TINT: [f32; 3] = [0.57, 0.74, 0.35];
const FOLIAGE_TINT: [f32; 3] = [0.28, 0.71, 0.09];
const WATER_TOP: f32 = 0.875;

fn face_visible(self_id: u16, self_kind: Kind, neighbor_id: u16) -> bool {
    let n = look(neighbor_id >> 4);
    match self_kind {
        Kind::Opaque => n.kind != Kind::Opaque,
        Kind::Cutout => {
            n.kind != Kind::Opaque && !(n.kind == Kind::Cutout && neighbor_id >> 4 == self_id >> 4)
        }
        Kind::Water => n.kind != Kind::Opaque && n.kind != Kind::Water,
        Kind::Skip => false,
    }
}

/// Mesh the chunk into opaque/cutout geometry and blended water geometry, in chunk-local coordinates.
pub fn mesh_chunk(hood: &Neighborhood, atlas: &Atlas) -> Meshes {
    let mut out = Meshes::default();
    for y in 0..HEIGHT as i32 {
        for z in 0..16 {
            for x in 0..16 {
                let state = hood.block(x, y, z);
                if state >> 4 == 0 {
                    continue;
                }
                mesh_block(hood, atlas, [x, y, z], state, &mut out);
            }
        }
    }
    out
}

fn mesh_block(hood: &Neighborhood, atlas: &Atlas, at: [i32; 3], state: u16, out: &mut Meshes) {
    let id = state >> 4;
    let l = look(id);
    if l.kind == Kind::Skip {
        return;
    }
    let water_surface =
        l.kind == Kind::Water && look(hood.block(at[0], at[1] + 1, at[2]) >> 4).kind != Kind::Water;
    for face in &FACES {
        let n = [
            at[0] + face.dir[0],
            at[1] + face.dir[1],
            at[2] + face.dir[2],
        ];
        if !face_visible(state, l.kind, hood.block(n[0], n[1], n[2])) {
            continue;
        }
        let (name, tint) = match face.dir {
            [0, 1, 0] => (l.top, l.tint),
            [0, -1, 0] => (
                l.bottom,
                if l.kind == Kind::Cutout {
                    l.tint
                } else {
                    Tint::None
                },
            ),
            _ => (
                l.side,
                if l.kind == Kind::Cutout {
                    l.tint
                } else {
                    Tint::None
                },
            ),
        };
        let rgb = face_color(atlas, &l, name, tint);
        let alpha = if l.kind == Kind::Water { 0.7 } else { 1.0 };
        let color = [
            rgb[0] * face.shade,
            rgb[1] * face.shade,
            rgb[2] * face.shade,
            alpha,
        ];
        let target = if l.kind == Kind::Water {
            &mut out.water
        } else {
            &mut out.opaque
        };
        let base = target.vertices.len() as u32;
        for (corner, uv) in face.corners {
            let mut y = corner[1];
            if water_surface && y == 1.0 {
                y = WATER_TOP;
            }
            target.vertices.push(Vertex {
                pos: [
                    at[0] as f32 + corner[0],
                    at[1] as f32 + y,
                    at[2] as f32 + corner[2],
                ],
                uv: atlas.uv(name, uv),
                color,
            });
        }
        target
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn face_color(atlas: &Atlas, l: &crate::blocks::Look, _name: &str, tint: Tint) -> [f32; 3] {
    if !atlas.textured {
        return l.color;
    }
    match (l.kind, tint) {
        (Kind::Water, _) => WATER_TINT,
        (Kind::Cutout, Tint::Grass) => FOLIAGE_TINT,
        (_, Tint::Grass) => GRASS_TINT,
        _ => [1.0, 1.0, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(f: impl Fn(usize, usize, usize) -> u16) -> Vec<u16> {
        let mut v = vec![0u16; 16 * 16 * HEIGHT];
        for y in 0..HEIGHT {
            for z in 0..16 {
                for x in 0..16 {
                    v[index(x, y, z)] = f(x, y, z);
                }
            }
        }
        v
    }

    #[test]
    fn single_block_has_six_faces() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 1 << 4 } else { 0 });
        let m = mesh_chunk(
            &Neighborhood {
                center: &c,
                neighbors: [None; 4],
            },
            &Atlas::flat(),
        );
        assert_eq!(m.opaque.vertices.len(), 24);
        assert_eq!(m.opaque.indices.len(), 36);
    }

    #[test]
    fn adjacent_blocks_hide_shared_faces() {
        let c = flat(|x, y, z| {
            if y == 70 && z == 4 && (x == 4 || x == 5) {
                1 << 4
            } else {
                0
            }
        });
        let m = mesh_chunk(
            &Neighborhood {
                center: &c,
                neighbors: [None; 4],
            },
            &Atlas::flat(),
        );
        assert_eq!(m.opaque.vertices.len(), 10 * 4);
    }

    #[test]
    fn neighbor_chunk_hides_border_face() {
        let a = flat(|x, y, z| if (x, y, z) == (15, 70, 4) { 1 << 4 } else { 0 });
        let b = flat(|x, y, z| if (x, y, z) == (0, 70, 4) { 1 << 4 } else { 0 });
        let alone = mesh_chunk(
            &Neighborhood {
                center: &a,
                neighbors: [None; 4],
            },
            &Atlas::flat(),
        );
        let next = mesh_chunk(
            &Neighborhood {
                center: &a,
                neighbors: [None, Some(&b), None, None],
            },
            &Atlas::flat(),
        );
        assert_eq!(alone.opaque.vertices.len() - next.opaque.vertices.len(), 4);
    }

    #[test]
    fn water_next_to_water_is_culled_and_goes_to_water_mesh() {
        let c = flat(|x, y, z| {
            if y == 60 && z == 4 && (x == 4 || x == 5) {
                9 << 4
            } else {
                0
            }
        });
        let m = mesh_chunk(
            &Neighborhood {
                center: &c,
                neighbors: [None; 4],
            },
            &Atlas::flat(),
        );
        assert!(m.opaque.is_empty());
        assert_eq!(m.water.vertices.len(), 10 * 4);
        assert!(m.water.vertices.iter().any(|v| v.pos[1] == 60.875));
    }
}
