//! Item icons in GUIs: `RenderItem.renderItemAndEffectIntoGUI` and `renderItemOverlayIntoGUI`
//! of 1.8.9.
//!
//! Item models load once from the client jar (`models/item/*.json` and their parents). Flat
//! (`builtin/generated`) items blit their layers; block models and the chest and skull entity
//! models are projected with the exact GUI transform (`setupGuiTransform`, the model's `gui`
//! display transform) and lit with `RenderHelper.enableGUIStandardItemLighting`.

use std::collections::HashMap;

use super::{tooltip, Canvas};
use serde_json::Value;
use world::inventory::items;
use world::inventory::ItemStack;

type V3 = [f32; 3];

/// One textured face in block units (0..1), corners top-left, bottom-left, bottom-right,
/// top-right as seen from outside.
#[derive(Clone, Debug)]
struct Face {
    pts: [V3; 4],
    uv: [[f32; 2]; 4],
    tex: String,
    tint: Option<usize>,
}

/// `ItemTransformVec3f` of the `gui` display slot.
#[derive(Clone, Copy, Debug)]
struct Display {
    translation: V3,
    rotation: V3,
    scale: V3,
}

const IDENTITY: Display = Display {
    translation: [0.0; 3],
    rotation: [0.0; 3],
    scale: [1.0; 3],
};

#[derive(Debug)]
enum Model {
    /// `builtin/generated` (and compass, clock): texture layers, layer `i` tinted by pass `i`.
    Flat(Vec<String>),
    Solid {
        faces: Vec<Face>,
        gui: Display,
    },
}

#[derive(Debug)]
pub struct Icons {
    models: HashMap<&'static str, Model>,
    /// Frame height over texture height for animated (vertical strip) textures.
    frames: HashMap<String, f32>,
    /// `Minecraft.getSystemTime()`, which moves the enchantment glint.
    pub now_ms: u64,
}

const GLINT: &str = "textures/misc/enchanted_item_glint.png";
const GLINT_COLOR: u32 = 0xFF80_40CC;
/// Glint texture coordinates span a sprite's share of the 1.8.9 item atlas.
/// shortcut: assumes a 1024 px wide atlas, upgrade if the atlas size is tracked.
const ATLAS_SHARE: f32 = 16.0 / 1024.0;

fn texture_path(name: &str) -> String {
    format!("textures/{name}.png")
}

/// Width and height from a PNG `IHDR`.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let be = |at: usize| {
        png.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    Some((be(16)?, be(20)?))
}

fn v3(v: &Value) -> Option<V3> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()? as f32,
        a.get(1)?.as_f64()? as f32,
        a.get(2)?.as_f64()? as f32,
    ])
}

fn rot_x(a: f32, p: V3) -> V3 {
    let (s, c) = a.to_radians().sin_cos();
    [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]]
}

