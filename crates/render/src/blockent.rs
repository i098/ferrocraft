//! Block entities of 1.8.9 that vanilla draws with models instead of chunk geometry: chests
//! (`TileEntityChestRenderer`, `TileEntityEnderChestRenderer`), signs (`TileEntitySignRenderer`),
//! skulls (`TileEntitySkullRenderer`, also on a worn head: `LayerCustomHead`) and banners
//! (`TileEntityBannerRenderer`). The renderer keeps the position of every such block and the data
//! the server sends for it, and meshes all of them again each frame.
//!
//! Not ported: the obfuscated, underlined and struck-through text styles of sign lines, glyphs
//! outside `ascii.png`, the destroy-stage overlay, and the sounds.

use crate::assets::{Font, Image};
use crate::entity::{add_box, rotate, Skin, Skins, V3};
use crate::mesh::{MeshData, Vertex};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

pub const CHEST: u16 = 54;
pub const SIGN_STANDING: u16 = 63;
pub const SIGN_WALL: u16 = 68;
pub const SKULL: u16 = 144;
pub const ENDER_CHEST: u16 = 130;
pub const TRAPPED_CHEST: u16 = 146;
pub const BANNER_STANDING: u16 = 176;
pub const BANNER_WALL: u16 = 177;
pub const ENCHANTING_TABLE: u16 = 116;

/// Block ids that carry a block entity drawn here.
pub fn is_tile(id: u16) -> bool {
    matches!(
        id,
        CHEST
            | ENDER_CHEST
            | TRAPPED_CHEST
            | SIGN_STANDING
            | SIGN_WALL
            | SKULL
            | BANNER_STANDING
            | BANNER_WALL
            | ENCHANTING_TABLE
    )
}

/// `TileEntity.getMaxRenderDistanceSquared`: 64 blocks.
const RENDER_DISTANCE_SQ: f64 = 4096.0;
/// Sign text is cut beyond 16 blocks.
const TEXT_DISTANCE_SQ: f64 = 256.0;
/// Width in font pixels of a sign line (`func_178908_a(.., 90, ..)`).
const SIGN_LINE_WIDTH: i32 = 90;
const SIGN_SCALE: f32 = 0.666_666_7;
const FONT_TEXTURE: &str = "textures/font/ascii.png";

/// The texture a batch of block-entity geometry samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tex {
    /// A jar path under `assets/minecraft/`, or a composed banner texture.
    Named(String),
    /// The shared entity skin atlas (player skins).
    Atlas,
}

/// Opening state of a chest lid (`TileEntityChest.lidAngle`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Lid {
    viewers: u8,
    prev: f32,
    now: f32,
}

/// Skull data of `TileEntitySkull`: `SkullType` and `Rot`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skull {
    pub kind: u8,
    pub rotation: u8,
    /// Key of the owner's skin in the renderer's player skin store.
    pub owner: Option<u128>,
}

/// Banner data of `TileEntityBanner`: `Base` and `Patterns`, colors as dye damage (0 black .. 15 white).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Banner {
    pub base: u8,
    /// Pattern code (`"bs"`, `"mc"`, ...) and dye damage of each layer.
    pub layers: Vec<(String, u8)>,
}

/// What the client knows about the block entities in the loaded chunks.
#[derive(Default)]
pub struct BlockEntities {
    /// Block state of every tile block.
    blocks: HashMap<[i32; 3], u16>,
    lids: HashMap<[i32; 3], Lid>,
    signs: HashMap<[i32; 3], [String; 4]>,
    skulls: HashMap<[i32; 3], Skull>,
    banners: HashMap<[i32; 3], Banner>,
    books: HashMap<[i32; 3], Book>,
}

/// The camera and clock of one frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: [f64; 3],
    /// `World.getTotalWorldTime`, which sways banners.
    pub world_time: i64,
    pub partial: f32,
}

impl BlockEntities {
    /// Records the block at `pos`; data of a position whose block changes kind is dropped.
    pub fn set_state(&mut self, pos: [i32; 3], state: u16) {
        let id = state >> 4;
        if is_tile(id) {
            if self.blocks.insert(pos, state).is_some_and(|s| s >> 4 != id) {
                self.forget(pos);
            }
        } else if self.blocks.remove(&pos).is_some() {
            self.forget(pos);
        }
    }

    fn forget(&mut self, pos: [i32; 3]) {
        self.lids.remove(&pos);
        self.signs.remove(&pos);
        self.skulls.remove(&pos);
        self.banners.remove(&pos);
        self.books.remove(&pos);
    }

    /// Rebuilds the tile blocks of chunk (`cx`, `cz`); `at` reads a chunk-local block. Data of a
    /// position stays while that position holds a tile block of the same kind.
    pub fn load_chunk(&mut self, cx: i32, cz: i32, at: impl Fn(usize, usize, usize) -> u16) {
        let inside = |p: &[i32; 3]| p[0].div_euclid(16) == cx && p[2].div_euclid(16) == cz;
        let old = std::mem::take(&mut self.blocks);
        let mut fresh = HashMap::new();
        for y in 0..256 {
            for z in 0..16 {
                for x in 0..16 {
                    let state = at(x, y, z);
                    if is_tile(state >> 4) {
                        fresh.insert([cx * 16 + x as i32, y as i32, cz * 16 + z as i32], state);
                    }
                }
            }
        }
        let keep = |p: &[i32; 3]| {
            !inside(p)
                || fresh
                    .get(p)
                    .is_some_and(|s| old.get(p).is_none_or(|o| o >> 4 == s >> 4))
        };
        self.lids.retain(|p, _| keep(p));
        self.signs.retain(|p, _| keep(p));
        self.skulls.retain(|p, _| keep(p));
        self.banners.retain(|p, _| keep(p));
        self.books.retain(|p, _| keep(p));
        self.blocks = old;
        self.blocks.retain(|p, _| !inside(p));
        self.blocks.extend(fresh);
    }

