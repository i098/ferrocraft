//! Item models in the world: the item in the right hand of a player or mob (`ItemInHandLayer`
//! with `ItemRenderer.renderItemInHand`) and dropped items (`ItemEntityRenderer`).
//!
//! A model is the extruded sprite of a `builtin/generated` item or the block cube, centred on the
//! origin with its `display` transforms from the item model json. Every piece is placed on the CPU
//! and sorted by texture: block faces sample the block atlas, sprites their own texture.
//!

use super::glint_mask::{self, Mask};
use super::{
    glint_uv_item, light_level, model, Class, EntityDraw, Xf, ITEM_GLINT_COLOR, ITEM_UV_SCALE, V3,
};
use crate::assets::{decode_png, Atlas};
use crate::hand::{self, chain, ident, mul, rotate, scale, scale3, translate, DialTurns, Mat};
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
    /// Block meshes only: each vertex color without face shade and AO, for the GUI.
    flat: Vec<[f32; 4]>,
    /// The opaque texels of a sprite, for the GUI glint.
    mask: Option<Mask>,
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
    /// `(frames, frame)` of a compass or clock strip.
    dial: Option<(usize, usize)>,
}

impl ItemModel {
    /// True when the model shows another strip frame than `turns` ask for.
    pub fn stale(&self, turns: &DialTurns) -> bool {
        self.dial
            .is_some_and(|(frames, frame)| turns.frame(self.id, frames) != frame)
    }

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
    turns: &DialTurns,
) -> Option<ItemModel> {
    let name = world::inventory::items::model(id, damage);
    let mut dial = None;
    let layers = geometry(read, atlas, name, id, damage, turns, &mut dial)?;
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
        dial,
    })
}

