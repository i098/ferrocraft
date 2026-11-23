//! Vanilla block models read from the client jar: blockstate variants and model JSON (`parent`, `textures`,
//! `elements` with per-face `uv` and `rotation`, element `rotation`), baked to quads like `FaceBakery`.
//! A resource pack that replaces a blockstate or model changes the result, as in vanilla.

use crate::assets::AssetSource;
use crate::mesh::face_uv;
use crate::shapes::quarter;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use variants::variant;

/// Face indices in `UP, DOWN, NORTH, SOUTH, WEST, EAST` order, as in `mesh.rs`.
const FACE_NAMES: [&str; 6] = ["up", "down", "north", "south", "west", "east"];

/// `EnumFaceDirection.getVertexInformation`: per face the four vertices, each as min (0) or max (1) of x, y, z.
const VERTS: [[[u8; 3]; 4]; 6] = [
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    [[0, 0, 1], [0, 0, 0], [1, 0, 0], [1, 0, 1]],
    [[1, 1, 0], [1, 0, 0], [0, 0, 0], [0, 1, 0]],
    [[0, 1, 1], [0, 0, 1], [1, 0, 1], [1, 1, 1]],
    [[0, 1, 0], [0, 0, 0], [0, 0, 1], [0, 1, 1]],
    [[1, 1, 1], [1, 0, 1], [1, 0, 0], [1, 1, 0]],
];

/// Normals in the same face order, to turn a face index with a blockstate rotation.
const NORMALS: [[f32; 3]; 6] = [
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, -1.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
];

/// Blockstates this renderer draws from their models, by name.
const BLOCKSTATES: [&str; 35] = [
    "torch",
    "redstone_torch",
    "unlit_redstone_torch",
    "unpowered_repeater",
    "powered_repeater",
    "unpowered_comparator",
    "powered_comparator",
    "cocoa",
    "anvil",
    "piston_head",
    "piston",
    "sticky_piston",
    "rail",
    "golden_rail",
    "detector_rail",
    "activator_rail",
    "redstone_wire",
    "lever",
    "stone_pressure_plate",
    "wooden_pressure_plate",
    "light_weighted_pressure_plate",
    "heavy_weighted_pressure_plate",
    "ladder",
    "end_portal_frame",
    "tripwire_hook",
    "wooden_door",
    "iron_door",
    "cactus",
    "vine",
    "flower_pot",
    "beacon",
    "cobblestone_wall",
    "mossy_cobblestone_wall",
    "glass_pane",
    "bed",
];
/// The other door blockstates, one per wood.
const WOOD_DOORS: [&str; 5] = [
    "spruce_door",
    "birch_door",
    "jungle_door",
    "acacia_door",
    "dark_oak_door",
];

/// One baked face: block-local corners in `0..=1`, texture coordinates in `0..=1` of the tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub tex: Arc<str>,
    pub face: usize,
    pub shade_face: usize,
    /// `cullface` after the blockstate rotation: hidden when that neighbour is opaque.
    pub cull: Option<usize>,
    pub on_boundary: bool,
    pub shade: bool,
    /// The face has a `tintindex`: the block's biome or fixed tint multiplies the texel.
    pub tint: bool,
}

#[derive(Default)]
pub struct Models {
    states: HashMap<String, HashMap<String, Vec<Quad>>>,
    textures: Vec<Arc<str>>,
}

struct Face {
    uv: [f32; 4],
    turns: usize,
    tex: String,
    cull: Option<usize>,
    tint: bool,
}

struct Element {
    from: [f32; 3],
    to: [f32; 3],
    rot: Option<([f32; 3], usize, f32, bool)>,
    shade: bool,
    faces: [Option<Face>; 6],
}

#[derive(Clone)]
struct Model {
    textures: HashMap<String, String>,
    elements: std::rc::Rc<Vec<Element>>,
}

fn vec3(v: &Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    Some([0, 1, 2].map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32))
}

