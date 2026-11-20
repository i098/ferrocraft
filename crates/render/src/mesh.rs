//! CPU chunk meshing: one quad per visible block face with vanilla smooth lighting and ambient occlusion
//! (one flat light per face when smooth lighting is off),
//! plus crosses, liquids and box shapes (faces next to an opaque block are culled, including across chunk borders).

use crate::assets::Atlas;
use crate::biome::{blended, Tint as BiomeTint};
use crate::blocks::{
    cached, is_opaque_cube, material_solid, tint_color, uses_neighbor_light, Kind, Look, Tint,
    DOWN, NORTH, UP,
};
use crate::light::Light;
use crate::models::Quad;
use crate::shapes::{boxes, Bx, BREWING_SPOKES};
use bytemuck::{Pod, Zeroable};

mod flow;
mod plants;
mod variants;

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

#[derive(Clone, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Adds the triangles of `other` after this mesh's own.
    pub fn append(&mut self, other: MeshData) {
        let base = self.vertices.len() as u32;
        self.vertices.extend(other.vertices);
        self.indices
            .extend(other.indices.into_iter().map(|i| i + base));
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
    /// `SetVisibility` of each section, bottom first (see `cull::section_visibility`).
    pub vis: [u64; 16],
}

/// The chunk to mesh plus its eight neighbours (when loaded), row by row over `dz` then `dx` in `-1..=1`, skipping the chunk itself.
pub struct Neighborhood<'a> {
    /// Chunk coordinates of `center`.
    pub origin: (i32, i32),
    pub center: &'a Blocks,
    pub neighbors: [Option<&'a Blocks>; 8],
    /// Biome ids of the columns (index `z * 16 + x`) of the chunk and its neighbours, row by row over
    /// `dz` then `dx` in `-1..=1`; a missing column counts as plains.
    pub biomes: [Option<&'a [u8; 256]>; 9],
}

/// Chunk offsets `(dx, dz)` of `Neighborhood::neighbors`.
pub const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

impl Neighborhood<'_> {
    /// Block state id at chunk-local `x`, `z` in `-16..=31` and any `y`; 0 (air) outside the loaded data.
    pub(crate) fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(0..HEIGHT as i32).contains(&y) {
            return 0;
        }
        let (dx, dz) = (x.div_euclid(16), z.div_euclid(16));
        let data = match (dx, dz) {
            (0, 0) => Some(self.center),
            (-1..=1, -1..=1) => {
                let slot = ((dz + 1) * 3 + dx + 1) as usize;
                self.neighbors[slot - usize::from(slot > 4)]
            }
            _ => None,
        };
        data.map_or(0, |d| {
            d[index(
                x.rem_euclid(16) as usize,
                y as usize,
                z.rem_euclid(16) as usize,
            )]
        })
    }

    /// Biome id at chunk-local `x`, `z` in `-16..=31`; plains outside the loaded data.
    pub(crate) fn biome(&self, x: i32, z: i32) -> u8 {
        let slot = ((z.div_euclid(16) + 1) * 3 + x.div_euclid(16) + 1) as usize;
        let column = self.biomes.get(slot).copied().flatten();
        column.map_or(1, |b| {
            b[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize]
        })
    }

    /// Whether the chunk at offset `dx`, `dz` is loaded, for `dx` and `dz` in `-1..=1`.
    pub(crate) fn loaded(&self, dx: i32, dz: i32) -> bool {
        let slot = ((dz + 1) * 3 + dx + 1) as usize;
        slot == 4 || self.neighbors[slot - usize::from(slot > 4)].is_some()
    }
}