/// Sprite layers or block geometry of a stack, with the texture each samples.
fn geometry(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    atlas: &Atlas,
    name: Option<&str>,
    id: i16,
    damage: i16,
    turns: &DialTurns,
    dial: &mut Option<(usize, usize)>,
) -> Option<Vec<Layer>> {
    let sprites = name.filter(|_| damage >= 0).and_then(|name| {
        let (textures, _) = hand::flat_model(read, name)?;
        let layers: Vec<Layer> = textures
            .into_iter()
            .filter_map(|texture| {
                let image = decode_png(&read(&texture)?)?;
                let frames = hand::strip_frames(&image);
                let frame = turns.frame(id, frames);
                if frames > 1 && DialTurns::is_dial(id) {
                    *dial = Some((frames, frame));
                }
                let flat = hand::Flat::new(&image, ident(), frame);
                let mut mesh = flat.mesh;
                // `RenderItem.renderItem` translates the sprite by -0.5 after scaling it.
                for v in &mut mesh.vertices {
                    v.pos = v.pos.map(|c| c - 0.5);
                }
                Some(Layer {
                    tex: ItemTex::Sprite(texture),
                    mesh,
                    normals: Some(flat.normals),
                    flat: Vec::new(),
                    mask: Some(Mask::of(&image)),
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
    let meta = if damage < 0 {
        super::mooshroom::BLOCK_FORM - damage
    } else {
        damage
    };
    let state = ((id as u16) << 4) | (meta as u16 & 15);
    Some(vec![Layer {
        tex: ItemTex::Atlas,
        mesh: hand::held_mesh(state, atlas, false),
        normals: (damage < 0).then(|| super::mooshroom::FACES.to_vec()),
        flat: hand::held_mesh(state, atlas, true)
            .vertices
            .iter()
            .map(|v| v.color)
            .collect(),
        mask: None,
    }])
}

/// The stacks `build` needs models for.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let held = (holds(d) && d.held != 0).then_some((d.held, d.held_damage));
    held.into_iter()
        .chain(worn_block(d))
        .chain(dropped(d).map(|s| (s.id, s.damage)))
        .chain(super::block::wanted(d))
        .chain(super::mooshroom::wanted(d))
}

fn holds(d: &EntityDraw) -> bool {
    matches!(
        d.class,
        Class::Player { .. }
            | Class::Mob(53 | 54 | 57 | 51 | super::witch::WITCH)
            | Class::Object(78)
    )
}

/// `WornSkullLayer` for a block helmet (see [`super::helmet`]), as its id and damage; or the
/// pumpkin of `LayerSnowmanHead`.
fn worn_block(d: &EntityDraw) -> Option<(i16, i16)> {
    super::snowman::wanted(d).or(super::helmet::block(d))
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
    /// Two passes per glowing block piece, over the glint texture; same positions as the pieces.
    pub glint: MeshData,
    /// The same for sprite pieces. Vanilla masks it to the opaque texels with an equal depth test;
    /// the GUI build cuts it to those texels (see `glint_mask`).
    pub glint_flat: MeshData,
    /// The white flash of primed TNT, drawn additively over the white texel.
    pub flash: MeshData,
    /// `display.color` of the leather armor the next sprite pieces show.
    dye: Option<i32>,
    /// The pieces take `LayerMooshroomMushroom`'s GL light instead of the box face shade.
    pub(super) gl_lit: bool,
}

pub(super) use crate::hand::point;

/// The direction `n` through the linear part of `m`.
pub(super) fn dir(m: &Mat, n: V3) -> V3 {
    std::array::from_fn(|r| m[0][r] * n[0] + m[1][r] * n[1] + m[2][r] * n[2])
}

/// Face shade of a world-space normal: lit from above like the entity boxes (top 1, side 0.8,
/// bottom 0.5).
pub(super) fn shade(n: V3) -> f32 {
    if super::UNSHADED.get() {
        return 1.0;
    }
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
        glint: Option<f32>,
    ) {
        for (i, layer) in model.layers.iter().enumerate() {
            let mut vertices = layer.mesh.vertices.clone();
            for (j, v) in vertices.iter_mut().enumerate() {
                v.pos = to_world(v.pos);
                v.light = level;
                if super::UNSHADED.get() && layer.normals.is_none() {
                    v.color = layer.flat[j];
                }
            }
            if let Some(normals) = &layer.normals {
                let rgb = world::inventory::items::tint(model.id, model.damage, self.dye, i);
                let tint = [16, 8, 0].map(|s| ((rgb >> s) & 0xFF) as f32 / 255.0);
                for (quad, n) in vertices.chunks_mut(4).zip(normals) {
                    let n = normal_to_world(*n);
                    let s = if self.gl_lit && !super::UNSHADED.get() {
                        super::mooshroom::gl_light(n)
                    } else {
                        shade(n)
                    };
                    for v in quad {
                        v.color = [s * tint[0], s * tint[1], s * tint[2], 1.0];
                    }
                }
            }
            if let Some(ms) = glint {
                self.add_glint(layer, &vertices, ms);
            }
            let target = self.mesh(&layer.tex, i);
            let base = target.vertices.len() as u32;
            target.vertices.extend(vertices);
            target
                .indices
                .extend(layer.mesh.indices.iter().map(|i| i + base));
        }
    }

    /// The two glint passes over a placed layer. The GUI has no equal depth test, so it gets the
    /// sprite glint cut to the opaque texels.
    fn add_glint(&mut self, layer: &Layer, vertices: &[Vertex], ms: f32) {
        let (scale, glint) = match layer.tex {
            ItemTex::Atlas => (1.0, &mut self.glint),
            ItemTex::Sprite(_) => (ITEM_UV_SCALE, &mut self.glint_flat),
        };
        let mut placed = MeshData {
            vertices: vertices.to_vec(),
            indices: layer.mesh.indices.clone(),
        };
        for v in &mut placed.vertices {
            (v.color, v.light) = (ITEM_GLINT_COLOR, [1.0; 2]);
        }
        let mask = layer.mask.as_ref().filter(|_| super::UNSHADED.get());
        for pass in 0..2 {
            let map = |uv: [f32; 2]| glint_uv_item(uv.map(|c| c * scale), ms, pass);
            let mut mesh = match mask {
                Some(mask) => glint_mask::split(&placed, mask, &map),
                None => glint_mask::remap(placed.clone(), &map),
            };
            let base = glint.vertices.len() as u32;
            glint.vertices.append(&mut mesh.vertices);
            glint.indices.extend(mesh.indices.iter().map(|i| i + base));
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
    for d in draws {
        mushrooms(&mut out, d, models, light_level(d, &light, false));
    }
    out
}

/// `LayerMooshroomMushroom`: the red mushroom block at each frame of the layer.
fn mushrooms(out: &mut Items, d: &EntityDraw, models: &Models, level: [f32; 2]) {
    let Some(Some(m)) = models.get(&super::mooshroom::RED_MUSHROOM) else {
        return;
    };
    let xf = Xf::new(d, false);
    out.gl_lit = true;
    for mat in super::mooshroom::frames(d) {
        let to_world = |p: V3| xf.world(point(&mat, p).map(|c| c * 16.0));
        let origin = to_world([0.0; 3]);
        // The face normal turned like the block: through the frame, the body and its yaw.
        let normal = |n: V3| {
            let t = to_world(n);
            [t[0] - origin[0], t[1] - origin[1], t[2] - origin[2]]
        };
        out.piece(m, &to_world, &normal, level, None);
    }
    out.gl_lit = false;
}

/// `WornSkullLayer` for a block helmet (and `LayerSnowmanHead`, which drops it 0.34375 instead): from the head's pivot and rotation (`ModelPart.transform`),
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
        translate(0.0, super::snowman::head_drop(d), 0.0),
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
    out.piece(m, &to_world, &normal, level, None);
}

/// `LayerHeldItemWitch`: the block pose is for chests (`isRenderTypeChest`): chest, ender chest, trapped chest.
fn is_chest(id: i16) -> bool {
    matches!(id, 54 | 130 | 146)
}

/// Blocks of `getRenderType() == 2`, the torches and the chests: `ItemInHandLayer` tilts them as held blocks.
fn is_tilted_block(id: i16) -> bool {
    matches!(id, 50 | 75 | 76) || is_chest(id)
}

/// The chest a player or mob holds: its block id and the frame `RenderItem.renderItem` leaves
/// before `TileEntityItemStackRenderer` draws it: the hand, `scale(2)` of a 3D model, `scale(0.5)`
/// and `translate(-0.5)`. The frame maps block units to the world, relative to `d.pos`.
pub(crate) fn held_chest(d: &EntityDraw) -> Option<(u16, Mat)> {
    if !(holds(d) && is_chest(d.held)) {
        return None;
    }
    let mat = chain(&[
        hand_matrix(d)?,
        scale(2.0),
        scale(0.5),
        translate(-0.5, -0.5, -0.5),
    ]);
    let mut xf = Xf::new(d, false);
    xf.pos = [0.0; 3];
    let at = |p: V3| xf.world(point(&mat, p).map(|c| c * 16.0));
    let origin = at([0.0; 3]);
    let axis = |e: V3| {
        let a = at(e);
        [a[0] - origin[0], a[1] - origin[1], a[2] - origin[2], 0.0]
    };
    let frame = [
        axis([1.0, 0.0, 0.0]),
        axis([0.0, 1.0, 0.0]),
        axis([0.0, 0.0, 1.0]),
        [origin[0], origin[1], origin[2], 1.0],
    ];
    Some((d.held as u16, frame))
}

/// Where the hand puts the item, before the display transform: the witch's nose layer, else the
/// right arm's pivot and rotation (`translateRightArm`).
fn hand_matrix(d: &EntityDraw) -> Option<Mat> {
    if d.class == Class::Mob(super::witch::WITCH) {
        return Some(super::witch::hand(d, is_chest(d.held)));
    }
    let parts = model(d).parts;
    let arm = parts.get(3)?;
    // A small holder first shrinks and tilts the item (`LayerHeldItem`, `isChild`).
    let child = if d.child {
        chain(&[translate(0.0, 0.625, 0.0), rotate(20.0, 0), scale(0.5)])
    } else {
        ident()
    };
    // `ModelPlayer.postRenderArm` moves the pivot of a small arm 1 px in.
    let slim = matches!(d.class, Class::Player { slim: true });
    let mut mat = chain(&[
        child,
        translate(
            (arm.pivot[0] + f32::from(u8::from(slim))) / 16.0,
            arm.pivot[1] / 16.0,
            arm.pivot[2] / 16.0,
        ),
        rotate(arm.rot[2].to_degrees(), 2),
        rotate(arm.rot[1].to_degrees(), 1),
        rotate(arm.rot[0].to_degrees(), 0),
        translate(-0.0625, 0.4375, 0.0625),
    ]);
    if is_tilted_block(d.held) {
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
    Some(mat)
}

/// `ItemInHandLayer.render`: from the right arm's pivot and rotation (`translateRightArm`), then
/// the item's `thirdperson` transform. Model units are the entity model's pixels over 16.
fn held(out: &mut Items, d: &EntityDraw, m: &ItemModel, level: [f32; 2]) {
    let xf = Xf::new(d, false);
    let Some(mat) = hand_matrix(d) else { return };
    // `ItemRenderer.renderItem` (bfn offsets 26-35) scales 3D models by 2 and `RenderItem.preTransform`
    // (bjh 409-416) scales flat ones by 2, before the display transform.
    let mat = chain(&[mat, scale(2.0), m.third, scale(0.5)]);
    // Model space is the biped's pixel frame; `Xf` maps it to the world.
    let to_world = |p: V3| xf.world(point(&mat, p).map(|c| c * 16.0));
    // Model y points down: the world y of a direction is its negation.
    let normal = |n: V3| {
        let [x, y, z] = dir(&mat, n);
        [-x, -y, z]
    };
    let glint = d.held_glint.then_some(d.age * 50.0);
    out.piece(m, &to_world, &normal, level, glint);
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
        out.piece(m, &to_world, &|n| dir(&mat, n), level, glint);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `ModelPlayer.postRenderArm`: a small arm's pivot moves 1 px in, which carries the item with it.
    #[test]
    fn a_slim_arm_holds_the_item_a_pixel_closer() {
        let mut d = super::super::tests::steve();
        d.held = 276;
        let wide = hand_matrix(&d).unwrap();
        d.class = Class::Player { slim: true };
        let slim = hand_matrix(&d).unwrap();
        assert!(((slim[3][0] - wide[3][0]).abs() - 1.0 / 16.0).abs() < 1e-5);
    }

    /// `LayerHeldItemWitch`: only chests take the block pose.
    #[test]
    fn the_witch_block_pose_is_for_chests() {
        assert!(is_chest(54) && is_chest(130) && is_chest(146));
        assert!(!is_chest(1));
    }

    fn top_color(mesh: &MeshData) -> [f32; 4] {
        mesh.vertices.iter().find(|v| v.pos[1] > 0.4).unwrap().color
    }

    fn textured() -> Atlas {
        let mut atlas = Atlas::flat();
        atlas.textured = true;
        atlas
    }

    fn rgb([r, g, b, _]: [f32; 4]) -> u32 {
        [r, g, b]
            .iter()
            .fold(0, |acc, c| acc << 8 | (c * 255.0).round() as u32)
    }

    /// `RenderItem` draws block quads white, or in their plains tint (`GrassColors`,
    /// `FoliageColors` at 0.5/1.0: 0x91bd59, 0x77ab2f) with no face shade or AO.
    #[test]
    fn flat_held_block_colors_are_the_tint_or_white() {
        let atlas = textured();
        for (id, tint) in [(1u16, 0xff_ffff), (2, 0x91_bd59), (18, 0x77_ab2f)] {
            let flat = hand::held_mesh(id << 4, &atlas, true);
            let colors: Vec<u32> = flat.vertices.iter().map(|v| rgb(v.color)).collect();
            assert_eq!(rgb(top_color(&flat)), tint, "block {id} top");
            assert!(
                colors.iter().all(|&c| c == tint || c == 0xff_ffff),
                "block {id}"
            );
            if id != 2 {
                assert!(colors.iter().all(|&c| c == tint), "block {id}");
            }
        }
    }

    /// Oracle: vanilla `LayerMooshroomMushroom` draws the block under `enableStandardItemLighting`:
    /// 0.4 + 0.6·N·L per light, L = (0.2,1,-0.7) and (-0.2,1,0.7). A vertical face (y = 0) gets at most
    /// 0.4 + 0.6·0.728/1.2207 = 0.758 and at least 0.4, never the full 1.0 of an unshaded quad.
    #[test]
    fn mooshroom_mushroom_sides_take_the_gl_light() {
        let mut d = super::super::tests::steve();
        d.class = Class::Mob(96);
        let model = resolve(
            &mut |_| None,
            &textured(),
            40,
            super::super::mooshroom::BLOCK_FORM,
            &DialTurns::default(),
        )
        .unwrap();
        let mut models = Models::new();
        models.insert(super::super::mooshroom::RED_MUSHROOM, Some(model));
        let mut out = Items::default();
        mushrooms(&mut out, &d, &models, [1.0, 1.0]);
        let colors: Vec<[f32; 4]> = out.atlas.vertices.iter().map(|v| v.color).collect();
        assert!(colors.len() >= 12 * 4, "three mushrooms of four quads");
        for c in colors {
            let m = c[0].max(c[1]).max(c[2]);
            assert!(m <= 0.76, "side face too bright: {m}");
            assert!(
                c[0].min(c[1]).min(c[2]) >= 0.39,
                "side face too dark: {c:?}"
            );
        }
    }

    /// Held sprites are drawn with back-face culling: every quad, sides included, must wind the same
    /// way about its outward normal, or an edge-on blade shows only the inside of its far faces.
    #[test]
    fn held_sprite_quads_all_wind_outward() {
        let mut d = super::super::tests::steve();
        d.held = 276;
        let mut rgba = vec![0u8; 3 * 3 * 4];
        for y in 0..3 {
            let o = (y * 3 + 1) * 4;
            rgba[o..o + 4].copy_from_slice(&[9, 9, 9, 255]);
        }
        let img = crate::assets::Image {
            width: 3,
            height: 3,
            rgba,
        };
        let flat = hand::Flat::new(&img, ident(), 0);
        let front = &flat.mesh.vertices[..4];
        assert!(front[3].pos[1] > front[0].pos[1] && front[3].uv[1] < front[0].uv[1]);
        let normals = flat.normals.clone();
        let layer = Layer {
            tex: ItemTex::Sprite("x".into()),
            normals: Some(flat.normals),
            mesh: flat.mesh,
            flat: vec![],
            mask: None,
        };
        let m = ItemModel {
            id: 276,
            damage: 0,
            layers: vec![layer],
            gui3d: false,
            third: ident(),
            ground: ident(),
            fixed: ident(),
            ground_scale: [1.0; 3],
            dial: None,
        };
        let mut out = Items::default();
        held(&mut out, &d, &m, [1.0, 1.0]);
        let mesh = out.sprites.values().next().unwrap();
        let mean = |pts: &[Vertex]| -> V3 {
            std::array::from_fn(|i| pts.iter().map(|v| v.pos[i]).sum::<f32>() / pts.len() as f32)
        };
        let centre = mean(&mesh.vertices);
        for (q, n) in mesh.vertices.chunks(4).zip(normals.iter()) {
            let out_of = std::array::from_fn::<f32, 3, _>(|i| mean(q)[i] - centre[i]);
            let sub = |a: usize| std::array::from_fn::<f32, 3, _>(|i| q[a].pos[i] - q[0].pos[i]);
            let (a, b) = (sub(1), sub(2));
            let c = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            assert!((0..3).map(|i| c[i] * out_of[i]).sum::<f32>() > 0.0, "{n:?}");
        }
    }
}
