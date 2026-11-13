//! Item models in the world: the item in the right hand of a player or mob (`ItemInHandLayer`
//! with `ItemRenderer.renderItemInHand`) and dropped items (`ItemEntityRenderer`).
//!
//! A model is the extruded sprite of a `builtin/generated` item or the block cube, centred on the
//! origin with its `display` transforms from the item model json. Every piece is placed on the CPU
//! and sorted by texture: block faces sample the block atlas, sprites their own texture.
//!

use super::{
    glint_uv_item, light_level, model, Class, EntityDraw, Xf, ITEM_GLINT_COLOR, ITEM_UV_SCALE, V3,
};
use crate::assets::{decode_png, Atlas};
use crate::hand::{self, chain, ident, mul, rotate, scale, scale3, translate, Mat};
use crate::jrandom::JavaRandom;
use crate::mesh::{MeshData, Vertex};
use std::collections::{BTreeMap, HashMap};

/// What a dropped item or a held item shows: id, damage, stack size, the enchantment glint and
/// the `display.color` of leather armor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: i16,
    pub damage: i16,
    pub count: u8,
    pub glint: bool,
    pub dye: Option<i32>,
}

/// Which texture the vertices of a model sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemTex {
    /// The block atlas.
    Atlas,
    /// A texture path below `assets/minecraft/`.
    Sprite(String),
}

/// One texture layer of a model: `layerN` of a flat item, or the whole block.
struct Layer {
    tex: ItemTex,
    /// Centred on the origin and `0..1` wide, before `RenderItem.renderItem` scales it by half.
    mesh: MeshData,
    /// One normal per quad of a sprite; block meshes carry their shade in the vertex color.
    normals: Option<Vec<V3>>,
}

/// A resolved item model.
pub struct ItemModel {
    id: i16,
    damage: i16,
    /// Drawn in order; layer `i` takes tint index `i`.
    layers: Vec<Layer>,
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
    let layers = geometry(read, atlas, name, id, damage)?;
    let mut display = |key| match name {
        Some(name) => hand::display(read, name, key),
        None => (ident(), [1.0; 3]),
    };
    let (third, _) = display("thirdperson");
    let (ground, ground_scale) = display("ground");
    let (fixed, _) = display("fixed");
    Some(ItemModel {
        id,
        damage,
        gui3d: layers[0].normals.is_none(),
        layers,
        third,
        ground,
        fixed,
        ground_scale,
    })
}

/// Sprite layers or block geometry of a stack, with the texture each samples.
fn geometry(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    atlas: &Atlas,
    name: Option<&str>,
    id: i16,
    damage: i16,
) -> Option<Vec<Layer>> {
    let sprites = name.and_then(|name| {
        let (textures, _) = hand::flat_model(read, name)?;
        let layers: Vec<Layer> = textures
            .into_iter()
            .filter_map(|texture| {
                let image = decode_png(&read(&texture)?)?;
                let flat = hand::Flat::new(&image, ident());
                let mut mesh = flat.mesh;
                // `RenderItem.renderItem` translates the sprite by -0.5 after scaling it.
                for v in &mut mesh.vertices {
                    v.pos = v.pos.map(|c| c - 0.5);
                }
                Some(Layer {
                    tex: ItemTex::Sprite(texture),
                    mesh,
                    normals: Some(flat.normals),
                })
            })
            .collect();
        (!layers.is_empty()).then_some(layers)
    });
    if sprites.is_some() {
        return sprites;
    }
    if !(1..256).contains(&id) {
        return None;
    }
    let state = ((id as u16) << 4) | (damage as u16 & 15);
    Some(vec![Layer {
        tex: ItemTex::Atlas,
        mesh: hand::held_mesh(state, atlas),
        normals: None,
    }])
}

/// The stacks `build` needs models for.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let held = (holds(d) && d.held != 0).then_some((d.held, d.held_damage));
    held.into_iter()
        .chain(worn_block(d))
        .chain(dropped(d).map(|s| (s.id, s.damage)))
        .chain(super::block::wanted(d))
}

fn holds(d: &EntityDraw) -> bool {
    matches!(
        d.class,
        Class::Player { .. } | Class::Mob(54 | 57 | 51) | Class::Object(78)
    )
}

/// `WornSkullLayer`: a block worn as a helmet by an armor stand, as its id and damage.
fn worn_block(d: &EntityDraw) -> Option<(i16, i16)> {
    let id = d.armor[3];
    (d.class == Class::Object(78) && (1..256).contains(&id))
        .then_some((id, d.look.stand.head_damage))
}

fn dropped(d: &EntityDraw) -> Option<Stack> {
    d.stack.filter(|_| d.class == Class::Object(2))
}