fn face_index(name: &str) -> Option<usize> {
    FACE_NAMES.iter().position(|n| *n == name)
}

/// `BlockPart.func_178236_a`: the `uv` a face gets when the JSON gives none.
fn default_uv(face: usize, from: [f32; 3], to: [f32; 3]) -> [f32; 4] {
    match face {
        0 | 1 => [from[0], from[2], to[0], to[2]],
        2 | 3 => [from[0], 16.0 - to[1], to[0], 16.0 - from[1]],
        _ => [from[2], 16.0 - to[1], to[2], 16.0 - from[1]],
    }
}

fn parse_element(e: &Value) -> Option<Element> {
    let (from, to) = (vec3(e.get("from")?)?, vec3(e.get("to")?)?);
    let rot = e.get("rotation").and_then(|r| {
        let name = r.get("axis")?.as_str()?;
        let axis = ["x", "y", "z"].iter().position(|a| *a == name)?;
        let angle = r.get("angle")?.as_f64()? as f32;
        let rescale = r.get("rescale").and_then(Value::as_bool) == Some(true);
        Some((vec3(r.get("origin")?)?, axis, angle, rescale))
    });
    let mut faces: [Option<Face>; 6] = Default::default();
    for (name, f) in e.get("faces")?.as_object()? {
        let Some(i) = face_index(name) else { continue };
        let uv = f
            .get("uv")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 4)
            .map(|a| [0, 1, 2, 3].map(|k| a[k].as_f64().unwrap_or(0.0) as f32))
            .unwrap_or_else(|| default_uv(i, from, to));
        faces[i] = Some(Face {
            uv,
            turns: f.get("rotation").and_then(Value::as_u64).unwrap_or(0) as usize / 90,
            tex: f.get("texture")?.as_str()?.to_owned(),
            cull: f
                .get("cullface")
                .and_then(Value::as_str)
                .and_then(face_index),
            tint: f.get("tintindex").is_some(),
        });
    }
    if faces.iter().all(Option::is_none) {
        return None;
    }
    Some(Element {
        from,
        to,
        rot,
        shade: e.get("shade").and_then(Value::as_bool) != Some(false),
        faces,
    })
}

/// Reads blockstates and models; keeps each model it has resolved.
struct Loader<'a> {
    source: &'a mut AssetSource,
    models: HashMap<String, Model>,
    names: Vec<Arc<str>>,
}

