//! CPU chunk meshing: one quad per visible block face with vanilla smooth lighting and ambient occlusion,
//! plus crosses, liquids and box shapes (faces next to an opaque block are culled, including across chunk borders).

use crate::assets::Atlas;
use crate::blocks::{
    cached, double_plant_top, is_opaque_cube, tint_color, Kind, Look, Tint, DOWN, NORTH, UP,
};
use crate::light::Light;
use crate::shapes::{boxes, Bx};
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
    /// Tint, face shade and ambient occlusion; alpha is 1.
    pub color: [f32; 4],
    /// Sky and block light, `level / 15`.
    pub light: [f32; 2],
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
    /// Opaque and alpha-tested geometry (lava included).
    pub opaque: MeshData,
    /// Blended geometry: water, ice, stained glass, slime.
    pub water: MeshData,
    /// Light of the chunk, `x` and `z` in `-1..=16`.
    pub light: Option<Light>,
}

/// The chunk to mesh plus the neighbours in -x, +x, -z, +z order (when loaded).
pub struct Neighborhood<'a> {
    /// Chunk coordinates of `center`.
    pub origin: (i32, i32),
    pub center: &'a Blocks,
    pub neighbors: [Option<&'a Blocks>; 4],
}

impl Neighborhood<'_> {
    /// Block state id at chunk-local `x`, `z` in `-1..=16` and any `y`; 0 (air) outside the loaded data.
    pub(crate) fn block(&self, x: i32, y: i32, z: i32) -> u16 {
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

/// Face directions in `UP, DOWN, NORTH, SOUTH, WEST, EAST` order.
const DIRS: [[i32; 3]; 6] = [
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
/// Vanilla per-face brightness.
const SHADE: [f32; 6] = [1.0, 0.5, 0.8, 0.8, 0.6, 0.6];
/// Corners of each face, counter-clockwise seen from outside.
const CORNERS: [[[f32; 3]; 4]; 6] = [
    [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
    [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
    [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
    [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]],
    [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]],
];

/// Texture coordinates of the block-local point `p` on `face` (vanilla default model UVs).
fn face_uv(face: usize, p: [f32; 3]) -> [f32; 2] {
    match face {
        0 => [p[0], p[2]],
        1 => [p[0], 1.0 - p[2]],
        2 => [1.0 - p[0], 1.0 - p[1]],
        3 => [p[0], 1.0 - p[1]],
        4 => [p[2], 1.0 - p[1]],
        _ => [1.0 - p[2], 1.0 - p[1]],
    }
}

fn rot_uv([u, v]: [f32; 2], quarter_turns: u8) -> [f32; 2] {
    (0..quarter_turns).fold([u, v], |[u, v], _| [v, 1.0 - u])
}

#[derive(Clone, Copy, PartialEq)]
enum Pass {
    Opaque,
    Blend,
}

struct Mesher<'a> {
    hood: &'a Neighborhood<'a>,
    atlas: &'a Atlas,
    light: Light,
    out: Meshes,
}

/// Mesh the chunk into opaque/cutout geometry and blended geometry, in chunk-local coordinates.
pub fn mesh_chunk(hood: &Neighborhood, atlas: &Atlas) -> Meshes {
    let mut m = Mesher {
        hood,
        atlas,
        light: Light::compute(hood),
        out: Meshes::default(),
    };
    for y in 0..HEIGHT as i32 {
        for z in 0..16 {
            for x in 0..16 {
                let state = hood.block(x, y, z);
                if state >> 4 != 0 {
                    m.block([x, y, z], state);
                }
            }
        }
    }
    m.out.light = Some(m.light);
    m.out
}

impl Mesher<'_> {
    fn st(&self, at: [i32; 3], d: [i32; 3]) -> u16 {
        self.hood.block(at[0] + d[0], at[1] + d[1], at[2] + d[2])
    }

    fn light_at(&self, at: [i32; 3], d: [i32; 3]) -> [f32; 2] {
        let p = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
        let (s, b) = if matches!(
            self.hood.block(p[0], p[1], p[2]) >> 4,
            44 | 126 | 182 | 53 | 67 | 108 | 109 | 114 | 128 | 134
                ..=136 | 156 | 163 | 164 | 180 | 60
        ) {
            [[0, 1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]]
                .iter()
                .map(|o| self.light.get(p[0] + o[0], p[1] + o[1], p[2] + o[2]))
                .fold((0, 0), |(s, b), (s2, b2)| (s.max(s2), b.max(b2)))
        } else {
            self.light.get(p[0], p[1], p[2])
        };
        [s as f32 / 15.0, b as f32 / 15.0]
    }

    /// Brighter of the block's own cell and its neighbour in direction `d`.
    fn flat_light(&self, at: [i32; 3], d: [i32; 3]) -> [f32; 2] {
        let (a, b) = (self.light_at(at, [0; 3]), self.light_at(at, d));
        [a[0].max(b[0]), a[1].max(b[1])]
    }

    fn block(&mut self, at: [i32; 3], state: u16) {
        let l = *cached(state);
        match l.kind {
            Kind::Skip => {}
            Kind::Opaque | Kind::Cutout => self.cube(at, state, &l),
            Kind::Water | Kind::Lava => self.liquid(at, state, &l),
            Kind::Cross => self.cross(at, state, &l),
            Kind::Shape => {
                let nb = |dx, dy, dz| self.hood.block(at[0] + dx, at[1] + dy, at[2] + dz);
                let list = boxes(state, &nb);
                for b in &list {
                    self.boxed(at, b, Self::pass_of(state));
                }
            }
        }
    }

    fn pass_of(state: u16) -> Pass {
        match state >> 4 {
            8 | 9 | 79 | 95 | 160 | 165 => Pass::Blend,
            _ => Pass::Opaque,
        }
    }

    fn target(&mut self, p: Pass) -> &mut MeshData {
        match p {
            Pass::Opaque => &mut self.out.opaque,
            Pass::Blend => &mut self.out.water,
        }
    }

    /// Color a texel is multiplied by; flat colors stand in for the texture when no assets are loaded.
    fn base_color(&self, l: &Look, tint: Tint) -> [f32; 3] {
        if self.atlas.textured {
            tint_color(tint)
        } else {
            l.color
        }
    }

    fn push_quad(&mut self, pass: Pass, v: [Vertex; 4], flip: bool) {
        let t = self.target(pass);
        let base = t.vertices.len() as u32;
        t.vertices.extend_from_slice(&v);
        let idx: [u32; 6] = if flip {
            [1, 2, 3, 1, 3, 0]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        t.indices.extend(idx.map(|i| base + i));
    }

    fn visible(&self, state: u16, l: &Look, n: u16) -> bool {
        if is_opaque_cube(n) {
            return false;
        }
        // Glass, ice and slime hide the faces they share with the same block; leaves keep them (fancy graphics).
        !(l.kind == Kind::Cutout && n >> 4 == state >> 4 && !matches!(state >> 4, 18 | 161))
    }

    fn cube(&mut self, at: [i32; 3], state: u16, l: &Look) {
        let pass = Self::pass_of(state);
        let emissive = crate::blocks::emission(state) > 0;
        for f in 0..6 {
            let n = self.st(at, DIRS[f]);
            if !self.visible(state, l, n) {
                continue;
            }
            let mut tex = l.tex[f];
            if state >> 4 == 2 && f >= NORTH && matches!(self.st(at, [0, 1, 0]) >> 4, 78 | 80) {
                tex = "grass_side_snowed";
            }
            let tint = match (l.kind, f) {
                (Kind::Opaque, UP) | (Kind::Cutout, _) => l.tint,
                _ => Tint::None,
            };
            let rgb = self.base_color(l, tint);
            let mut verts = [Vertex::zeroed(); 4];
            let mut ao = [1.0f32; 4];
            for (i, c) in CORNERS[f].iter().enumerate() {
                let (light, a) = if emissive {
                    (self.flat_light(at, DIRS[f]), 1.0)
                } else {
                    self.smooth(at, f, *c)
                };
                ao[i] = a;
                let k = SHADE[f] * a;
                verts[i] = Vertex {
                    pos: [
                        at[0] as f32 + c[0],
                        at[1] as f32 + c[1],
                        at[2] as f32 + c[2],
                    ],
                    uv: self.atlas.uv(tex, face_uv(f, *c)),
                    color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, 1.0],
                    light,
                };
            }
            self.push_quad(pass, verts, ao[0] + ao[2] > ao[1] + ao[3]);
        }
    }

    /// Vanilla smooth lighting at one corner of a face: light is the average of the four cells around
    /// the corner on the face's outer side; occlusion darkens corners next to opaque blocks.
    fn smooth(&self, at: [i32; 3], face: usize, corner: [f32; 3]) -> ([f32; 2], f32) {
        let dir = DIRS[face];
        let axis = dir.iter().position(|d| *d != 0).unwrap();
        let (b, c) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let sign = |a: usize| if corner[a] > 0.5 { 1 } else { -1 };
        let off = |db: i32, dc: i32| {
            let mut d = dir;
            d[b] += db * sign(b);
            d[c] += dc * sign(c);
            d
        };
        let base = self.light_at(at, dir);
        let sample = |d: [i32; 3]| {
            if is_opaque_cube(self.st(at, d)) {
                (base, 0.2)
            } else {
                (self.light_at(at, d), 1.0)
            }
        };
        let (s1, a1) = sample(off(1, 0));
        let (s2, a2) = sample(off(0, 1));
        let (sc, ac) = if a1 < 1.0 && a2 < 1.0 {
            (s1, 0.2)
        } else {
            sample(off(1, 1))
        };
        let light = [0, 1].map(|i| (base[i] + s1[i] + s2[i] + sc[i]) / 4.0);
        (light, (1.0 + a1 + a2 + ac) / 4.0)
    }

    fn liquid(&mut self, at: [i32; 3], state: u16, l: &Look) {
        let lava = l.kind == Kind::Lava;
        let same = |s: u16| matches!(s >> 4, 8 | 9) != lava && matches!(s >> 4, 8..=11);
        let pass = if lava { Pass::Opaque } else { Pass::Blend };
        let rgb = self.base_color(l, Tint::None);
        let h = [
            [
                self.corner_h(at, &same, 0, 0),
                self.corner_h(at, &same, 0, 1),
            ],
            [
                self.corner_h(at, &same, 1, 0),
                self.corner_h(at, &same, 1, 1),
            ],
        ];
        let own = self.light_at(at, [0; 3]);
        for f in 0..6 {
            let n = self.st(at, DIRS[f]);
            // `BlockLiquid.shouldSideBeRendered`: the top face shows even under an opaque block.
            if same(n) || (f != UP && is_opaque_cube(n)) {
                continue;
            }
            let mut light = if f == UP {
                let up = self.light_at(at, [0, 1, 0]);
                [own[0].max(up[0]), own[1].max(up[1])]
            } else {
                self.flat_light(at, DIRS[f])
            };
            if lava {
                light[1] = 1.0;
            }
            let mut verts = [Vertex::zeroed(); 4];
            for (i, c) in CORNERS[f].iter().enumerate() {
                let y = if c[1] > 0.5 {
                    h[c[0] as usize][c[2] as usize]
                } else {
                    0.0
                };
                let p = [c[0], y, c[2]];
                let uv = liquid_uv(f, p);
                let k = SHADE[f];
                verts[i] = Vertex {
                    pos: [
                        at[0] as f32 + p[0],
                        at[1] as f32 + p[1],
                        at[2] as f32 + p[2],
                    ],
                    uv: self.atlas.uv(l.tex[f], uv),
                    color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, 1.0],
                    light,
                };
            }
            self.push_quad(pass, verts, false);
        }
        let _ = state;
    }

    fn corner_h(&self, at: [i32; 3], same: &dyn Fn(u16) -> bool, cx: i32, cz: i32) -> f32 {
        let (mut f, mut n) = (0.0f32, 0.0f32);
        for i in 0..4 {
            let d = [cx - (i & 1), 0, cz - (i >> 1 & 1)];
            if same(self.st(at, [d[0], 1, d[2]])) {
                return 1.0;
            }
            let s = self.st(at, d);
            if same(s) {
                let m = s & 15;
                let pct = (if m >= 8 { 0 } else { m } + 1) as f32 / 9.0;
                if m >= 8 || m == 0 {
                    f += pct * 10.0;
                    n += 10.0;
                }
                f += pct;
                n += 1.0;
            } else if !matches!(cached(s).kind, Kind::Opaque | Kind::Cutout | Kind::Shape) {
                f += 1.0;
                n += 1.0;
            }
        }
        1.0 - f / n
    }

    fn cross(&mut self, at: [i32; 3], state: u16, l: &Look) {
        let (id, meta) = (state >> 4, state & 15);
        let (mut tex, mut tint) = (l.tex[0], l.tint);
        if id == 175 && meta & 8 != 0 {
            (tex, tint) = double_plant_top(self.st(at, [0, -1, 0]));
        }
        let rgb = self.base_color(l, tint);
        let light = self.light_at(at, [0; 3]);
        let (wx, wz) = (
            self.hood.origin.0 * 16 + at[0],
            self.hood.origin.1 * 16 + at[2],
        );
        let (ox, oz) = if matches!(id, 31 | 32 | 37 | 38 | 175) {
            plant_offset(wx, 0, wz)
        } else {
            (0.0, 0.0)
        };
        let planes: &[([f32; 2], [f32; 2])] = if matches!(id, 59 | 141 | 142) {
            &[
                ([0.25, 0.0], [0.25, 1.0]),
                ([0.75, 0.0], [0.75, 1.0]),
                ([0.0, 0.25], [1.0, 0.25]),
                ([0.0, 0.75], [1.0, 0.75]),
            ]
        } else {
            &[([0.05, 0.05], [0.95, 0.95]), ([0.05, 0.95], [0.95, 0.05])]
        };
        for (a, b) in planes {
            for (p, q) in [(a, b), (b, a)] {
                // Both windings so the plane shows from either side.
                let mut verts = [Vertex::zeroed(); 4];
                let corners = [
                    (p, 0.0, [0.0, 1.0]),
                    (q, 0.0, [1.0, 1.0]),
                    (q, 1.0, [1.0, 0.0]),
                    (p, 1.0, [0.0, 0.0]),
                ];
                for (v, (xz, y, uv)) in verts.iter_mut().zip(corners) {
                    *v = Vertex {
                        pos: [
                            at[0] as f32 + xz[0] + ox,
                            at[1] as f32 + y,
                            at[2] as f32 + xz[1] + oz,
                        ],
                        uv: self.atlas.uv(tex, uv),
                        color: [rgb[0], rgb[1], rgb[2], 1.0],
                        light,
                    };
                }
                self.push_quad(Pass::Opaque, verts, false);
            }
        }
    }

    fn boxed(&mut self, at: [i32; 3], b: &Bx, pass: Pass) {
        let l = Look {
            kind: Kind::Shape,
            tint: b.tint,
            color: [0.6; 3],
            tex: b.tex,
        };
        let rgb = self.base_color(&l, b.tint);
        for f in 0..6 {
            if b.mask >> f & 1 == 0 {
                continue;
            }
            let on_border = [
                b.max[1] >= 1.0,
                b.min[1] <= 0.0,
                b.min[2] <= 0.0,
                b.max[2] >= 1.0,
                b.min[0] <= 0.0,
                b.max[0] >= 1.0,
            ][f];
            if b.cull && on_border && is_opaque_cube(self.st(at, DIRS[f])) {
                continue;
            }
            let light = self.flat_light(at, if on_border { DIRS[f] } else { [0; 3] });
            let mut verts = [Vertex::zeroed(); 4];
            for (i, c) in CORNERS[f].iter().enumerate() {
                let p = [0, 1, 2].map(|a| if c[a] > 0.5 { b.max[a] } else { b.min[a] });
                let mut uv = face_uv(f, p);
                if f == UP {
                    uv = rot_uv(uv, b.rot);
                }
                let k = SHADE[f];
                verts[i] = Vertex {
                    pos: [
                        at[0] as f32 + p[0],
                        at[1] as f32 + p[1],
                        at[2] as f32 + p[2],
                    ],
                    uv: self.atlas.uv(b.tex[f], uv),
                    color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, 1.0],
                    light,
                };
            }
            self.push_quad(pass, verts, false);
        }
    }
}