/// World-space meshes of the held and dropped items, split by texture, and their glint layer.
#[derive(Default)]
pub struct Items {
    pub atlas: MeshData,
    /// Sprite meshes by `(layer, texture)`: drawn layer by layer, so an overlay lands over the
    /// coplanar layer below it as `LessEqual` depth tests need.
    pub sprites: BTreeMap<(u8, String), MeshData>,
    /// Two passes per glowing piece, over the glint texture; same positions as the pieces.
    pub glint: MeshData,
    /// The white flash of primed TNT, drawn additively over the white texel.
    pub flash: MeshData,
    /// `display.color` of the leather armor the next sprite pieces show.
    dye: Option<i32>,
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
    fn mesh(&mut self, tex: &ItemTex, layer: usize) -> &mut MeshData {
        match tex {
            ItemTex::Atlas => &mut self.atlas,
            ItemTex::Sprite(path) => self.sprites.entry((layer as u8, path.clone())).or_default(),
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
        let target = self.sprites.entry((0, tex.to_owned())).or_default();
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

    /// A white copy of `model` with `alpha`, placed with `to_world`.
    pub(super) fn flash(&mut self, model: &ItemModel, to_world: &dyn Fn(V3) -> V3, alpha: f32) {
        let mesh = &model.layers[0].mesh;
        let base = self.flash.vertices.len() as u32;
        self.flash
            .vertices
            .extend(mesh.vertices.iter().map(|v| Vertex {
                pos: to_world(v.pos),
                uv: crate::sky::white_uv(),
                color: [1.0, 1.0, 1.0, alpha],
                light: [1.0; 2],
            }));
        self.flash
            .indices
            .extend(mesh.indices.iter().map(|i| i + base));
    }

    /// Places each layer of `model` with `to_world` (model units to world coordinates) and
    /// `normal_to_world`, sprite layer `i` tinted by `ItemColors` index `i`.
    pub(super) fn piece(
        &mut self,
        model: &ItemModel,
        to_world: &dyn Fn(V3) -> V3,
        normal_to_world: &dyn Fn(V3) -> V3,
        level: [f32; 2],
        direct: bool,
        glint: Option<f32>,
    ) {
        for (i, layer) in model.layers.iter().enumerate() {
            let mut vertices = layer.mesh.vertices.clone();
            // The front quad of a sprite (the first) winds clockwise, for the hand's model space;
            // placed straight into the world it needs the reverse order to face outward.
            if direct && layer.normals.is_some() {
                vertices[..4].reverse();
            }
            for v in &mut vertices {
                v.pos = to_world(v.pos);
                v.light = level;
            }
            if let Some(normals) = &layer.normals {
                let rgb = world::inventory::items::tint(model.id, model.damage, self.dye, i);
                let tint = [16, 8, 0].map(|s| ((rgb >> s) & 0xFF) as f32 / 255.0);
                for (quad, n) in vertices.chunks_mut(4).zip(normals) {
                    let s = shade(normal_to_world(*n));
                    for v in quad {
                        v.color = [s * tint[0], s * tint[1], s * tint[2], 1.0];
                    }
                }
            }
            if let Some(ms) = glint {
                self.add_glint(&layer.tex, &vertices, &layer.mesh.indices, ms);
            }
            let target = self.mesh(&layer.tex, i);
            let base = target.vertices.len() as u32;
            target.vertices.extend(vertices);
            target
                .indices
                .extend(layer.mesh.indices.iter().map(|i| i + base));
        }
    }

    /// The two glint passes over a placed layer.
    fn add_glint(&mut self, tex: &ItemTex, vertices: &[Vertex], indices: &[u32], ms: f32) {
        let scale = match tex {
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
            self.glint.indices.extend(indices.iter().map(|i| i + base));
        }
    }
}

/// Item meshes of every draw. `light` gives the `[sky, block]` level at a world position.
pub fn build(draws: &[EntityDraw], models: &Models, light: impl Fn([f64; 3]) -> [f32; 2]) -> Items {
    let mut out = Items::default();
    for d in draws {
        let level = light_level(d, &light, false);
        if holds(d) && d.held != 0 {
            if let Some(Some(m)) = models.get(&(d.held, d.held_damage)) {
                out.dye = d.held_dye;
                held(&mut out, d, m, level);
            }
        }
        if let Some(key) = worn_block(d) {
            if let Some(Some(m)) = models.get(&key) {
                head(&mut out, d, m, level);
            }
        }
        if let Some(stack) = dropped(d) {
            if let Some(Some(m)) = models.get(&(stack.id, stack.damage)) {
                out.dye = stack.dye;
                draw_drop(&mut out, d, stack, m, level);
            }
        }
    }
    out.dye = None;
    super::block::build(&mut out, draws, models, &light);
    out
}

/// `WornSkullLayer` for a block helmet: from the head's pivot and rotation (`ModelPart.transform`),
/// down a quarter block, turned around and scaled by 0.625; `ItemInHandRenderer.render` doubles a
/// block model and `RenderItem.renderItem` halves it. The `head` display transform of a block
/// model is the identity.
fn head(out: &mut Items, d: &EntityDraw, m: &ItemModel, level: [f32; 2]) {
    let parts = model(d).parts;
    let Some(head) = parts.first() else { return };
    let xf = Xf::new(d, false);
    let stance = if d.child {
        chain(&[scale(0.7), translate(0.0, 1.0, 0.0)])
    } else if d.sneaking {
        translate(0.0, 0.2, 0.0)
    } else {
        ident()
    };
    let prepare = if m.gui3d { scale(2.0) } else { ident() };
    let mat = chain(&[
        stance,
        translate(
            head.pivot[0] / 16.0,
            head.pivot[1] / 16.0,
            head.pivot[2] / 16.0,
        ),
        rotate(head.rot[2].to_degrees(), 2),
        rotate(head.rot[1].to_degrees(), 1),
        rotate(head.rot[0].to_degrees(), 0),
        translate(0.0, -0.25, 0.0),
        rotate(180.0, 1),
        scale3([0.625, -0.625, -0.625]),
        prepare,
        scale(0.5),
    ]);
    let to_world = |p: V3| xf.world(point(&mat, p).map(|c| c * 16.0));
    let normal = |n: V3| {
        let [x, y, z] = dir(&mat, n);
        [-x, -y, z]
    };
    out.piece(m, &to_world, &normal, level, false, None);
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
    // A small holder first shrinks and tilts the item (`ItemInHandLayer`, `isBaby`).
    let child = if d.child && d.class == Class::Object(78) {
        chain(&[translate(0.0, 0.625, 0.0), rotate(20.0, 0), scale(0.5)])
    } else {
        ident()
    };
    let mut mat = chain(&[
        child,
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