    pub fn remove_chunk(&mut self, cx: i32, cz: i32) {
        let inside = |p: &[i32; 3]| p[0].div_euclid(16) == cx && p[2].div_euclid(16) == cz;
        self.blocks.retain(|p, _| !inside(p));
        self.lids.retain(|p, _| !inside(p));
        self.signs.retain(|p, _| !inside(p));
        self.skulls.retain(|p, _| !inside(p));
        self.banners.retain(|p, _| !inside(p));
        self.books.retain(|p, _| !inside(p));
    }

    pub fn clear(&mut self) {
        *self = BlockEntities::default();
    }

    /// `S24 BlockAction` for a chest: event 1 carries the number of viewers.
    pub fn block_action(&mut self, pos: [i32; 3], block: u16, event: u8, value: u8) {
        if matches!(block, CHEST | ENDER_CHEST | TRAPPED_CHEST) && event == 1 {
            self.lids.entry(pos).or_default().viewers = value;
        }
    }

    pub fn set_sign(&mut self, pos: [i32; 3], lines: [String; 4]) {
        self.signs.insert(pos, lines);
    }

    pub fn set_skull(&mut self, pos: [i32; 3], skull: Skull) {
        self.skulls.insert(pos, skull);
    }

    pub fn set_banner(&mut self, pos: [i32; 3], banner: Banner) {
        self.banners.insert(pos, banner);
    }

    /// One client tick of every chest lid (`TileEntityChest.update`) and enchanting table book
    /// (`TileEntityEnchantmentTable.update`); `players` are the positions of all players in the world.
    pub fn tick(&mut self, players: &[[f64; 3]]) {
        for (&pos, &state) in &self.blocks {
            if state >> 4 != ENCHANTING_TABLE {
                continue;
            }
            let centre = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ];
            // `World.getClosestPlayer(.., 3.0)`.
            let near = players
                .iter()
                .min_by(|a, b| dist3(a, &centre).total_cmp(&dist3(b, &centre)))
                .filter(|p| dist3(p, &centre) < 9.0)
                .map(|p| (p[0] - centre[0], p[2] - centre[2]));
            let seed = i64::from(pos[0]) ^ (i64::from(pos[1]) << 20) ^ (i64::from(pos[2]) << 40);
            self.books
                .entry(pos)
                .or_insert_with(|| Book::new(seed))
                .update(near);
        }
        for lid in self.lids.values_mut() {
            lid.prev = lid.now;
            let open = lid.viewers > 0;
            if (!open && lid.now > 0.0) || (open && lid.now < 1.0) {
                lid.now += if open { 0.1 } else { -0.1 };
                lid.now = lid.now.clamp(0.0, 1.0);
            }
        }
    }

    /// Banners in range, by the key of their composed texture.
    pub fn banners_in_range(&self, eye: [f64; 3]) -> impl Iterator<Item = (String, &Banner)> {
        self.blocks
            .iter()
            .filter(move |(p, s)| {
                matches!(*s >> 4, BANNER_STANDING | BANNER_WALL) && in_range(p, eye)
            })
            .map(|(p, _)| {
                let banner = self.banners.get(p).unwrap_or(&NO_BANNER);
                (banner_key(banner), banner)
            })
    }

    /// The geometry of every tile in range, grouped by texture.
    pub fn build(
        &self,
        view: &View,
        skins: &Skins,
        font: Option<&Font>,
        light: impl Fn([f64; 3]) -> [f32; 2],
    ) -> Vec<(Tex, MeshData)> {
        let mut out = Batches::default();
        for (&pos, &state) in &self.blocks {
            if !in_range(&pos, view.eye) {
                continue;
            }
            let center = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ];
            let level = light(center);
            let mark = out.mark();
            self.tile(&mut out, pos, state, view, font, skins);
            out.light_since(mark, level);
        }
        out.0
    }

    /// One tile at `pos` with block state `state`, by block id.
    fn tile(
        &self,
        out: &mut Batches,
        pos: [i32; 3],
        state: u16,
        view: &View,
        font: Option<&Font>,
        skins: &Skins,
    ) {
        let (id, meta) = (state >> 4, (state & 15) as i32);
        let at = Mat::at([pos[0].into(), pos[1].into(), pos[2].into()]);
        match id {
            CHEST | TRAPPED_CHEST | ENDER_CHEST => self.chest(out, pos, id, meta, at, view.partial),
            SIGN_STANDING | SIGN_WALL => {
                let near = dist_sq(&pos, view.eye) < TEXT_DISTANCE_SQ;
                let lines = self.signs.get(&pos).filter(|_| near);
                sign(out, id == SIGN_STANDING, meta, at, lines, font);
            }
            SKULL => {
                let skull = self.skulls.get(&pos).copied().unwrap_or_default();
                skull_block(out, meta & 7, skull, at, skins);
            }
            ENCHANTING_TABLE => {
                let book = self.books.get(&pos).copied().unwrap_or_default();
                book_block(out, &book, at, view.partial);
            }
            _ => {
                let banner = self.banners.get(&pos).unwrap_or(&NO_BANNER);
                banner_block(out, id == BANNER_STANDING, meta, pos, view, banner, at);
            }
        }
    }

    fn chest(&self, out: &mut Batches, pos: [i32; 3], id: u16, meta: i32, at: Mat, partial: f32) {
        let same = |dx, dz| {
            self.blocks
                .get(&[pos[0] + dx, pos[1], pos[2] + dz])
                .is_some_and(|s| s >> 4 == id)
        };
        let ender = id == ENDER_CHEST;
        let (x_neg, x_pos, z_neg, z_pos) = if ender {
            (false, false, false, false)
        } else {
            (same(-1, 0), same(1, 0), same(0, -1), same(0, 1))
        };
        // The half with a neighbour toward -x or -z draws nothing: the other half draws both.
        if x_neg || z_neg {
            return;
        }
        let double = x_pos || z_pos;
        let name = chest_texture(id, double);
        let turn = wall_turn(meta);
        let lid = self.lids.get(&pos).copied().unwrap_or_default();
        let open = lid.prev + (lid.now - lid.prev) * partial;
        let open = 1.0 - open;
        let angle = -((1.0 - open * open * open) * FRAC_PI_2);

        let mut m = at.translate([0.0, 1.0, 1.0]).scale([1.0, -1.0, -1.0]);
        m = m.translate([0.5, 0.5, 0.5]);
        if meta == 2 && x_pos {
            m = m.translate([1.0, 0.0, 0.0]);
        }
        if meta == 5 && z_pos {
            m = m.translate([0.0, 0.0, -1.0]);
        }
        m = m.rotate_y(turn).translate([-0.5, -0.5, -0.5]);

        let (width, tex) = if double {
            (30.0, [128.0, 64.0])
        } else {
            (14.0, [64.0, 64.0])
        };
        let knob_x = if double { 15.0 } else { 8.0 };
        let target = out.mesh(Tex::Named(format!("textures/entity/chest/{name}.png")));
        let lid_rot = [angle, 0.0, 0.0];
        for part in [
            Cube::new([0.0, -5.0, -14.0], [width, 5.0, 14.0], (0.0, 0.0))
                .at([1.0, 7.0, 15.0], lid_rot),
            Cube::new([-1.0, -2.0, -15.0], [2.0, 4.0, 1.0], (0.0, 0.0))
                .at([knob_x, 7.0, 15.0], lid_rot),
            Cube::new([0.0, 0.0, 0.0], [width, 10.0, 14.0], (0.0, 19.0))
                .at([1.0, 6.0, 1.0], [0.0; 3]),
        ] {
            part.emit(target, &m, tex, 0.0);
        }
    }
}

