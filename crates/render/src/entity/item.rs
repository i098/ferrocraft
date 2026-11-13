//! Item models in the world: the item in the right hand of a player or mob (`ItemInHandLayer`
//! with `ItemRenderer.renderItemInHand`) and dropped items (`ItemEntityRenderer`).
//!
//! A model is the extruded sprite of a `builtin/generated` item or the block cube, centred on the
//! origin with its `display` transforms from the item model json. Every piece is placed on the CPU
//! and sorted by texture: block faces sample the block atlas, sprites their own texture.
//!
//! shortcut: bow pull stages, the cast fishing rod and baby holders are not ported; the enchantment
//! glint covers the whole piece.

use super::{
    glint_uv_item, light_level, model, Class, EntityDraw, Xf, ITEM_GLINT_COLOR, ITEM_UV_SCALE, V3,
};
use crate::assets::{decode_png, Atlas};
use crate::hand::{self, chain, ident, mul, rotate, scale, scale3, translate, Mat};
use crate::jrandom::JavaRandom;
use crate::mesh::{MeshData, Vertex};
use std::collections::{BTreeMap, HashMap};

/// What a dropped item or a held item shows: id, damage, stack size and the enchantment glint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: i16,
    pub damage: i16,
    pub count: u8,
    pub glint: bool,
}

/// Which texture the vertices of a model sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemTex {
    /// The block atlas.
    Atlas,
    /// A texture path below `assets/minecraft/`.
    Sprite(String),
}

/// A resolved item model.
pub struct ItemModel {
    tex: ItemTex,
    /// Centred on the origin and `0..1` wide, before `RenderItem.renderItem` scales it by half.
    mesh: MeshData,
    /// One normal per quad of a sprite; block meshes carry their shade in the vertex color.
    normals: Option<Vec<V3>>,
    /// `BakedModel.isGui3d`: blocks, not sprites.
    gui3d: bool,
    third: Mat,
    ground: Mat,
    fixed: Mat,
    ground_scale: V3,
}

impl ItemModel {
    pub(super) fn gui3d(&self) -> bool {
        self.gui3d
    }

    pub(super) fn fixed(&self) -> Mat {
        self.fixed
    }
}

/// Models by `(id, damage)`; `None` when the item has none to draw.
pub type Models = HashMap<(i16, i16), Option<ItemModel>>;

/// `ItemModelMesher` for one stack: the extruded sprite of a flat model, else the block of an id
/// below 256. `read` takes paths below `assets/minecraft/`.
pub fn resolve(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    atlas: &Atlas,
    id: i16,
    damage: i16,
) -> Option<ItemModel> {
    let name = world::inventory::items::model(id, damage);
    let (tex, mesh, normals) = geometry(read, atlas, name, id, damage)?;
    let mut display = |key| match name {
        Some(name) => hand::display(read, name, key),
        None => (ident(), [1.0; 3]),
    };
    let (third, _) = display("thirdperson");
    let (ground, ground_scale) = display("ground");
    let (fixed, _) = display("fixed");
    Some(ItemModel {
        tex,
        mesh,
        gui3d: normals.is_none(),
        normals,
        third,
        ground,
        fixed,
        ground_scale,
    })
}

/// Sprite or block geometry of a stack, with the texture it samples.
fn geometry(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    atlas: &Atlas,
    name: Option<&str>,
    id: i16,
    damage: i16,
) -> Option<(ItemTex, MeshData, Option<Vec<V3>>)> {
    let flat = name.and_then(|name| {
        let (texture, _) = hand::flat_model(read, name)?;
        let image = decode_png(&read(&texture)?)?;
        Some((texture, hand::Flat::new(&image, ident())))
    });
    if let Some((texture, flat)) = flat {
        let mut mesh = flat.mesh;
        // `RenderItem.renderItem` translates the sprite by -0.5 after scaling it.
        for v in &mut mesh.vertices {
            v.pos = v.pos.map(|c| c - 0.5);
        }
        return Some((ItemTex::Sprite(texture), mesh, Some(flat.normals)));
    }
    if !(1..256).contains(&id) {
        return None;
    }
    let state = ((id as u16) << 4) | (damage as u16 & 15);
    Some((ItemTex::Atlas, hand::held_mesh(state, atlas), None))
}

/// The stacks `build` needs models for.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let held = (holds(d) && d.held != 0).then_some((d.held, d.held_damage));
    held.into_iter().chain(dropped(d).map(|s| (s.id, s.damage)))
}

fn holds(d: &EntityDraw) -> bool {
    matches!(d.class, Class::Player { .. } | Class::Mob(54 | 57 | 51))
}