fn rot_y(a: f32, p: V3) -> V3 {
    let (s, c) = a.to_radians().sin_cos();
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

fn rot_z(a: f32, p: V3) -> V3 {
    let (s, c) = a.to_radians().sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 {
        a
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// `setupGuiTransform` for 3D models without the slot offset: scale(1,1,-1), scale 0.5 * 40,
/// rotate 210 about x, rotate -135 about y; then `renderItem` scales by 0.5 again.
fn gui_base(p: V3) -> V3 {
    let p = rot_x(210.0, rot_y(-135.0, p));
    [p[0] * 10.0, p[1] * 10.0, -p[2] * 10.0]
}

/// `ItemCameraTransforms.applyTransform`: translate, rotate y, x, z, then scale.
fn display(d: &Display, p: V3) -> V3 {
    let p = [p[0] * d.scale[0], p[1] * d.scale[1], p[2] * d.scale[2]];
    let p = rot_y(d.rotation[1], rot_x(d.rotation[0], rot_z(d.rotation[2], p)));
    [
        p[0] + d.translation[0],
        p[1] + d.translation[1],
        p[2] + d.translation[2],
    ]
}

fn display_dir(d: &Display, n: V3) -> V3 {
    rot_y(d.rotation[1], rot_x(d.rotation[0], rot_z(d.rotation[2], n)))
}

/// `RenderHelper.enableGUIStandardItemLighting`: the two item lights turned by
/// rotate(-30, y) and rotate(165, x), 0.4 ambient and 0.6 diffuse each.
fn light(n: V3) -> f32 {
    let l0 = rot_y(-30.0, rot_x(165.0, normalize([0.2, 1.0, -0.7])));
    let l1 = rot_y(-30.0, rot_x(165.0, normalize([-0.2, 1.0, 0.7])));
    (0.4 + 0.6 * dot(n, l0).max(0.0) + 0.6 * dot(n, l1).max(0.0)).min(1.0)
}

fn scale_rgb(argb: u32, k: f32) -> u32 {
    let ch = |shift: u32| ((((argb >> shift) & 0xFF) as f32 * k).round() as u32).min(255) << shift;
    (argb & 0xFF00_0000) | ch(16) | ch(8) | ch(0)
}

fn tint_color(s: &ItemStack, pass: Option<usize>) -> u32 {
    let rgb = pass
        .and_then(|i| items::variant(s.id, s.damage).and_then(|v| v.tint.get(i).copied()))
        .unwrap_or(0xFF_FFFF);
    0xFF00_0000 | rgb
}

/// Corners (top-left, bottom-left, bottom-right, top-right) of a block model element face,
/// in model pixels, and `BlockPartFace` default UVs.
fn element_face(dir: &str, f: V3, t: V3) -> Option<([V3; 4], [f32; 4])> {
    Some(match dir {
        "up" => (
            [
                [f[0], t[1], f[2]],
                [f[0], t[1], t[2]],
                [t[0], t[1], t[2]],
                [t[0], t[1], f[2]],
            ],
            [f[0], f[2], t[0], t[2]],
        ),
        "down" => (
            [
                [f[0], f[1], t[2]],
                [f[0], f[1], f[2]],
                [t[0], f[1], f[2]],
                [t[0], f[1], t[2]],
            ],
            [f[0], 16.0 - t[2], t[0], 16.0 - f[2]],
        ),
        "north" => (
            [
                [t[0], t[1], f[2]],
                [t[0], f[1], f[2]],
                [f[0], f[1], f[2]],
                [f[0], t[1], f[2]],
            ],
            [16.0 - t[0], 16.0 - t[1], 16.0 - f[0], 16.0 - f[1]],
        ),
        "south" => (
            [
                [f[0], t[1], t[2]],
                [f[0], f[1], t[2]],
                [t[0], f[1], t[2]],
                [t[0], t[1], t[2]],
            ],
            [f[0], 16.0 - t[1], t[0], 16.0 - f[1]],
        ),
        "west" => (
            [
                [f[0], t[1], f[2]],
                [f[0], f[1], f[2]],
                [f[0], f[1], t[2]],
                [f[0], t[1], t[2]],
            ],
            [f[2], 16.0 - t[1], t[2], 16.0 - f[1]],
        ),
        "east" => (
            [
                [t[0], t[1], t[2]],
                [t[0], f[1], t[2]],
                [t[0], f[1], f[2]],
                [t[0], t[1], f[2]],
            ],
            [16.0 - t[2], 16.0 - t[1], 16.0 - f[2], 16.0 - f[1]],
        ),
        _ => return None,
    })
}

/// `BlockPartRotation` applied by `FaceBakery.rotatePart`, including `rescale`.
fn element_rotation(rot: &Value, p: V3) -> V3 {
    let (Some(origin), Some(axis), Some(angle)) = (
        rot.get("origin").and_then(v3),
        rot["axis"].as_str(),
        rot["angle"].as_f64(),
    ) else {
        return p;
    };
    let angle = angle as f32;
    let q = sub(p, origin);
    let (mut q, k) = match axis {
        "x" => (rot_x(angle, q), 0),
        "y" => (rot_y(angle, q), 1),
        _ => (rot_z(angle, q), 2),
    };
    if rot["rescale"].as_bool() == Some(true) {
        let s = 1.0 / angle.to_radians().cos().abs();
        for (i, c) in q.iter_mut().enumerate() {
            if i != k {
                *c *= s;
            }
        }
    }
    [q[0] + origin[0], q[1] + origin[1], q[2] + origin[2]]
}

/// `ModelBox` faces (texture offset `uv`, box origin and size in model pixels, `grow` inflation),
/// mapped into block units by `map`; `map` must keep handedness.
fn model_box(
    tex: &str,
    size: (f32, f32),
    uv: (f32, f32),
    o: V3,
    d: V3,
    grow: f32,
    map: &dyn Fn(V3) -> V3,
) -> Vec<Face> {
    let (u, v) = uv;
    let (dx, dy, dz) = (d[0], d[1], d[2]);
    let (x, y, z) = (o[0] - grow, o[1] - grow, o[2] - grow);
    let (x2, y2, z2) = (o[0] + dx + grow, o[1] + dy + grow, o[2] + dz + grow);
    let p = [
        [x2, y, z],
        [x2, y2, z],
        [x, y2, z],
        [x, y, z2],
        [x2, y, z2],
        [x2, y2, z2],
        [x, y2, z2],
        [x, y, z],
    ];
    // TexturedQuad vertex order is top-right, top-left, bottom-left, bottom-right.
    let quads: [([usize; 4], [f32; 4]); 6] = [
        (
            [4, 0, 1, 5],
            [u + dz + dx, v + dz, u + dz + dx + dz, v + dz + dy],
        ),
        ([7, 3, 6, 2], [u, v + dz, u + dz, v + dz + dy]),
        ([4, 3, 7, 0], [u + dz, v, u + dz + dx, v + dz]),
        ([1, 2, 6, 5], [u + dz + dx, v + dz, u + dz + dx + dx, v]),
        ([0, 7, 2, 1], [u + dz, v + dz, u + dz + dx, v + dz + dy]),
        (
            [3, 4, 5, 6],
            [u + dz + dx + dz, v + dz, u + dz + dx + dz + dx, v + dz + dy],
        ),
    ];
    quads
        .iter()
        .map(|(vi, [u1, v1, u2, v2])| {
            let (tw, th) = size;
            let (u1, v1, u2, v2) = (u1 / tw, v1 / th, u2 / tw, v2 / th);
            Face {
                pts: [map(p[vi[1]]), map(p[vi[2]]), map(p[vi[3]]), map(p[vi[0]])],
                uv: [[u1, v1], [u1, v2], [u2, v2], [u2, v1]],
                tex: tex.to_string(),
                tint: None,
            }
        })
        .collect()
}

/// `ModelChest` as `TileEntityChestRenderer` draws it for an item (facing 0, lid shut).
fn chest(tex: &str) -> Vec<Face> {
    let map = |q: V3| [q[0] / 16.0, 1.0 - q[1] / 16.0, 1.0 - q[2] / 16.0];
    let at = |rp: V3, o: V3| [rp[0] + o[0], rp[1] + o[1], rp[2] + o[2]];
    let size = (64.0, 64.0);
    let mut f = model_box(
        tex,
        size,
        (0.0, 19.0),
        at([1.0, 6.0, 1.0], [0.0; 3]),
        [14.0, 10.0, 14.0],
        0.0,
        &map,
    );
    f.extend(model_box(
        tex,
        size,
        (0.0, 0.0),
        at([1.0, 7.0, 15.0], [0.0, -5.0, -14.0]),
        [14.0, 5.0, 14.0],
        0.0,
        &map,
    ));
    f.extend(model_box(
        tex,
        size,
        (0.0, 0.0),
        at([8.0, 7.0, 15.0], [-1.0, -2.0, -15.0]),
        [2.0, 4.0, 1.0],
        0.0,
        &map,
    ));
    f
}

/// `TileEntitySkullRenderer.renderSkull` for an item: `TileEntityItemStackRenderer` moves it by
/// (-0.5, 0, -0.5) and scales it by 2; the skull is drawn upright, facing 0.
fn skull(kind: i16) -> Vec<Face> {
    let map = |q: V3| [0.5 - q[0] / 8.0, -q[1] / 8.0, 0.5 + q[2] / 8.0];
    let (tex, size, humanoid) = match kind {
        1 => (
            "textures/entity/skeleton/wither_skeleton.png",
            (64.0, 32.0),
            false,
        ),
        2 => ("textures/entity/zombie/zombie.png", (64.0, 64.0), true),
        3 => ("textures/entity/steve.png", (64.0, 64.0), true),
        4 => ("textures/entity/creeper/creeper.png", (64.0, 32.0), false),
        _ => ("textures/entity/skeleton/skeleton.png", (64.0, 32.0), false),
    };
    let mut f = model_box(
        tex,
        size,
        (0.0, 0.0),
        [-4.0, -8.0, -4.0],
        [8.0; 3],
        0.0,
        &map,
    );
    if humanoid {
        // ModelHumanoidHead's hat layer.
        f.extend(model_box(
            tex,
            size,
            (32.0, 0.0),
            [-4.0, -8.0, -4.0],
            [8.0; 3],
            0.25,
            &map,
        ));
    }
    f
}

/// The `gui` display transform of the nearest model in the chain that has one.
fn gui_display(chain: &[(String, Value)]) -> Display {
    chain
        .iter()
        .find_map(|(_, v)| v["display"].get("gui"))
        .map_or(IDENTITY, |g| Display {
            translation: g
                .get("translation")
                .and_then(v3)
                .map_or([0.0; 3], |t| t.map(|c| (c / 16.0).clamp(-1.5, 1.5))),
            rotation: g.get("rotation").and_then(v3).unwrap_or([0.0; 3]),
            scale: g
                .get("scale")
                .and_then(v3)
                .map_or([1.0; 3], |s| s.map(|c| c.clamp(-4.0, 4.0))),
        })
}

/// Texture variables of the chain; a child's value hides its parents'.
fn texture_map(chain: &[(String, Value)]) -> HashMap<String, String> {
    let mut textures = HashMap::new();
    for (_, v) in chain {
        for (k, t) in v["textures"].as_object().into_iter().flatten() {
            if let Some(t) = t.as_str() {
                textures.entry(k.clone()).or_insert_with(|| t.to_string());
            }
        }
    }
    textures
}

/// Follows `#variable` references to a texture path; gives up after 8 hops.
fn resolve_texture(textures: &HashMap<String, String>, t: &str) -> Option<String> {
    let mut t = t.to_string();
    for _ in 0..8 {
        match t.strip_prefix('#') {
            Some(k) => t = textures.get(k)?.clone(),
            None => return Some(texture_path(&t)),
        }
    }
    None
}

/// The faces `TileEntityItemStackRenderer` draws for a `builtin/entity` item.
fn entity_faces(name: &str) -> Option<Vec<Face>> {
    Some(match name {
        "chest" => chest("textures/entity/chest/normal.png"),
        "trapped_chest" => chest("textures/entity/chest/trapped.png"),
        "ender_chest" => chest("textures/entity/chest/ender.png"),
        n if n.starts_with("skull_") => {
            let kinds = [
                "skull_skeleton",
                "skull_wither",
                "skull_zombie",
                "skull_char",
                "skull_creeper",
            ];
            skull(kinds.iter().position(|k| *k == n).unwrap_or(0) as i16)
        }
        // shortcut: banners (TileEntityBannerRenderer with patterns) draw nothing yet; add when banner patterns are ported.
        _ => return None,
    })
}

/// The faces of the `elements` of the nearest model in the chain, as `FaceBakery` bakes them.
fn element_faces(
    chain: &[(String, Value)],
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Option<Vec<Face>> {
    let elements = chain.iter().find_map(|(_, v)| v["elements"].as_array())?;
    let mut faces = Vec::new();
    for e in elements {
        let (Some(f), Some(t)) = (e.get("from").and_then(v3), e.get("to").and_then(v3)) else {
            continue;
        };
        for (dir, face) in e["faces"].as_object().into_iter().flatten() {
            let Some((pts, default_uv)) = element_face(dir, f, t) else {
                continue;
            };
            let Some(tex) = face["texture"].as_str().and_then(resolve) else {
                continue;
            };
            let uv = face.get("uv").and_then(|u| {
                let a = u.as_array()?;
                Some([0, 1, 2, 3].map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32))
            });
            let [u0, v0, u1, v1] = uv.unwrap_or(default_uv).map(|c| c / 16.0);
            let corners = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]];
            let r = (face["rotation"].as_u64().unwrap_or(0) / 90) as usize;
            let pts = pts.map(|p| {
                let p = e.get("rotation").map_or(p, |rot| element_rotation(rot, p));
                p.map(|c| c / 16.0)
            });
            faces.push(Face {
                pts,
                uv: [0, 1, 2, 3].map(|k| corners[(k + r) % 4]),
                tex,
                tint: face["tintindex"].as_u64().map(|i| i as usize),
            });
        }
    }
    Some(faces)
}

/// A model and its parents, child first.
fn chain(read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, name: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut next = Some(name.to_string());
    while let Some(n) = next.take() {
        if n.starts_with("builtin/") || out.len() > 16 {
            out.push((n, Value::Null));
            break;
        }
        let Some(v) = read(&format!("models/{n}.json"))
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            break;
        };
        next = v["parent"].as_str().map(str::to_string);
        out.push((n, v));
    }
    out
}