fn liquid_uv(f: usize, p: [f32; 3]) -> [f32; 2] {
    let uv = face_uv(f, p);
    if f >= NORTH {
        [uv[0] * 0.5, (1.0 - p[1]) * 0.5]
    } else if f == DOWN {
        [p[0], p[2]]
    } else {
        uv
    }
}

/// Vanilla `Block.getOffset` for the XZ offset type: a stable pseudo-random shift of up to a quarter block.
fn plant_offset(x: i32, y: i32, z: i32) -> (f32, f32) {
    let mut i = x.wrapping_mul(3129871) as i64 ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    i = i
        .wrapping_mul(i)
        .wrapping_mul(42317861)
        .wrapping_add(i.wrapping_mul(11));
    let f = |shift: u32| ((i >> shift & 15) as f32 / 15.0 - 0.5) * 0.5;
    (f(16), f(24))
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

    fn mesh(c: &[u16], neighbors: [Option<&[u16]>; 4]) -> Meshes {
        mesh_chunk(
            &Neighborhood {
                origin: (0, 0),
                center: c,
                neighbors,
            },
            &Atlas::flat(),
        )
    }

    #[test]
    fn single_block_has_six_faces() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 1 << 4 } else { 0 });
        let m = mesh(&c, [None; 4]);
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
        assert_eq!(mesh(&c, [None; 4]).opaque.vertices.len(), 10 * 4);
    }

    #[test]
    fn neighbor_chunk_hides_border_face() {
        let a = flat(|x, y, z| if (x, y, z) == (15, 70, 4) { 1 << 4 } else { 0 });
        let b = flat(|x, y, z| if (x, y, z) == (0, 70, 4) { 1 << 4 } else { 0 });
        let alone = mesh(&a, [None; 4]);
        let next = mesh(&a, [None, Some(&b), None, None]);
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
        let m = mesh(&c, [None; 4]);
        assert!(m.opaque.is_empty());
        assert_eq!(m.water.vertices.len(), 10 * 4);
        // Corners average with the empty neighbours, so a lone source block is lower than 8/9.
        assert!(m.water.vertices.iter().all(|v| v.pos[1] < 60.89));
        assert!(m.water.vertices.iter().any(|v| v.pos[1] > 60.6));
    }

    #[test]
    fn water_under_water_is_full_height() {
        let c = flat(|x, y, z| {
            if (x, z) == (4, 4) && (59..=60).contains(&y) {
                9 << 4
            } else {
                0
            }
        });
        let m = mesh(&c, [None; 4]);
        assert!(m
            .water
            .vertices
            .iter()
            .any(|v| (v.pos[1] - 60.698).abs() < 1e-3));
        assert!(m.water.vertices.iter().all(|v| v.pos[1] <= 60.9));
        assert!(m.water.vertices.iter().any(|v| v.pos[1] == 60.0));
    }

    #[test]
    fn corner_next_to_a_wall_is_darker_than_open_floor() {
        // Floor at y=59 with a pillar at (5,60,4): the floor vertex touching the pillar is occluded.
        let c = flat(|x, y, z| match (x, y, z) {
            (_, 0..=59, _) => 1 << 4,
            (5, 60, 4) => 1 << 4,
            _ => 0,
        });
        let m = mesh(&c, [None; 4]);
        let at = |x: f32, z: f32| {
            m.opaque
                .vertices
                .iter()
                .filter(|v| v.pos == [x, 60.0, z])
                .map(|v| v.color[0])
                .fold(f32::MAX, f32::min)
        };
        assert!(at(5.0, 4.0) < at(8.0, 8.0) * 0.8);
    }

    #[test]
    fn bottom_slab_is_half_height() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 44 << 4 } else { 0 });
        let m = mesh(&c, [None; 4]);
        assert!(m.opaque.vertices.iter().all(|v| v.pos[1] <= 70.5));
        assert!(m.opaque.vertices.iter().any(|v| v.pos[1] == 70.5));
    }

    #[test]
    fn plant_is_two_crossed_quads_drawn_from_both_sides() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 37 << 4 } else { 0 });
        assert_eq!(mesh(&c, [None; 4]).opaque.vertices.len(), 16);
    }

    #[test]
    fn water_top_shows_under_a_solid_block() {
        let c = flat(|x, y, z| match (x, y, z) {
            (4, 60, 4) => 9 << 4,
            (3, 60, 4) | (5, 60, 4) | (4, 60, 3) | (4, 60, 5) | (4, 59, 4) | (4, 61, 4) => 1 << 4,
            _ => 0,
        });
        let m = mesh(&c, [None; 4]);
        assert_eq!(m.water.vertices.len(), 4);
    }

    #[test]
    fn stained_glass_pane_blends_and_packed_ice_does_not() {
        let at = |id: u16| flat(move |x, y, z| if (x, y, z) == (4, 70, 4) { id << 4 } else { 0 });
        let pane = mesh(&at(160), [None; 4]);
        assert!(pane.opaque.is_empty() && !pane.water.is_empty());
        let ice = mesh(&at(174), [None; 4]);
        assert!(ice.water.is_empty() && !ice.opaque.is_empty());
    }
}