/// Texture file of a chest under `textures/entity/chest`.
fn chest_texture(id: u16, double: bool) -> &'static str {
    match (id, double) {
        (ENDER_CHEST, _) => "ender",
        (TRAPPED_CHEST, false) => "trapped",
        (TRAPPED_CHEST, true) => "trapped_double",
        (_, false) => "normal",
        (_, true) => "normal_double",
    }
}

fn dist3(p: &[f64; 3], c: &[f64; 3]) -> f64 {
    (0..3).map(|i| (p[i] - c[i]).powi(2)).sum()
}

const PI: f32 = std::f32::consts::PI;

/// Wraps `a` into [-pi, pi) the way the tile code does.
fn wrap_pi(mut a: f32) -> f32 {
    while a >= PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}

/// The animation state of `TileEntityEnchantmentTable` (fields by their MCP names).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Book {
    pub tick_count: i32,
    pub page_flip: f32,
    pub page_flip_prev: f32,
    flip_t: f32,
    flip_a: f32,
    pub book_spread: f32,
    pub book_spread_prev: f32,
    pub book_rotation: f32,
    pub book_rotation_prev: f32,
    t_rot: f32,
    rng: Option<crate::jrandom::JavaRandom>,
}

impl Book {
    fn new(seed: i64) -> Book {
        Book {
            rng: Some(crate::jrandom::JavaRandom::new(seed)),
            ..Book::default()
        }
    }