fn dropped(d: &EntityDraw) -> Option<Stack> {
    d.stack.filter(|_| d.class == Class::Object(2))
}

/// World-space meshes of the held and dropped items, split by texture, and their glint layer.
#[derive(Default)]
pub struct Items {
    pub atlas: MeshData,
    pub sprites: BTreeMap<String, MeshData>,
    /// Two passes per glowing piece, over the glint texture; same positions as the pieces.
    pub glint: MeshData,
}

/// `p` through `m`, a column-major affine matrix.
pub(super) fn point(m: &Mat, p: V3) -> V3 {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

/// The direction `n` through the linear part of `m`.
pub(super) fn dir(m: &Mat, n: V3) -> V3 {
    std::array::from_fn(|r| m[0][r] * n[0] + m[1][r] * n[1] + m[2][r] * n[2])
}

/// Face shade of a world-space normal: lit from above like the entity boxes (top 1, side 0.8,
/// bottom 0.5).
pub(super) fn shade(n: V3) -> f32 {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    let y = n[1] / len;
    if y >= 0.0 {
        0.8 + 0.2 * y
    } else {
        0.8 + 0.3 * y
    }
}

impl Items {
    fn mesh(&mut self, tex: &ItemTex) -> &mut MeshData {
        match tex {
            ItemTex::Atlas => &mut self.atlas,
            ItemTex::Sprite(path) => self.sprites.entry(path.clone()).or_default(),
        }
    }

    /// One textured quad of a hanging entity, corners counter-clockwise from the front.
    pub(super) fn quad(
        &mut self,
        tex: &str,
        pos: [V3; 4],
        uv: [[f32; 2]; 4],
        shade: f32,
        light: [f32; 2],
    ) {
        let target = self.sprites.entry(tex.to_owned()).or_default();
        let base = target.vertices.len() as u32;
        for i in 0..4 {
            target.vertices.push(Vertex {
                pos: pos[i],
                uv: uv[i],
                color: [shade, shade, shade, 1.0],
                light,
            });
        }
        target.indices.extend([0, 1, 2, 0, 2, 3].map(|i| i + base));
    }

    /// Places `model` with `to_world` (model units to world coordinates) and `normal_to_world`.
    pub(super) fn piece(
        &mut self,
        model: &ItemModel,
        to_world: &dyn Fn(V3) -> V3,
        normal_to_world: &dyn Fn(V3) -> V3,
        level: [f32; 2],
        direct: bool,
        glint: Option<f32>,
    ) {
        let mut vertices = model.mesh.vertices.clone();
        // The front quad of a sprite (the first) winds clockwise, for the hand's model space; placed
        // straight into the world it needs the reverse order to face outward.
        if direct && model.normals.is_some() {
            vertices[..4].reverse();
        }
        for v in &mut vertices {
            v.pos = to_world(v.pos);
            v.light = level;
        }
        if let Some(normals) = &model.normals {
            for (quad, n) in vertices.chunks_mut(4).zip(normals) {
                let s = shade(normal_to_world(*n));
                for v in quad {
                    v.color = [s, s, s, 1.0];
                }
            }
        }
        if let Some(ms) = glint {
            let scale = match model.tex {
                ItemTex::Atlas => 1.0,
                ItemTex::Sprite(_) => ITEM_UV_SCALE,
            };
            for pass in 0..2 {
                let base = self.glint.vertices.len() as u32;
                self.glint.vertices.extend(vertices.iter().map(|v| Vertex {
                    uv: glint_uv_item(v.uv.map(|c| c * scale), ms, pass),
                    color: ITEM_GLINT_COLOR,
                    light: [1.0; 2],
                    ..*v
                }));
                self.glint
                    .indices
                    .extend(model.mesh.indices.iter().map(|i| i + base));
            }
        }
        let target = self.mesh(&model.tex);
        let base = target.vertices.len() as u32;
        target.vertices.extend(vertices);
        target
            .indices
            .extend(model.mesh.indices.iter().map(|i| i + base));
    }
}

/// Item meshes of every draw. `light` gives the `[sky, block]` level at a world position.
pub fn build(draws: &[EntityDraw], models: &Models, light: impl Fn([f64; 3]) -> [f32; 2]) -> Items {
    let mut out = Items::default();
    for d in draws {
        let level = light_level(d, &light, false);
        if holds(d) && d.held != 0 {
            if let Some(Some(m)) = models.get(&(d.held, d.held_damage)) {
                held(&mut out, d, m, level);
            }
        }
        if let Some(stack) = dropped(d) {
            if let Some(Some(m)) = models.get(&(stack.id, stack.damage)) {
                draw_drop(&mut out, d, stack, m, level);
            }
        }
    }
    out
}

/// Torches, `BlockTorch.getRenderType() == 2`: `ItemInHandLayer` tilts them as held blocks.
fn is_torch(id: i16) -> bool {
    matches!(id, 50 | 75 | 76)
}

/// `ItemInHandLayer.render`: from the right arm's pivot and rotation (`translateRightArm`), then
/// the item's `thirdperson` transform. Model units are the entity model's pixels over 16.
fn held(out: &mut Items, d: &EntityDraw, m: &ItemModel, level: [f32; 2]) {
    let parts = model(d).parts;
    let Some(arm) = parts.get(3) else { return };
    let xf = Xf::new(d, false);
    let mut mat = chain(&[
        translate(
            arm.pivot[0] / 16.0,
            arm.pivot[1] / 16.0,
            arm.pivot[2] / 16.0,
        ),
        rotate(arm.rot[2].to_degrees(), 2),
        rotate(arm.rot[1].to_degrees(), 1),
        rotate(arm.rot[0].to_degrees(), 0),
        translate(-0.0625, 0.4375, 0.0625),
    ]);
    if is_torch(d.held) {
        mat = chain(&[
            mat,
            translate(0.0, 0.1875, -0.3125),
            rotate(20.0, 0),
            rotate(45.0, 1),
            scale3([-0.375, -0.375, 0.375]),
        ]);
    }
    if d.sneaking {
        mat = mul(mat, translate(0.0, 0.203_125, 0.0));
    }
    // `renderItemInHand` scales flat models by 2 before the display transform.
    let prepare = if m.gui3d { ident() } else { scale(2.0) };
    let mat = chain(&[mat, prepare, m.third, scale(0.5)]);
    // Model space is the biped's pixel frame; `Xf` maps it to the world.
    let to_world = |p: V3| xf.world(point(&mat, p).map(|c| c * 16.0));
    // Model y points down: the world y of a direction is its negation.
    let normal = |n: V3| {
        let [x, y, z] = dir(&mat, n);
        [-x, -y, z]
    };
    let glint = d.held_glint.then_some(d.age * 50.0);
    out.piece(m, &to_world, &normal, level, false, glint);
}

/// `ItemEntityRenderer.render`: bob, spin and one to five copies by stack size.
fn draw_drop(out: &mut Items, d: &EntityDraw, stack: Stack, m: &ItemModel, level: [f32; 2]) {
    let copies = copy_count(stack.count);
    let bob = ((d.age / 10.0 + d.hover_start).sin()) * 0.1 + 0.1;
    let spin = (d.age / 20.0 + d.hover_start).to_degrees();
    let mut base = chain(&[
        translate(0.0, bob + 0.25 * m.ground_scale[1], 0.0),
        rotate(spin, 1),
    ]);
    if !m.gui3d {
        base = mul(
            base,
            translate(0.0, 0.0, -0.046_875 * (copies - 1) as f32 * 0.5),
        );
    }
    let mut random = JavaRandom::new(187);
    let glint = stack.glint.then_some(d.age * 50.0);
    for i in 0..copies {
        let mat = copy_matrix(m, &mut base, &mut random, i);
        let to_world = |p: V3| {
            let [x, y, z] = point(&mat, p);
            [
                (d.pos[0] + f64::from(x)) as f32,
                (d.pos[1] + f64::from(y)) as f32,
                (d.pos[2] + f64::from(z)) as f32,
            ]
        };
        out.piece(m, &to_world, &|n| dir(&mat, n), level, true, glint);
    }
}

/// `ItemEntityRenderer.getModelCount`.
fn copy_count(count: u8) -> usize {
    match count {
        49.. => 5,
        33..=48 => 4,
        17..=32 => 3,
        2..=16 => 2,
        _ => 1,
    }
}

/// Matrix of copy `i`: blocks jitter at random, sprites stack along z.
fn copy_matrix(m: &ItemModel, base: &mut Mat, random: &mut JavaRandom, i: usize) -> Mat {
    if m.gui3d {
        let offset = if i > 0 {
            let mut r = || (random.next_float() * 2.0 - 1.0) * 0.15;
            translate(r(), r(), r())
        } else {
            ident()
        };
        return chain(&[*base, offset, scale(0.5), m.ground, scale(0.5)]);
    }
    let placed = chain(&[*base, m.ground, scale(0.5)]);
    *base = mul(*base, translate(0.0, 0.0, 0.046_875 * m.ground_scale[2]));
    placed
}