impl Loader<'_> {
    /// `ModelBlock` with its `parent` chain: the child's textures win, its elements replace the parent's.
    fn model(&mut self, name: &str, depth: u8) -> Option<Model> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        if depth > 16 || name.starts_with("builtin/") {
            return None;
        }
        if let Some(m) = self.models.get(name) {
            return Some(m.clone());
        }
        let path = format!("assets/minecraft/models/{name}.json");
        let json: serde_json::Map<String, Value> =
            serde_json::from_slice(&self.source.read(&path)?).ok()?;
        let parent = json.get("parent").map_or(Some(""), Value::as_str)?;
        let elements = json
            .get("elements")
            .map_or(Some(&[][..]), |list| list.as_array().map(Vec::as_slice))?;
        let mut model = match (parent.is_empty(), elements.is_empty()) {
            (false, true) => self.model(parent, depth + 1)?,
            (true, false) => Model {
                textures: HashMap::new(),
                elements: elements
                    .iter()
                    .map(parse_element)
                    .collect::<Option<Vec<_>>>()?
                    .into(),
            },
            _ => return None,
        };
        if let Some(t) = json.get("textures").and_then(Value::as_object) {
            for (k, v) in t {
                if let Some(v) = v.as_str() {
                    model.textures.insert(k.clone(), v.to_owned());
                }
            }
        }
        self.models.insert(name.to_owned(), model.clone());
        Some(model)
    }

    /// The atlas name of a face texture: `#var` is looked up in the model's textures, `blocks/x` is `x`.
    fn texture(&mut self, model: &Model, reference: &str) -> Option<Arc<str>> {
        let mut r = reference;
        for _ in 0..16 {
            match r.strip_prefix('#') {
                Some(var) => r = model.textures.get(var)?,
                None => break,
            }
        }
        let r = r.strip_prefix("minecraft:").unwrap_or(r);
        let name = r.strip_prefix("blocks/")?;
        if name.contains(['\\', ':']) || name.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return None;
        }
        if let Some(known) = self.names.iter().find(|n| &***n == name) {
            return Some(known.clone());
        }
        let new: Arc<str> = name.into();
        self.names.push(new.clone());
        Some(new)
    }

    fn bake(&mut self, spec: &Value) -> Option<Vec<Quad>> {
        let name = spec.get("model")?.as_str()?;
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        let model = self.model(&format!("block/{name}"), 0)?;
        let turn = |k| (spec.get(k).and_then(Value::as_u64).unwrap_or(0) / 90) as u8;
        let (x, y) = (turn("x"), turn("y"));
        let uvlock = spec.get("uvlock").and_then(Value::as_bool) == Some(true);
        let mut quads = Vec::new();
        for el in model.elements.iter() {
            for (f, face) in el.faces.iter().enumerate() {
                let Some(face) = face else { continue };
                let tex = self.texture(&model, &face.tex)?;
                quads.push(bake_quad(el, f, face, tex, (x, y), uvlock));
            }
        }
        Some(quads)
    }
}

/// `FaceBakery.rotatePart`: rotates `p` (1/16 units) about the element's `origin`.
fn rotate_part(
    p: [f32; 3],
    (origin, axis, angle, rescale): ([f32; 3], usize, f32, bool),
) -> [f32; 3] {
    let (s, c) = angle.to_radians().sin_cos();
    let q = [0, 1, 2].map(|a| p[a] - origin[a]);
    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut r = q;
    r[i] = q[i] * c - q[j] * s;
    r[j] = q[i] * s + q[j] * c;
    if rescale {
        let k = if angle.abs() == 22.5 {
            1.0 / 22.5f32.to_radians().cos() - 1.0
        } else {
            1.0 / 45.0f32.to_radians().cos() - 1.0
        };
        for a in (0..3).filter(|a| *a != axis) {
            r[a] *= 1.0 + k;
        }
    }
    [0, 1, 2].map(|a| r[a] + origin[a])
}

fn geometric_face(pos: &[[f32; 3]; 4], fallback: usize) -> usize {
    let a: [f32; 3] = std::array::from_fn(|i| pos[1][i] - pos[0][i]);
    let b: [f32; 3] = std::array::from_fn(|i| pos[2][i] - pos[0][i]);
    let [x, y, z] = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let mut face = fallback;
    let mut best = 0.0;
    for (i, score) in [y, -y, -z, z, -x, x].into_iter().enumerate() {
        if score > best {
            (face, best) = (i, score);
        }
    }
    face
}