    /// `TileEntityEnchantmentTable.update`; `near` is the x and z offset from the block centre to the
    /// closest player within 3 blocks.
    pub fn update(&mut self, near: Option<(f64, f64)>) {
        self.book_spread_prev = self.book_spread;
        self.book_rotation_prev = self.book_rotation;
        if let Some((dx, dz)) = near {
            self.t_rot = dz.atan2(dx) as f32;
            self.book_spread += 0.1;
            let rng = self
                .rng
                .get_or_insert_with(|| crate::jrandom::JavaRandom::new(0));
            if self.book_spread < 0.5 || rng.next_int(40) == 0 {
                let before = self.flip_t;
                while before == self.flip_t {
                    self.flip_t += (rng.next_int(4) - rng.next_int(4)) as f32;
                }
            }
        } else {
            self.t_rot += 0.02;
            self.book_spread -= 0.1;
        }
        self.book_rotation = wrap_pi(self.book_rotation);
        self.t_rot = wrap_pi(self.t_rot);
        self.book_rotation += wrap_pi(self.t_rot - self.book_rotation) * 0.4;
        self.book_spread = self.book_spread.clamp(0.0, 1.0);
        self.tick_count += 1;
        self.page_flip_prev = self.page_flip;
        let f = ((self.flip_t - self.page_flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_a += (f - self.flip_a) * 0.9;
        self.page_flip += self.flip_a;
    }
}

/// Opening angle and page flips of `ModelBook` at `partial`: (`ticks`, left flip, right flip, spread).
fn book_pose(b: &Book, partial: f32) -> (f32, f32, f32, f32) {
    let flip = b.page_flip_prev + (b.page_flip - b.page_flip_prev) * partial;
    let clamp = |f: f32| ((f - f.trunc()) * 1.6 - 0.3).clamp(0.0, 1.0);
    (
        b.tick_count as f32 + partial,
        clamp(flip + 0.25),
        clamp(flip + 0.75),
        b.book_spread_prev + (b.book_spread - b.book_spread_prev) * partial,
    )
}

/// `ModelBook.setRotationAngles`: y rotation of the two covers, two page stacks and two flipping
/// pages, and the shared x of the pages' rotation point (model pixels).
fn book_angles(ticks: f32, flip_l: f32, flip_r: f32, spread: f32) -> ([f32; 6], f32) {
    let f = ((ticks * 0.02).sin() * 0.1 + 1.25) * spread;
    (
        [
            PI + f,
            -f,
            f,
            -f,
            f - f * 2.0 * flip_l,
            f - f * 2.0 * flip_r,
        ],
        f.sin(),
    )
}

/// `TileEntityEnchantmentTableRenderer.renderTileEntityAt`.
fn book_block(out: &mut Batches, book: &Book, at: Mat, partial: f32) {
    let (ticks, flip_l, flip_r, spread) = book_pose(book, partial);
    let turn = wrap_pi(book.book_rotation - book.book_rotation_prev);
    let yaw = book.book_rotation_prev + turn * partial;
    let m = at
        .translate([0.5, 0.75, 0.5])
        .translate([0.0, 0.1 + (ticks * 0.1).sin() * 0.01, 0.0])
        .rotate_y(-yaw * 180.0 / PI)
        .rotate_z(80.0);
    let (y, page_x) = book_angles(ticks, flip_l, flip_r, spread);
    let target = out.mesh(Tex::Named(
        "textures/entity/enchanting_table_book.png".into(),
    ));
    let tex = [64.0, 32.0];
    let part = |min: V3, size: V3, uv, pivot: V3, rot_y: f32| {
        Cube::new(min, size, uv).at(pivot, [0.0, rot_y, 0.0])
    };
    let page = [page_x, 0.0, 0.0];
    for cube in [
        part(
            [-6.0, -5.0, 0.0],
            [6.0, 10.0, 0.0],
            (0.0, 0.0),
            [0.0, 0.0, -1.0],
            y[0],
        ),
        part(
            [0.0, -5.0, 0.0],
            [6.0, 10.0, 0.0],
            (16.0, 0.0),
            [0.0, 0.0, 1.0],
            y[1],
        ),
        part(
            [-1.0, -5.0, 0.0],
            [2.0, 10.0, 0.0],
            (12.0, 0.0),
            [0.0; 3],
            PI / 2.0,
        ),
        part([0.0, -4.0, -0.99], [5.0, 8.0, 1.0], (0.0, 10.0), page, y[2]),
        part(
            [0.0, -4.0, -0.01],
            [5.0, 8.0, 1.0],
            (12.0, 10.0),
            page,
            y[3],
        ),
        part([0.0, -4.0, 0.0], [5.0, 8.0, 0.0], (24.0, 10.0), page, y[4]),
        part([0.0, -4.0, 0.0], [5.0, 8.0, 0.0], (24.0, 10.0), page, y[5]),
    ] {
        cube.emit(target, &m, tex, 0.0);
    }
}

static NO_BANNER: Banner = Banner {
    base: 0,
    layers: Vec::new(),
};

fn dist_sq(pos: &[i32; 3], eye: [f64; 3]) -> f64 {
    (0..3)
        .map(|i| (f64::from(pos[i]) + 0.5 - eye[i]).powi(2))
        .sum()
}

fn in_range(pos: &[i32; 3], eye: [f64; 3]) -> bool {
    dist_sq(pos, eye) < RENDER_DISTANCE_SQ
}

/// Mesh batches by texture.
#[derive(Default)]
struct Batches(Vec<(Tex, MeshData)>);

impl Batches {
    fn mesh(&mut self, tex: Tex) -> &mut MeshData {
        let i = self
            .0
            .iter()
            .position(|(t, _)| *t == tex)
            .unwrap_or_else(|| {
                self.0.push((tex, MeshData::default()));
                self.0.len() - 1
            });
        &mut self.0[i].1
    }

    /// Vertex counts per batch, to find the vertices one tile added.
    fn mark(&self) -> Vec<usize> {
        self.0.iter().map(|(_, m)| m.vertices.len()).collect()
    }

    /// Sets the light of every vertex added since `mark`.
    fn light_since(&mut self, mark: Vec<usize>, level: [f32; 2]) {
        for (i, (_, mesh)) in self.0.iter_mut().enumerate() {
            let from = mark.get(i).copied().unwrap_or(0);
            for v in &mut mesh.vertices[from..] {
                v.light = level;
            }
        }
    }
}

/// An affine map, the matrix stack of the fixed-function pipeline: each call multiplies on the
/// right, so the last call acts on a point first.
#[derive(Clone, Copy, Debug)]
pub struct Mat([[f32; 4]; 3]);

impl Mat {
    pub fn at(p: [f64; 3]) -> Mat {
        Mat([
            [1.0, 0.0, 0.0, p[0] as f32],
            [0.0, 1.0, 0.0, p[1] as f32],
            [0.0, 0.0, 1.0, p[2] as f32],
        ])
    }

    pub fn translate(mut self, t: V3) -> Mat {
        for row in &mut self.0 {
            row[3] += row[0] * t[0] + row[1] * t[1] + row[2] * t[2];
        }
        self
    }

    pub fn scale(mut self, s: V3) -> Mat {
        for row in &mut self.0 {
            for (v, k) in row.iter_mut().zip(s) {
                *v *= k;
            }
        }
        self
    }

    /// `glRotatef(deg, 0, 0, 1)`.
    pub fn rotate_z(self, deg: f32) -> Mat {
        let (s, c) = deg.to_radians().sin_cos();
        self.times([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]])
    }

    /// `glRotatef(deg, 0, 1, 0)`.
    pub fn rotate_y(self, deg: f32) -> Mat {
        let (s, c) = deg.to_radians().sin_cos();
        self.times([[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]])
    }

    /// `ModelRenderer.postRender` rotation: z, then y, then x (radians), so x acts first.
    pub fn rotate_zyx(self, rot: V3) -> Mat {
        let (sx, cx) = rot[0].sin_cos();
        let (sy, cy) = rot[1].sin_cos();
        let (sz, cz) = rot[2].sin_cos();
        self.times([[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]])
            .times([[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]])
            .times([[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]])
    }

    fn times(mut self, r: [[f32; 3]; 3]) -> Mat {
        for row in &mut self.0 {
            let a = [row[0], row[1], row[2]];
            for j in 0..3 {
                row[j] = a[0] * r[0][j] + a[1] * r[1][j] + a[2] * r[2][j];
            }
        }
        self
    }

    pub fn apply(&self, p: V3) -> V3 {
        std::array::from_fn(|i| {
            let r = &self.0[i];
            r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + r[3]
        })
    }
}

/// One `ModelRenderer` box: corners in model pixels, rotation point and rotation (radians).
struct Cube {
    min: V3,
    size: V3,
    uv: (f32, f32),
    pivot: V3,
    rot: V3,
    grow: f32,
}

impl Cube {
    fn new(min: V3, size: V3, uv: (f32, f32)) -> Cube {
        Cube {
            min,
            size,
            uv,
            pivot: [0.0; 3],
            rot: [0.0; 3],
            grow: 0.0,
        }
    }