impl Icons {
    /// Loads every item model of the item table; `read` returns a file of the client jar
    /// below `assets/minecraft/` (for example `models/item/stone.json`).
    pub fn new(mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Icons {
        let mut icons = Icons {
            models: HashMap::new(),
            frames: HashMap::new(),
            now_ms: 0,
        };
        for name in items::model_names() {
            if let Some(m) = icons.load(&mut read, name) {
                icons.models.insert(name, m);
            }
        }
        icons
    }

    fn frame(&mut self, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, tex: &str) {
        if !self.frames.contains_key(tex) {
            let k = read(tex)
                .as_deref()
                .and_then(png_size)
                .map_or(1.0, |(w, h)| if h > w { w as f32 / h as f32 } else { 1.0 });
            self.frames.insert(tex.to_string(), k);
        }
    }

    fn load(&mut self, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, name: &str) -> Option<Model> {
        let chain = chain(read, &format!("item/{name}"));
        let textures = texture_map(&chain);
        let resolve = |t: &str| resolve_texture(&textures, t);
        let model = match chain.last().map(|(n, _)| n.as_str()) {
            Some("builtin/generated" | "builtin/compass" | "builtin/clock") => Model::Flat(
                (0..5)
                    .map_while(|i| resolve(&format!("#layer{i}")))
                    .collect(),
            ),
            Some("builtin/entity") => Model::Solid {
                faces: entity_faces(name)?,
                gui: gui_display(&chain),
            },
            _ => Model::Solid {
                faces: element_faces(&chain, &resolve)?,
                gui: gui_display(&chain),
            },
        };
        let texs: Vec<String> = match &model {
            Model::Flat(layers) => layers.clone(),
            Model::Solid { faces, .. } => faces.iter().map(|f| f.tex.clone()).collect(),
        };
        for t in texs {
            self.frame(read, &t);
        }
        Some(model)
    }

    fn frame_scale(&self, tex: &str) -> f32 {
        self.frames.get(tex).copied().unwrap_or(1.0)
    }

    /// `renderItemAndEffectIntoGUI` at GUI position `(x, y)` (the slot's top-left).
    pub fn draw(&self, c: &mut dyn Canvas, s: &ItemStack, x: i32, y: i32) {
        let Some(model) = items::model(s.id, s.damage).and_then(|n| self.models.get(n)) else {
            return;
        };
        let (x, y) = (x as f32, y as f32);
        let glint = tooltip::glint(s);
        match model {
            Model::Flat(layers) => {
                for (i, tex) in layers.iter().enumerate() {
                    let k = self.frame_scale(tex);
                    // shortcut: compass and clock show frame 0; upgrade when the engine passes the world angle and time.
                    c.blit(
                        tex,
                        x,
                        y,
                        16.0,
                        16.0,
                        [0.0, 0.0, 1.0, k],
                        tint_color(s, Some(i)),
                    );
                }
                if glint {
                    let pts = [[x, y], [x + 16.0, y], [x + 16.0, y + 16.0], [x, y + 16.0]];
                    self.glint(c, pts, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
                }
            }
            Model::Solid { faces, gui } => {
                let mut shown: Vec<(f32, [[f32; 2]; 4], &Face, u32)> = Vec::new();
                for f in faces {
                    let n = normalize(cross(sub(f.pts[1], f.pts[0]), sub(f.pts[2], f.pts[0])));
                    let n = display_dir(gui, n);
                    let n = rot_x(210.0, rot_y(-135.0, n));
                    let n = [n[0], n[1], -n[2]];
                    if n[2] <= 0.0 {
                        continue;
                    }
                    let eye = f.pts.map(|p| gui_base(display(gui, sub(p, [0.5; 3]))));
                    let depth = eye.iter().map(|p| p[2]).sum::<f32>() / 4.0;
                    // Canvas corners run top-left, top-right, bottom-right, bottom-left.
                    let pts = [0, 3, 2, 1].map(|i| [x + 8.0 + eye[i][0], y + 8.0 + eye[i][1]]);
                    // `renderQuad` overwrites the baked face shade with `putColor4`: only GL lighting shades.
                    let color = scale_rgb(tint_color(s, f.tint), light(n));
                    shown.push((depth, pts, f, color));
                }
                shown.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, pts, f, color) in &shown {
                    let k = self.frame_scale(&f.tex);
                    let uv = [0, 3, 2, 1].map(|i| [f.uv[i][0], f.uv[i][1] * k]);
                    c.quad(&f.tex, *pts, uv, [*color; 4]);
                    if glint {
                        self.glint(c, *pts, uv);
                    }
                }
            }
        }
    }

    /// `RenderItem.renderEffect`: two additive passes of the glint texture, each scaled by 8,
    /// scrolled by the clock and turned by -50 and 10 degrees.
    fn glint(&self, c: &mut dyn Canvas, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4]) {
        let passes = [
            ((self.now_ms % 3000) as f32 / 3000.0 / 8.0, -50.0_f32),
            (-((self.now_ms % 4873) as f32) / 4873.0 / 8.0, 10.0),
        ];
        for (shift, angle) in passes {
            let uv = uv.map(|[u, v]| {
                let r = rot_z(angle, [u * ATLAS_SHARE, v * ATLAS_SHARE, 0.0]);
                [8.0 * (r[0] + shift), 8.0 * r[1]]
            });
            c.quad_add(GLINT, pts, uv, GLINT_COLOR);
        }
    }