/// `FaceBakery.makeBakedQuad` for one face of `el`, then the blockstate rotation `(x, y)` in quarter turns.
fn bake_quad(
    el: &Element,
    f: usize,
    face: &Face,
    tex: Arc<str>,
    (x, y): (u8, u8),
    uvlock: bool,
) -> Quad {
    let turned = |n: [f32; 3]| NORMALS.iter().position(|m| *m == quarter(n, x, y));
    let mut pos = [[0.0; 3]; 4];
    let mut uv = [[0.0; 2]; 4];
    for (i, c) in VERTS[f].iter().enumerate() {
        let mut p = [0, 1, 2].map(|a| if c[a] == 1 { el.to[a] } else { el.from[a] });
        if let Some(r) = el.rot {
            p = rotate_part(p, r);
        }
        pos[i] = quarter(p.map(|v| v / 16.0 - 0.5), x, y).map(|v| v + 0.5);
        // `BlockFaceUV.func_178348_a` / `func_178346_a`: the face `rotation` shifts which uv corner a vertex gets.
        let k = (i + face.turns) % 4;
        uv[i] = [
            if k == 0 || k == 1 {
                face.uv[0]
            } else {
                face.uv[2]
            } / 16.0,
            if k == 0 || k == 3 {
                face.uv[1]
            } else {
                face.uv[3]
            } / 16.0,
        ];
    }
    let shade_face =
        turned(NORMALS[f]).expect("a quarter turn maps a face normal to a face normal");
    let geometric_face = if el.rot.is_some() {
        geometric_face(&pos, shade_face)
    } else {
        shade_face
    };
    if uvlock {
        lock_uv(&mut uv, &pos, geometric_face, face.turns);
    }
    let on_boundary = on_boundary(&pos, geometric_face);
    Quad {
        pos,
        uv,
        tex,
        face: geometric_face,
        shade_face,
        cull: face.cull.and_then(|c| turned(NORMALS[c])),
        on_boundary,
        shade: el.shade,
        tint: face.tint,
    }
}

fn on_boundary(pos: &[[f32; 3]; 4], face: usize) -> bool {
    let axis = [1, 2, 0][face / 2];
    let plane = pos[0][axis];
    let reaches_boundary = if NORMALS[face][axis] < 0.0 {
        plane < 0.0001
    } else {
        plane > 0.9999
    };
    reaches_boundary && pos[1..].iter().all(|p| p[axis] == plane)
}

/// `FaceBakery.func_178401_a`: the texture stays fixed to the world; each vertex takes the default uv of its
/// turned position on the turned face (the face's own `uv` is not read), stored in the slot `rotation` shifts it to.
fn lock_uv(uv: &mut [[f32; 2]; 4], pos: &[[f32; 3]; 4], face: usize, turns: usize) {
    for (i, p) in pos.iter().enumerate() {
        uv[(i + 4 - turns % 4) % 4] = face_uv(face, *p);
    }
}

impl Models {
    pub fn load(source: &mut AssetSource) -> Models {
        let mut loader = Loader {
            source,
            models: HashMap::new(),
            names: Vec::new(),
        };
        let mut states = HashMap::new();
        for name in BLOCKSTATES
            .iter()
            .chain(&WOOD_DOORS)
            .chain(&variants::STAIRS)
            .chain(&variants::GATES)
        {
            let mut variants = serde_json::Map::new();
            for bytes in loader
                .source
                .read_all(&format!("assets/minecraft/blockstates/{name}.json"))
            {
                let Ok(Value::Object(mut json)) = serde_json::from_slice(&bytes) else {
                    continue;
                };
                if let Some(Value::Object(layer)) = json.remove("variants") {
                    variants.extend(layer);
                }
            }
            let mut baked = HashMap::new();
            for (key, spec) in variants {
                let entries = spec
                    .as_array()
                    .map_or(std::slice::from_ref(&spec), Vec::as_slice);
                let quads = entries.iter().find_map(|entry| {
                    let start = loader.names.len();
                    loader.bake(entry).or_else(|| {
                        loader.names.truncate(start);
                        None
                    })
                });
                if let Some(quads) = quads {
                    baked.insert(key, quads);
                }
            }
            if !baked.is_empty() {
                states.insert((*name).to_owned(), baked);
            }
        }
        Models {
            states,
            textures: loader.names,
        }
    }

    /// Textures the quads use that the block atlas must hold.
    pub fn textures(&self) -> &[Arc<str>] {
        &self.textures
    }

    /// The model quads of `state`, or `None` when this block is not drawn from a model (or none was loaded).
    pub fn quads(&self, state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> Option<&[Quad]> {
        let (name, key) = variant(state, nb)?;
        self.states.get(name)?.get(&key).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests;
mod variants;