    fn at(mut self, pivot: V3, rot: V3) -> Cube {
        self.pivot = pivot;
        self.rot = rot;
        self
    }

    fn grown(mut self, grow: f32) -> Cube {
        self.grow = grow;
        self
    }

    /// `ModelRenderer.render(0.0625)` under the matrix `m`; `tex` is the texture size in texels
    /// and `v_off` the row of the texture in a shared atlas.
    fn emit(&self, out: &mut MeshData, m: &Mat, tex: [f32; 2], v_off: f32) {
        let tf = |p: V3| {
            let r = rotate(p.map(|v| v / 16.0), self.rot);
            m.apply([
                r[0] + self.pivot[0] / 16.0,
                r[1] + self.pivot[1] / 16.0,
                r[2] + self.pivot[2] / 16.0,
            ])
        };
        let g = self.grow;
        add_box(
            out,
            &tf,
            self.min.map(|v| v - g),
            self.size.map(|v| v + 2.0 * g),
            self.size,
            self.uv,
            false,
            v_off,
            [1.0; 3],
            tex,
        );
    }
}

/// `ModelSign` under the frame of the block: board and, for a standing sign, the post; then the
/// four lines.
fn sign(
    out: &mut Batches,
    standing: bool,
    meta: i32,
    at: Mat,
    lines: Option<&[String; 4]>,
    font: Option<&Font>,
) {
    let f = SIGN_SCALE;
    let m = if standing {
        at.translate([0.5, 0.75 * f, 0.5])
            .rotate_y(-(meta * 360) as f32 / 16.0)
    } else {
        at.translate([0.5, 0.75 * f, 0.5])
            .rotate_y(-wall_turn(meta))
            .translate([0.0, -0.3125, -0.4375])
    };
    let model = m.scale([f, -f, -f]);
    let target = out.mesh(Tex::Named("textures/entity/sign.png".into()));
    let tex = [64.0, 32.0];
    Cube::new([-12.0, -14.0, -1.0], [24.0, 12.0, 2.0], (0.0, 0.0)).emit(target, &model, tex, 0.0);
    if standing {
        Cube::new([-1.0, -2.0, -1.0], [2.0, 14.0, 2.0], (0.0, 14.0)).emit(target, &model, tex, 0.0);
    }
    let (Some(lines), Some(font)) = (lines, font) else {
        return;
    };
    let f3 = 0.015_625 * f;
    let text = m.translate([0.0, 0.5 * f, 0.07 * f]).scale([f3, -f3, f3]);
    let target = out.mesh(Tex::Named(FONT_TEXTURE.into()));
    for (j, line) in lines.iter().enumerate() {
        let line = first_line(font, line, SIGN_LINE_WIDTH);
        let x = -(string_width(font, &line) / 2);
        let y = j as i32 * 10 - lines.len() as i32 * 5;
        draw_string(target, &text, font, &line, x as f32, y as f32);
    }
}

/// Yaw of a wall block with facing metadata `meta` (2 north .. 5 east), as the sign and banner renderers.
fn wall_turn(meta: i32) -> f32 {
    match meta {
        2 => 180.0,
        4 => 90.0,
        5 => -90.0,
        _ => 0.0,
    }
}

/// `TileEntitySkullRenderer.renderSkull` for the block at `at` facing `facing` (1 up, 2..5 sides).
fn skull_block(out: &mut Batches, facing: i32, skull: Skull, at: Mat, skins: &Skins) {
    let (m, turn) = match facing {
        2 => (at.translate([0.5, 0.25, 0.74]), 0.0),
        3 => (at.translate([0.5, 0.25, 0.26]), 180.0),
        4 => (at.translate([0.74, 0.25, 0.5]), 270.0),
        5 => (at.translate([0.26, 0.25, 0.5]), 90.0),
        _ => (
            at.translate([0.5, 0.0, 0.5]),
            f32::from(skull.rotation) * 360.0 / 16.0,
        ),
    };
    emit_skull(out, &m, turn, skull.kind, skull.owner, skins);
}

/// The head model of skull type `kind` under `m`, turned by `turn` degrees.
fn emit_skull(out: &mut Batches, m: &Mat, turn: f32, kind: u8, owner: Option<u128>, skins: &Skins) {
    let m = m.scale([-1.0, -1.0, 1.0]);
    let rot = [0.0, turn.to_radians(), 0.0];
    let head = Cube::new([-4.0, -8.0, -4.0], [8.0, 8.0, 8.0], (0.0, 0.0)).at([0.0; 3], rot);
    let hat = || {
        Cube::new([-4.0, -8.0, -4.0], [8.0, 8.0, 8.0], (32.0, 0.0))
            .at([0.0; 3], rot)
            .grown(0.25)
    };
    let named = |path: &str| Tex::Named(format!("textures/entity/{path}.png"));
    match kind {
        1 => head.emit(
            out.mesh(named("skeleton/wither_skeleton")),
            &m,
            [64.0, 32.0],
            0.0,
        ),
        4 => head.emit(out.mesh(named("creeper/creeper")), &m, [64.0, 32.0], 0.0),
        2 => {
            let target = out.mesh(named("zombie/zombie"));
            head.emit(target, &m, [64.0, 64.0], 0.0);
            hat().emit(target, &m, [64.0, 64.0], 0.0);
        }
        3 => {
            let skin = owner.and_then(|k| skins.player(k)).unwrap_or(Skin::Steve);
            let dims = [Skins::width() as f32, Skins::height() as f32];
            let v = skins.v_offset(skin);
            let target = out.mesh(Tex::Atlas);
            head.emit(target, &m, dims, v);
            hat().emit(target, &m, dims, v);
        }
        _ => head.emit(out.mesh(named("skeleton/skeleton")), &m, [64.0, 32.0], 0.0),
    }
}

/// `ModelBanner` under the frame of the block; the cloth sways with the world time.
fn banner_block(
    out: &mut Batches,
    standing: bool,
    meta: i32,
    pos: [i32; 3],
    view: &View,
    banner: &Banner,
    at: Mat,
) {
    let f = SIGN_SCALE;
    let m = if standing {
        at.translate([0.5, 0.75 * f, 0.5])
            .rotate_y(-(meta * 360) as f32 / 16.0)
    } else {
        at.translate([0.5, -0.25 * f, 0.5])
            .rotate_y(-wall_turn(meta))
            .translate([0.0, -0.3125, -0.4375])
    };
    let phase =
        (pos[0] * 7 + pos[1] * 9 + pos[2] * 13) as f32 + view.world_time as f32 + view.partial;
    let sway =
        (-0.0125 + 0.01 * (phase * std::f32::consts::PI * 0.02).cos()) * std::f32::consts::PI;
    let model = m.scale([f, -f, -f]);
    let target = out.mesh(Tex::Named(banner_key(banner)));
    let tex = [64.0, 64.0];
    Cube::new([-10.0, 0.0, -2.0], [20.0, 40.0, 1.0], (0.0, 0.0))
        .at([0.0, -32.0, 0.0], [sway, 0.0, 0.0])
        .emit(target, &model, tex, 0.0);
    if standing {
        Cube::new([-1.0, -30.0, -1.0], [2.0, 42.0, 2.0], (44.0, 0.0))
            .emit(target, &model, tex, 0.0);
    }
    Cube::new([-10.0, -32.0, -1.0], [20.0, 2.0, 2.0], (0.0, 42.0)).emit(target, &model, tex, 0.0);
}

/// A skull on the head of a mob or player (`LayerCustomHead`).
#[derive(Clone, Copy, Debug)]
pub struct WornHead {
    /// Feet position.
    pub pos: [f64; 3],
    /// `renderYawOffset` in degrees.
    pub body_yaw: f32,
    /// Head yaw relative to the body, and head pitch, in degrees.
    pub head_yaw: f32,
    pub pitch: f32,
    pub sneaking: bool,
    /// Model scale of the renderer: 0.9375 for players, 1 for most mobs.
    pub scale: f32,
    /// Item damage of the skull item: the skull type.
    pub kind: u8,
    pub owner: Option<u128>,
}

pub fn build_worn_heads(
    heads: &[WornHead],
    skins: &Skins,
    light: impl Fn([f64; 3]) -> [f32; 2],
) -> Vec<(Tex, MeshData)> {
    let mut out = Batches::default();
    for h in heads {
        let level = light([h.pos[0], h.pos[1] + 1.5, h.pos[2]]);
        let mark = out.mark();
        let s = h.scale;
        let mut m = Mat::at(h.pos)
            .rotate_y(180.0 - h.body_yaw)
            .scale([-1.0, -1.0, 1.0])
            .scale([s, s, s])
            .translate([0.0, -1.507_812_5, 0.0]);
        m = m
            .translate([0.0, if h.sneaking { 1.0 / 16.0 } else { 0.0 }, 0.0])
            .rotate_zyx([h.pitch.to_radians(), h.head_yaw.to_radians(), 0.0]);
        if h.sneaking {
            m = m.translate([0.0, 0.2, 0.0]);
        }
        let k = 1.1875;
        m = m.scale([k, -k, -k]);
        emit_skull(&mut out, &m, 180.0, h.kind, h.owner, skins);
        out.light_since(mark, level);
    }
    out.0
}

/// Texture key of a banner: base color and layers.
pub fn banner_key(b: &Banner) -> String {
    let layers: Vec<String> = b.layers.iter().map(|(p, c)| format!("{p}.{c}")).collect();
    format!("banner:{}:{}", b.base, layers.join(","))
}

const PATTERNS: [(&str, &str); 38] = [
    ("bs", "stripe_bottom"),
    ("ts", "stripe_top"),
    ("ls", "stripe_left"),
    ("rs", "stripe_right"),
    ("cs", "stripe_center"),
    ("ms", "stripe_middle"),
    ("drs", "stripe_downright"),
    ("dls", "stripe_downleft"),
    ("ss", "small_stripes"),
    ("cr", "cross"),
    ("sc", "straight_cross"),
    ("bt", "triangle_bottom"),
    ("tt", "triangle_top"),
    ("bts", "triangles_bottom"),
    ("tts", "triangles_top"),
    ("ld", "diagonal_left"),
    ("rd", "diagonal_right"),
    ("lud", "diagonal_up_left"),
    ("rud", "diagonal_up_right"),
    ("mc", "circle"),
    ("mr", "rhombus"),
    ("vh", "half_vertical"),
    ("hh", "half_horizontal"),
    ("vhr", "half_vertical_right"),
    ("hhb", "half_horizontal_bottom"),
    ("bl", "square_bottom_left"),
    ("br", "square_bottom_right"),
    ("tl", "square_top_left"),
    ("tr", "square_top_right"),
    ("bo", "border"),
    ("cbo", "curly_border"),
    ("bri", "bricks"),
    ("gra", "gradient"),
    ("gru", "gradient_up"),
    ("cre", "creeper"),
    ("sku", "skull"),
    ("flo", "flower"),
    ("moj", "mojang"),
];

/// `TileEntityBanner.EnumBannerPattern` code to texture file.
fn pattern_file(code: &str) -> Option<&'static str> {
    PATTERNS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|&(_, file)| file)
}