    /// `renderItemOverlayIntoGUI`: the stack size (or `text`) and the durability bar.
    pub fn overlay(&self, c: &mut dyn Canvas, s: &ItemStack, x: i32, y: i32, text: Option<&str>) {
        if s.count != 1 || text.is_some() {
            let label = match text {
                Some(t) => t.to_string(),
                None if s.count < 1 => format!("\u{a7}c{}", s.count),
                None => s.count.to_string(),
            };
            c.text(
                &label,
                (x + 19 - 2 - c.width(&label)) as f32,
                (y + 6 + 3) as f32,
                0xFFFF_FFFF,
                true,
            );
        }
        let max = i32::from(items::item(s.id).max_damage);
        let unbreakable = s
            .nbt
            .as_ref()
            .and_then(|n| tooltip::get(&n.root, "Unbreakable"))
            .is_some_and(|t| matches!(t, world::inventory::Tag::Byte(b) if *b != 0));
        if max > 0 && s.damage > 0 && !unbreakable {
            let d = f64::from(s.damage) / f64::from(max);
            let len = (13.0 - d * 13.0).round() as i32;
            let g = (255.0 - d * 255.0).round() as u32;
            let (x, y) = (x + 2, y + 13);
            c.fill(x, y, x + 13, y + 2, 0xFF00_0000);
            c.fill(
                x,
                y,
                x + 12,
                y + 1,
                0xFF00_0000 | (((255 - g) / 4) << 16) | (64 << 8),
            );
            c.fill(
                x,
                y,
                x + len,
                y + 1,
                0xFF00_0000 | ((255 - g) << 16) | (g << 8),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube shows its top, north and east faces, the top brightest.
    #[test]
    fn gui_cube_faces() {
        let f = |dir| {
            let (pts, _) = element_face(dir, [0.0; 3], [16.0; 3]).unwrap();
            let pts = pts.map(|p| p.map(|c| c / 16.0));
            let n = rot_x(
                210.0,
                rot_y(
                    -135.0,
                    normalize(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]))),
                ),
            );
            let n = [n[0], n[1], -n[2]];
            (n[2] > 0.0, light(n))
        };
        let shown: Vec<&str> = ["up", "down", "north", "south", "west", "east"]
            .into_iter()
            .filter(|d| f(d).0)
            .collect();
        assert_eq!(shown, ["up", "north", "east"]);
        // Top lit fully, east (screen left) brighter than north (screen right).
        assert!(f("up").1 > f("east").1 && f("east").1 > f("north").1);
        // The cube spans the 16 px slot: its top corner is at the slot's top edge.
        let top = gui_base(sub([0.0, 1.0, 1.0], [0.5; 3]));
        assert!((8.0 + top[1]).abs() < 1.5, "{top:?}");
    }
}