/// Face directions in `UP, DOWN, NORTH, SOUTH, WEST, EAST` order.
pub(crate) const DIRS: [[i32; 3]; 6] = [
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
pub(crate) const CORNERS: [[[f32; 3]; 4]; 6] = [
    [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
    [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
    [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
    [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]],
    [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]],
];

/// Texture coordinates of the block-local point `p` on `face` (vanilla default model UVs).
pub(crate) fn face_uv(face: usize, p: [f32; 3]) -> [f32; 2] {
    match face {
        0 => [p[0], p[2]],
        1 => [p[0], 1.0 - p[2]],
        2 => [1.0 - p[0], 1.0 - p[1]],
        3 => [p[0], 1.0 - p[1]],
        4 => [p[2], 1.0 - p[1]],
        _ => [1.0 - p[2], 1.0 - p[1]],
    }
}

pub(crate) fn rot_uv([u, v]: [f32; 2], quarter_turns: u8) -> [f32; 2] {
    (0..quarter_turns).fold([u, v], |[u, v], _| [v, 1.0 - u])
}

#[derive(Clone, Copy, PartialEq)]
enum Pass {
    Opaque,
    Blend,
}

pub fn view_opaque(state: u16, fancy: bool) -> bool {
    is_opaque_cube(state) || (!fancy && is_leaves(state))
}

fn is_leaves(state: u16) -> bool {
    matches!(state >> 4, 18 | 161)
}

/// Vertex alpha that tells `fs_world` to sample the atlas at level 0 (`CUTOUT`, not `MIPPED_CUTOUT`).
pub(crate) const UNMIPPED: f32 = 3.0;

/// Blocks of vanilla's `CUTOUT` layer, drawn after `setBlurMipmap(false, false)` (`EntityRenderer.renderWorldPass`):
/// plants, torches, rails, doors, glass, webs, fire and the like. Leaves, bars and panes are `CUTOUT_MIPPED`, so they and
/// every `SOLID` block keep the mip levels.
const CUTOUT_BLOCKS: [u16; 51] = [
    6, 20, 26, 27, 28, 30, 31, 32, 37, 38, 39, 40, 50, 51, 52, 55, 59, 64, 65, 66, 71, 75, 76, 81,
    83, 92, 93, 94, 96, 104, 105, 106, 111, 115, 117, 127, 138, 140, 141, 142, 149, 150, 157, 167,
    175, 193, 194, 195, 196, 197, 255,
];

fn is_unmipped(state: u16) -> bool {
    CUTOUT_BLOCKS.contains(&(state >> 4))
}

struct Mesher<'a> {
    hood: &'a Neighborhood<'a>,
    atlas: &'a Atlas,
    /// `GameSettings.ambientOcclusion != 0`: per-corner smooth lighting, else one flat light per face.
    smooth: bool,
    /// `GameSettings.fancyGraphics`: leaves are cutout cubes keeping inner faces; else solid, culled cubes.
    fancy: bool,
    light: Light,
    out: Meshes,
    /// Colors keep the tint only: no face shade and no ambient occlusion (the GUI lights the block itself).
    flat: bool,
    /// The block being meshed draws in vanilla's `CUTOUT` layer: no mip levels (see [`is_unmipped`]).
    unmipped: bool,
}

/// Mesh the chunk into opaque/cutout geometry and blended geometry, in chunk-local coordinates.
pub fn mesh_chunk(hood: &Neighborhood, atlas: &Atlas, smooth: bool, fancy: bool) -> Meshes {
    mesh(hood, atlas, smooth, fancy, false)
}

/// [`mesh_chunk`] with vertex colors holding the tint alone, as vanilla's GUI item draws a block.
pub fn mesh_chunk_flat(hood: &Neighborhood, atlas: &Atlas, smooth: bool, fancy: bool) -> Meshes {
    mesh(hood, atlas, smooth, fancy, true)
}

fn mesh(hood: &Neighborhood, atlas: &Atlas, smooth: bool, fancy: bool, flat: bool) -> Meshes {
    let mut m = Mesher {
        flat,
        hood,
        atlas,
        unmipped: false,
        smooth,
        fancy,
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
    let vis = std::array::from_fn(|sy| {
        crate::cull::section_visibility(|x, y, z| {
            m.opaque(hood.block(x as i32, (sy * 16 + y) as i32, z as i32))
        })
    });
    m.out.vis = vis;
    m.out.light = Some(m.light);
    m.out
}

impl Mesher<'_> {
    /// A face shade, or 1 when the colors hold the tint alone.
    fn lit(&self, shade: f32) -> f32 {
        if self.flat {
            1.0
        } else {
            shade
        }
    }

    fn st(&self, at: [i32; 3], d: [i32; 3]) -> u16 {
        self.hood.block(at[0] + d[0], at[1] + d[1], at[2] + d[2])
    }

    fn light_at(&self, at: [i32; 3], d: [i32; 3]) -> [f32; 2] {
        let p = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
        let (s, b) = if uses_neighbor_light(self.hood.block(p[0], p[1], p[2])) {
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
        self.unmipped = is_unmipped(state);
        let l = *cached(state);
        match l.kind {
            Kind::Skip => {}
            Kind::Opaque | Kind::Cutout => self.cube(at, state, &l),
            Kind::Water | Kind::Lava => self.liquid(at, state, &l),
            Kind::Cross => self.cross(at, state, &l),
            // Chests are drawn as models (`blockent.rs`); their box only serves picking and cracks.
            Kind::Shape if matches!(state >> 4, 54 | 130 | 146) => {}
            Kind::Shape => {
                let nb = |dx, dy, dz| self.hood.block(at[0] + dx, at[1] + dy, at[2] + dz);
                if let Some(quads) = self.atlas.models.quads(state, &nb) {
                    self.modelled(at, quads, &l);
                    return;
                }
                let list = boxes(state, &nb);
                // `waterlily.json`: four y turns picked by the position hash.
                let orient = if state >> 4 == 111 {
                    self.orient_at(at, state)
                } else {
                    variants::Orient::default()
                };
                for b in &list {
                    self.boxed(at, b, Self::pass_of(state), orient);
                }
                if state >> 4 == 117 {
                    self.brewing_spokes(at, state & 7);
                }
            }
        }
    }

    fn pass_of(state: u16) -> Pass {
        match state >> 4 {
            8 | 9 | 79 | 90 | 95 | 160 | 165 => Pass::Blend,
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
    fn base_color(&self, l: &Look, tint: Tint, at: [i32; 3]) -> [f32; 3] {
        if !self.atlas.textured {
            return l.color;
        }
        let biome_tint = match tint {
            Tint::Grass => BiomeTint::Grass,
            Tint::Foliage => BiomeTint::Foliage,
            Tint::Water => BiomeTint::Water,
            _ => return tint_color(tint),
        };
        let (cx, cz) = self.hood.origin;
        let pos = (cx * 16 + at[0], at[1], cz * 16 + at[2]);
        blended(&self.atlas.colormaps, biome_tint, pos, |x, z| {
            self.hood.biome(x - cx * 16, z - cz * 16)
        })
    }

    fn push_quad(&mut self, pass: Pass, mut v: [Vertex; 4], flip: bool) {
        if self.unmipped && pass == Pass::Opaque {
            for vert in &mut v {
                vert.color[3] = UNMIPPED;
            }
        }
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
        if self.opaque(n) {
            return false;
        }
        // Glass, ice and slime hide the faces they share with the same block; fancy leaves keep them.
        !(l.kind == Kind::Cutout && n >> 4 == state >> 4 && !matches!(state >> 4, 18 | 161))
    }

    /// `Block.isOpaqueCube`: leaves are opaque on Fast (`BlockLeaves.isOpaqueCube` is `!fancyGraphics`).
    fn opaque(&self, state: u16) -> bool {
        view_opaque(state, self.fancy)
    }

    /// Leaves drawn as solid blocks: the world shader skips its alpha test for vertex alpha 2.
    fn solid_leaves(&self, state: u16) -> bool {
        !self.fancy && is_leaves(state)
    }

    /// The blockstate variant of the cube at `at` (see `variants`).
    fn orient_at(&self, at: [i32; 3], state: u16) -> variants::Orient {
        let world = [
            self.hood.origin.0 * 16 + at[0],
            at[1],
            self.hood.origin.1 * 16 + at[2],
        ];
        variants::orient(state, world, matches!(self.st(at, [0, 1, 0]) >> 4, 78 | 80))
    }

    fn cube(&mut self, at: [i32; 3], state: u16, l: &Look) {
        let pass = Self::pass_of(state);
        let emissive = crate::blocks::emission(state) > 0;
        let alpha = if self.solid_leaves(state) { 2.0 } else { 1.0 };
        let orient = self.orient_at(at, state);
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
            let rgb = self.base_color(l, tint, at);
            let mut verts = [Vertex::zeroed(); 4];
            let mut ao = [1.0f32; 4];
            for (i, c) in CORNERS[f].iter().enumerate() {
                let (light, a) = self.corner_light(at, f, *c, emissive);
                ao[i] = a;
                let k = self.lit(SHADE[f] * a);
                verts[i] = Vertex {
                    pos: [
                        at[0] as f32 + c[0],
                        at[1] as f32 + c[1],
                        at[2] as f32 + c[2],
                    ],
                    uv: self.atlas.uv(tex, orient.uv(f, *c)),
                    color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, alpha],
                    light,
                };
            }
            let flip = ao[0] + ao[2] > ao[1] + ao[3];
            self.push_quad(pass, verts, flip);
            if state >> 4 == 2 && f >= NORTH && tex == l.tex[f] && self.atlas.textured {
                self.grass_overlay(l, at, f, verts, flip);
            }
        }
    }

    /// `grass_side_overlay` over the side of a grass block, tinted by the biome (vanilla's second model layer).
    /// It sits a hair outside the side so the depth test keeps it above the untinted texture.
    fn grass_overlay(
        &mut self,
        l: &Look,
        at: [i32; 3],
        f: usize,
        mut verts: [Vertex; 4],
        flip: bool,
    ) {
        let tint = self.base_color(l, Tint::Grass, at);
        for (v, c) in verts.iter_mut().zip(CORNERS[f]) {
            // The side is untinted, so its red channel is the face shade times ambient occlusion.
            let shade = v.color[0];
            v.color = [tint[0] * shade, tint[1] * shade, tint[2] * shade, 1.0];
            v.uv = self.atlas.uv("grass_side_overlay", face_uv(f, c));
            for (p, d) in v.pos.iter_mut().zip(DIRS[f]) {
                *p += d as f32 * 0.001;
            }
        }
        self.push_quad(Pass::Opaque, verts, flip);
    }

    /// Light and occlusion of one face corner; flat lighting (and emissive blocks) skip the corner average.
    fn corner_light(&self, at: [i32; 3], f: usize, c: [f32; 3], emissive: bool) -> ([f32; 2], f32) {
        match (emissive, self.smooth) {
            (true, _) => (self.flat_light(at, DIRS[f]), 1.0),
            // `BlockModelRenderer.renderModelFlat`: the light of the cell the face looks at.
            (false, false) => (self.light_at(at, DIRS[f]), 1.0),
            (false, true) => self.smooth(at, f, c),
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
            if self.opaque(self.st(at, d)) {
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
        let rgb = self.base_color(l, l.tint, at);
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
        let gap = self.liquid_gap_above(at, &same);
        let flow = self.flow_angle(at, state, &same);
        for (f, &dir) in DIRS.iter().enumerate() {
            let n = self.st(at, dir);
            // `BlockLiquid.shouldSideBeRendered`: the top face shows even under an opaque block.
            if same(n) || (f != UP && self.opaque(n)) {
                continue;
            }
            let mut light = if f == UP {
                let up = self.light_at(at, [0, 1, 0]);
                [own[0].max(up[0]), own[1].max(up[1])]
            } else {
                self.flat_light(at, dir)
            };
            if lava {
                light[1] = 1.0;
            }
            let mut verts = self.liquid_face(at, f, &h, (l, flow.filter(|_| f == UP)), rgb, light);
            self.push_quad(pass, verts, false);
            if f >= NORTH || (f == UP && gap) {
                verts.reverse();
                self.push_quad(pass, verts, false);
            }
        }
    }

    /// `LiquidRenderer` draws sides from both sides, and the top too while a cell above is open
    /// (`isNeighboringGap`).
    fn liquid_gap_above(&self, at: [i32; 3], same: &dyn Fn(u16) -> bool) -> bool {
        (-1..=1).any(|dx| {
            (-1..=1).any(|dz| {
                let s = self.st(at, [dx, 1, dz]);
                !same(s) && !self.opaque(s)
            })
        })
    }

    fn liquid_face(
        &self,
        at: [i32; 3],
        f: usize,
        h: &[[f32; 2]; 2],
        (l, angle): (&Look, Option<f32>),
        rgb: [f32; 3],
        light: [f32; 2],
    ) -> [Vertex; 4] {
        let k = self.lit(SHADE[f]);
        // `BlockFluidRenderer`: a flowing top uses the `_flow` sprite turned by the flow angle.
        let tex = match angle {
            Some(_) => l.tex[2],
            None => l.tex[f],
        };
        CORNERS[f].map(|c| {
            let y = if c[1] > 0.5 {
                h[c[0] as usize][c[2] as usize]
            } else {
                0.0
            };
            let p = [c[0], y, c[2]];
            Vertex {
                pos: [
                    at[0] as f32 + p[0],
                    at[1] as f32 + p[1],
                    at[2] as f32 + p[2],
                ],
                uv: self.atlas.uv(
                    tex,
                    angle.map_or_else(|| liquid_uv(f, p), |a| flow::flow_uv(a, c)),
                ),
                color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, 1.0],
                light,
            }
        })
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
            } else if !material_solid(s) {
                f += 1.0;
                n += 1.0;
            }
        }
        1.0 - f / n
    }

    /// The three crossed planes of a brewing stand; a set bottle bit shows the bottle half of the texture.
    fn brewing_spokes(&mut self, at: [i32; 3], bottles: u16) {
        let tex = "brewing_stand";
        let light = self.light_at(at, [0; 3]);
        for (tip, i) in BREWING_SPOKES {
            let (u_centre, u_tip) = if bottles >> i & 1 == 1 {
                (0.0, 0.5)
            } else {
                (1.0, 0.5)
            };
            let ends = [([8.0, 8.0], u_centre), (tip, u_tip)];
            let corner = |(xz, u): ([f32; 2], f32), y: f32| Vertex {
                pos: [
                    at[0] as f32 + xz[0] / 16.0,
                    at[1] as f32 + y,
                    at[2] as f32 + xz[1] / 16.0,
                ],
                uv: self.atlas.uv(tex, [u, 1.0 - y]),
                color: [1.0, 1.0, 1.0, 1.0],
                light,
            };
            let quad = [
                corner(ends[0], 0.0),
                corner(ends[1], 0.0),
                corner(ends[1], 1.0),
                corner(ends[0], 1.0),
            ];
            let mut back = quad;
            back.reverse();
            self.push_quad(Pass::Opaque, quad, false);
            self.push_quad(Pass::Opaque, back, false);
        }
    }

    /// Quads baked from the block's vanilla model: explicit uv, face and element rotation are kept.
    fn modelled(&mut self, at: [i32; 3], quads: &[Quad], l: &Look) {
        let (plain, tinted) = (
            self.base_color(l, Tint::None, at),
            self.base_color(l, l.tint, at),
        );
        for q in quads {
            let rgb = if q.tint { tinted } else { plain };
            if q.cull.is_some_and(|c| self.opaque(self.st(at, DIRS[c]))) {
                continue;
            }
            let light = self.flat_light(at, q.cull.map_or([0; 3], |c| DIRS[c]));
            let k = if q.shade {
                self.lit(SHADE[q.face])
            } else {
                1.0
            };
            let verts = std::array::from_fn(|i| Vertex {
                pos: [0, 1, 2].map(|a| at[a] as f32 + q.pos[i][a]),
                uv: self.atlas.uv(&q.tex, q.uv[i]),
                color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, 1.0],
                light,
            });
            self.push_quad(Pass::Opaque, verts, false);
        }
    }

    fn boxed(&mut self, at: [i32; 3], b: &Bx, pass: Pass, orient: variants::Orient) {
        let l = Look {
            kind: Kind::Shape,
            tint: b.tint,
            color: [0.6; 3],
            tex: b.tex,
        };
        let rgb = self.base_color(&l, b.tint, at);
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
            if b.cull && on_border && self.opaque(self.st(at, DIRS[f])) {
                continue;
            }
            let light = self.flat_light(at, if on_border { DIRS[f] } else { [0; 3] });
            let mut verts = [Vertex::zeroed(); 4];
            for (i, c) in CORNERS[f].iter().enumerate() {
                let p = [0, 1, 2].map(|a| if c[a] > 0.5 { b.max[a] } else { b.min[a] });
                let mut uv = orient.uv(f, p);
                if f == UP {
                    uv = rot_uv(uv, b.rot);
                }
                let k = self.lit(SHADE[f]);
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

/// `MathHelper.getPositionRandom`: the hash vanilla keys block offsets and model variants on.
pub(crate) fn position_random(x: i32, y: i32, z: i32) -> i64 {
    let i = x.wrapping_mul(3129871) as i64 ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    i.wrapping_mul(i)
        .wrapping_mul(42317861)
        .wrapping_add(i.wrapping_mul(11))
}

/// Quads of a plant model for the block-break overlay (see `plants::cross_quads`).
pub(crate) fn cross_quads(id: u16, at: [i32; 3]) -> Vec<[([f32; 3], [f32; 2]); 4]> {
    plants::cross_quads(id, at)
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

    fn mesh(c: &[u16], neighbors: [Option<&[u16]>; 8]) -> Meshes {
        mesh_smooth(c, neighbors, true)
    }

    fn mesh_smooth(c: &[u16], neighbors: [Option<&[u16]>; 8], smooth: bool) -> Meshes {
        mesh_chunk(
            &Neighborhood {
                origin: (0, 0),
                center: c,
                neighbors,
                biomes: [None; 9],
            },
            &Atlas::flat(),
            smooth,
            true,
        )
    }

    #[test]
    fn cutout_layer_blocks_skip_mip_levels_and_solid_ones_keep_them() {
        // Poppy (`CUTOUT`), oak leaves (`MIPPED_CUTOUT`) and stone (`SOLID`) side by side.
        let c = flat(|x, y, z| match (x, y, z) {
            (2, 70, 4) => 38 << 4,
            (5, 70, 4) => 18 << 4,
            (8, 70, 4) => 1 << 4,
            _ => 0,
        });
        let quads = mesh(&c, [None; 8]).opaque.vertices;
        let by_x = |lo: f32, hi: f32| -> Vec<f32> {
            quads
                .iter()
                .filter(|v| (lo..hi).contains(&v.pos[0]))
                .map(|v| v.color[3])
                .collect()
        };
        let (poppy, leaves, stone) = (by_x(1.5, 3.6), by_x(5.0, 6.0), by_x(8.0, 9.0));
        assert!(!poppy.is_empty() && poppy.iter().all(|&a| a == UNMIPPED));
        assert!(!leaves.is_empty() && leaves.iter().all(|&a| a == 1.0));
        assert!(!stone.is_empty() && stone.iter().all(|&a| a == 1.0));
    }

    #[test]
    fn fast_leaves_are_solid_and_cull_inner_faces() {
        // Two oak leaves side by side: `BlockLeaves.isOpaqueCube` is `!fancyGraphics`.
        let c = flat(|x, y, z| {
            if y == 70 && z == 4 && (x == 4 || x == 5) {
                18 << 4
            } else {
                0
            }
        });
        let leaves = |fancy| {
            let hood = Neighborhood {
                origin: (0, 0),
                center: &c,
                neighbors: [None; 8],
                biomes: [None; 9],
            };
            mesh_chunk(&hood, &Atlas::flat(), true, fancy)
                .opaque
                .vertices
        };
        let (fast, fancy) = (leaves(false), leaves(true));
        assert_eq!(fancy.len(), 12 * 4, "fancy keeps the inner faces");
        assert!(
            fancy.iter().all(|v| v.color[3] == 1.0),
            "fancy is alpha tested"
        );
        assert_eq!(fast.len(), 10 * 4, "fast hides the shared faces");
        assert!(
            fast.iter().all(|v| v.color[3] > 1.5),
            "fast skips the alpha test"
        );
    }

    #[test]
    fn flat_lighting_gives_every_face_one_light() {
        // The block above-beside shades the top corners next to it only under smooth lighting.
        let c = flat(|x, y, z| match (x, y, z) {
            (4, 70, 4) | (5, 71, 4) => 1 << 4,
            _ => 0,
        });
        let faces_equal = |m: &Meshes| {
            m.opaque
                .vertices
                .chunks(4)
                .all(|q| q.iter().all(|v| v.light == q[0].light))
        };
        assert!(!faces_equal(&mesh_smooth(&c, [None; 8], true)));
        assert!(faces_equal(&mesh_smooth(&c, [None; 8], false)));
    }

    #[test]
    fn single_block_has_six_faces() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 1 << 4 } else { 0 });
        let m = mesh(&c, [None; 8]);
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
        assert_eq!(mesh(&c, [None; 8]).opaque.vertices.len(), 10 * 4);
    }

    #[test]
    fn neighbor_chunk_hides_border_face() {
        let a = flat(|x, y, z| if (x, y, z) == (15, 70, 4) { 1 << 4 } else { 0 });
        let b = flat(|x, y, z| if (x, y, z) == (0, 70, 4) { 1 << 4 } else { 0 });
        let alone = mesh(&a, [None; 8]);
        let next = mesh(&a, [None, None, None, None, Some(&b), None, None, None]);
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
        let m = mesh(&c, [None; 8]);
        assert!(m.opaque.is_empty());
        // 2 tops and 2 bottoms, plus 6 open sides drawn from both sides and 2 top back faces (air above).
        assert_eq!(m.water.vertices.len(), 18 * 4);
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
        let m = mesh(&c, [None; 8]);
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
        let m = mesh(&c, [None; 8]);
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
        let m = mesh(&c, [None; 8]);
        assert!(m.opaque.vertices.iter().all(|v| v.pos[1] <= 70.5));
        assert!(m.opaque.vertices.iter().any(|v| v.pos[1] == 70.5));
    }

    #[test]
    fn plant_is_two_crossed_quads_drawn_from_both_sides() {
        let c = flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 37 << 4 } else { 0 });
        assert_eq!(mesh(&c, [None; 8]).opaque.vertices.len(), 16);
    }

    #[test]
    fn water_top_back_face_needs_a_gap_above() {
        // `LiquidRenderer` draws the top's underside only while `isNeighboringGap` sees a non-opaque cell above.
        let walled = |gap: bool| {
            flat(move |x, y, z| match (x, y, z) {
                (4, 60, 4) => 9 << 4,
                (5, 61, 5) if gap => 0,
                (3..=5, 61, 3..=5) => 1 << 4,
                (3, 60, 4) | (5, 60, 4) | (4, 60, 3) | (4, 60, 5) | (4, 59, 4) => 1 << 4,
                _ => 0,
            })
        };
        assert_eq!(mesh(&walled(false), [None; 8]).water.vertices.len(), 4);
        assert_eq!(mesh(&walled(true), [None; 8]).water.vertices.len(), 8);
    }

    #[test]
    fn stained_glass_pane_blends_and_packed_ice_does_not() {
        let at = |id: u16| flat(move |x, y, z| if (x, y, z) == (4, 70, 4) { id << 4 } else { 0 });
        let pane = mesh(&at(160), [None; 8]);
        assert!(pane.opaque.is_empty() && !pane.water.is_empty());
        let ice = mesh(&at(174), [None; 8]);
        assert!(ice.water.is_empty() && !ice.opaque.is_empty());
    }

    #[test]
    fn nether_portal_is_a_blended_slab() {
        let m = mesh(
            &flat(|x, y, z| if (x, y, z) == (4, 70, 4) { 90 << 4 } else { 0 }),
            [None; 8],
        );
        assert!(m.opaque.is_empty());
        assert_eq!(m.water.vertices.len(), 8, "two broad faces only");
    }

    #[test]
    #[ignore = "needs a JDK and downloads the 1.8.9 client jar"]
    fn unmipped_blocks_are_the_cutout_layer_of_the_1_8_9_jar() {
        let layers = oracle::Oracle::get().run("layer\n").unwrap();
        assert_eq!(layers.len(), 256, "{layers:?}");
        for (id, layer) in layers.iter().enumerate() {
            assert_eq!(
                is_unmipped((id as u16) << 4),
                layer == "Cutout",
                "block {id} is {layer}"
            );
        }
    }
}