/// `MapColor` of each `EnumDyeColor`, by dye damage (0 black .. 15 white).
const DYE: [u32; 16] = [
    0x191919, 0x993333, 0x667F33, 0x664C33, 0x334CB2, 0x7F3FB2, 0x4C7F99, 0x999999, 0x4C4C4C,
    0xF27FA5, 0x7FCC19, 0xE5E533, 0x6699D8, 0xB24CD8, 0xD87F33, 0xFFFFFF,
];

/// `LayeredColorMaskTexture` of a banner: `banner_base.png`, then for each layer (the base color
/// first) the cloth of the base texture tinted with the dye, masked by the red channel of the
/// pattern texture and blended over the result. `read` loads `textures/entity/<name>.png`.
pub fn banner_image(read: &mut dyn FnMut(&str) -> Option<Image>, b: &Banner) -> Option<Image> {
    let base = read("banner_base")?;
    let mut out = Image {
        width: base.width,
        height: base.height,
        rgba: base.rgba.clone(),
    };
    let layers = std::iter::once(("base", b.base)).chain(
        b.layers
            .iter()
            .filter_map(|(code, c)| Some((pattern_file(code)?, *c))),
    );
    for (file, color) in layers.take(17) {
        let Some(mask) = read(&format!("banner/{file}")) else {
            continue;
        };
        if (mask.width, mask.height) != (base.width, base.height) {
            continue;
        }
        let dye = DYE[usize::from(color & 15)];
        let tint = [(dye >> 16) & 255, (dye >> 8) & 255, dye & 255];
        for i in 0..base.width * base.height {
            let (m, p) = (&mask.rgba[i * 4..i * 4 + 4], i * 4);
            if m[3] == 0 {
                continue;
            }
            let a = u32::from(m[0]);
            for (c, t) in tint.into_iter().enumerate() {
                let src = u32::from(base.rgba[p + c]) * t / 255;
                let dst = u32::from(out.rgba[p + c]);
                out.rgba[p + c] = ((src * a + dst * (255 - a)) / 255) as u8;
            }
            let dst_a = u32::from(out.rgba[p + 3]);
            out.rgba[p + 3] = (a + dst_a * (255 - a) / 255) as u8;
        }
    }
    Some(out)
}

const COLORS: &str = "0123456789abcdefklmnor";

/// `FontRenderer.colorCode` of the sixteen text colors.
fn color_code(i: usize) -> [f32; 3] {
    let j = (i >> 3 & 1) * 85;
    let mut r = (i >> 2 & 1) * 170 + j;
    let g = (i >> 1 & 1) * 170 + j;
    let b = (i & 1) * 170 + j;
    if i == 6 {
        r += 85;
    }
    [r, g, b].map(|v| v as f32 / 255.0)
}

/// A run of text with its style, as `FontRenderer.renderStringAtPos` walks it.
struct Glyph {
    ch: char,
    color: [f32; 3],
    bold: bool,
    italic: bool,
}

fn styled(text: &str) -> Vec<Glyph> {
    let mut out = Vec::new();
    let (mut color, mut bold, mut italic) = ([0.0; 3], false, false);
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let Some(code) = chars.next() else { break };
            match COLORS.find(code.to_ascii_lowercase()) {
                Some(i) if i < 16 => (color, bold, italic) = (color_code(i), false, false),
                Some(17) => bold = true,
                Some(20) => italic = true,
                Some(21) => (color, bold, italic) = ([0.0; 3], false, false),
                _ => {}
            }
        } else {
            out.push(Glyph {
                ch: c,
                color,
                bold,
                italic,
            });
        }
    }
    out
}

/// Width of a glyph of `ascii.png` in font pixels; glyphs outside it have none here.
fn char_width(font: &Font, c: char) -> i32 {
    if (' '..='~').contains(&c) {
        i32::from(font.ascii[c as usize])
    } else {
        0
    }
}

/// `FontRenderer.getStringWidth`.
fn string_width(font: &Font, text: &str) -> i32 {
    styled(text)
        .iter()
        .map(|g| char_width(font, g.ch) + i32::from(g.bold && char_width(font, g.ch) > 0))
        .sum()
}

/// The first line of `text` wrapped to `width` font pixels, breaking at the last space.
fn first_line(font: &Font, text: &str, width: i32) -> String {
    if string_width(font, text) <= width {
        return text.to_owned();
    }
    let (mut used, mut cut, mut last_space) = (0, 0, None);
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '§' {
            // The code after it has no width.
            chars.next();
            continue;
        }
        used += char_width(font, c);
        if c == ' ' {
            last_space = Some(i);
        }
        if used > width {
            cut = last_space.unwrap_or(i);
            break;
        }
    }
    text[..cut].to_owned()
}

/// `FontRenderer.renderDefaultChar` for each glyph of `text` with its top-left corner at (`x`,
/// `y`) font pixels in the plane of `m` (y down).
fn draw_string(out: &mut MeshData, m: &Mat, font: &Font, text: &str, mut x: f32, y: f32) {
    for g in styled(text) {
        let width = char_width(font, g.ch);
        if width == 0 {
            continue;
        }
        let c = g.ch as usize;
        let (u, v) = (((c % 16) * 8) as f32, ((c / 16) * 8) as f32);
        let w = width as f32 - 0.01;
        let skew = if g.italic { 1.0 } else { 0.0 };
        for shift in if g.bold { &[0.0, 1.0][..] } else { &[0.0][..] } {
            let x = x + shift;
            let corners = [
                ([x - skew, y + 7.99], [u / 128.0, (v + 7.99) / 128.0]),
                (
                    [x + w - 1.0 - skew, y + 7.99],
                    [(u + w - 1.0) / 128.0, (v + 7.99) / 128.0],
                ),
                ([x + w - 1.0 + skew, y], [(u + w - 1.0) / 128.0, v / 128.0]),
                ([x + skew, y], [u / 128.0, v / 128.0]),
            ];
            let base = out.vertices.len() as u32;
            for (p, uv) in corners {
                out.vertices.push(Vertex {
                    pos: m.apply([p[0], p[1], 0.0]),
                    uv,
                    color: [g.color[0], g.color[1], g.color[2], 1.0],
                    light: [1.0, 1.0],
                });
            }
            out.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        x += (width + i32::from(g.bold)) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_n(lid: &mut BlockEntities, n: usize) {
        for _ in 0..n {
            lid.tick(&[]);
        }
    }

    #[test]
    fn lid_opens_by_a_tenth_per_tick_and_closes_again() {
        let mut be = BlockEntities::default();
        let pos = [3, 64, 3];
        be.set_state(pos, CHEST << 4 | 2);
        be.block_action(pos, CHEST, 1, 1);
        tick_n(&mut be, 4);
        assert!((be.lids[&pos].now - 0.4).abs() < 1e-6);
        tick_n(&mut be, 20);
        assert_eq!(be.lids[&pos].now, 1.0);
        be.block_action(pos, CHEST, 1, 0);
        tick_n(&mut be, 3);
        assert!((be.lids[&pos].now - 0.7).abs() < 1e-6);
        tick_n(&mut be, 20);
        assert_eq!(be.lids[&pos].now, 0.0);
    }

    #[test]
    fn block_action_ignores_other_blocks_and_events() {
        let mut be = BlockEntities::default();
        be.block_action([0, 0, 0], 1, 1, 1);
        be.block_action([1, 0, 0], CHEST, 2, 1);
        assert!(be.lids.is_empty());
    }

    #[test]
    fn chunk_reload_keeps_data_of_unchanged_tiles_only() {
        let tile = |kind: u16| {
            move |x: usize, y: usize, z: usize| {
                if (x, y, z) == (1, 5, 2) {
                    kind << 4
                } else {
                    0
                }
            }
        };
        let pos = [17, 5, 2];
        let lines = || ["a".to_string(), String::new(), String::new(), String::new()];
        let mut be = BlockEntities::default();
        be.set_sign(pos, lines());
        be.load_chunk(1, 0, tile(SIGN_STANDING));
        assert_eq!(be.signs.get(&pos), Some(&lines()));
        be.load_chunk(1, 0, tile(SIGN_STANDING));
        assert_eq!(be.signs.get(&pos), Some(&lines()));
        be.set_state(pos, BANNER_STANDING << 4);
        be.set_banner(pos, Banner::default());
        be.load_chunk(1, 0, tile(BANNER_STANDING));
        assert!(be.signs.is_empty() && !be.banners.is_empty());
        be.load_chunk(1, 0, |_, _, _| 0);
        assert!(be.banners.is_empty() && be.blocks.is_empty());
    }

    #[test]
    fn replacing_a_tile_drops_its_data() {
        let mut be = BlockEntities::default();
        be.set_state([0, 0, 0], SIGN_STANDING << 4);
        be.set_sign([0, 0, 0], ["a".into(), "".into(), "".into(), "".into()]);
        be.set_state([0, 0, 0], SIGN_STANDING << 4 | 4);
        assert!(!be.signs.is_empty());
        be.set_state([0, 0, 0], 0);
        assert!(be.signs.is_empty() && be.blocks.is_empty());
    }

    #[test]
    fn color_codes_match_the_vanilla_table() {
        assert_eq!(color_code(6), [1.0, 170.0 / 255.0, 0.0]);
        assert_eq!(color_code(15), [1.0; 3]);
        assert_eq!(color_code(4), [170.0 / 255.0, 0.0, 0.0]);
    }

    #[test]
    fn banner_layers_blend_the_dyed_cloth_by_the_mask() {
        let solid = |rgba: [u8; 4]| Image {
            width: 2,
            height: 1,
            rgba: rgba.repeat(2),
        };
        let mut read = |name: &str| match name {
            "banner_base" => Some(solid([200, 200, 200, 255])),
            "banner/base" => Some(solid([255, 0, 0, 255])),
            // Mask is full on the first texel, empty on the second.
            "banner/border" => Some(Image {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 0, 0, 255],
            }),
            _ => None,
        };
        let banner = Banner {
            base: 15,
            layers: vec![("bo".into(), 1)],
        };
        let img = banner_image(&mut read, &banner).unwrap();
        // White base: unchanged cloth. First texel then takes the red dye (0x99, 0x33, 0x33).
        assert_eq!(&img.rgba[4..8], &[200, 200, 200, 255]);
        let tinted = |c: u32| u8::try_from(200 * c / 255).unwrap();
        assert_eq!(
            &img.rgba[0..4],
            &[tinted(0x99), tinted(0x33), tinted(0x33), 255]
        );
    }

    #[test]
    fn book_opens_by_a_tenth_per_tick_while_a_player_is_near() {
        let mut book = Book::new(1);
        book.update(Some((1.0, 0.0)));
        let one = book.book_spread;
        book.update(Some((1.0, 0.0)));
        assert_eq!((one, book.book_spread), (0.1, 0.2));
        assert_eq!(book.tick_count, 2);
        // The book turns toward the player: atan2(dz, dx) = 0 pulls the rotation to 0.
        assert_eq!(book.book_rotation, 0.0);
        for _ in 0..20 {
            book.update(None);
        }
        assert_eq!(book.book_spread, 0.0);
    }

    #[test]
    fn book_page_angles_at_two_ticks() {
        // ModelBook.setRotationAngles with p1 = 0 (sin 0): f = 1.25 * spread.
        let (y, x) = book_angles(0.0, 0.0, 1.0, 0.5);
        let f = 0.625_f32;
        assert_eq!(y[0], PI + f);
        assert_eq!(y[1], -f);
        assert_eq!(y[4], f); // left flipping page at rest: f - 2f * 0
        assert_eq!(y[5], -f); // right one fully turned: f - 2f * 1
        assert_eq!(x, f.sin());
        // Fifty ticks later the sway term sin(1.0) * 0.1 widens the opening.
        let (y, _) = book_angles(50.0, 0.0, 0.0, 0.5);
        let f = (1.0_f32.sin() * 0.1 + 1.25) * 0.5;
        assert_eq!(y[2], f);
        assert!(y[2] > 0.625);
    }

    #[test]
    fn flipping_page_phase_wraps_into_the_unit_range() {
        let book = Book {
            page_flip: 3.9,
            page_flip_prev: 3.9,
            ..Book::default()
        };
        let (_, l, r, _) = book_pose(&book, 0.0);
        // (4.15 - 4) * 1.6 - 0.3 < 0 clamps to 0; (4.65 - 4) * 1.6 - 0.3 = 0.74.
        assert_eq!(l, 0.0);
        assert!((r - 0.74).abs() < 1e-5);
    }
}
