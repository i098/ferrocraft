//! Vanilla block models read from the client jar: blockstate variants and model JSON (`parent`, `textures`,
//! `elements` with per-face `uv` and `rotation`, element `rotation`), baked to quads like `FaceBakery`.
//! A resource pack that replaces a blockstate or model changes the result, as in vanilla.

use crate::assets::AssetSource;
use crate::shapes::quarter;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

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
const BLOCKSTATES: [&str; 20] = [
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
    /// Face used for the brightness (`BakedQuad.getFace`), after the blockstate rotation.
    pub face: usize,
    /// `cullface` after the blockstate rotation: hidden when that neighbour is opaque.
    pub cull: Option<usize>,
    /// The element's `shade`: false draws at full brightness.
    pub shade: bool,
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
}

struct Element {
    from: [f32; 3],
    to: [f32; 3],
    rot: Option<([f32; 3], usize, f32, bool)>,
    shade: bool,
    faces: [Option<Face>; 6],
}

#[derive(Clone, Default)]
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
        });
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
    fn json(&mut self, path: &str) -> Option<Value> {
        serde_json::from_slice(&self.source.read(&format!("assets/minecraft/{path}.json"))?).ok()
    }

    /// `ModelBlock` with its `parent` chain: the child's textures win, its elements replace the parent's.
    fn model(&mut self, name: &str, depth: u8) -> Option<Model> {
        if let Some(m) = self.models.get(name) {
            return Some(m.clone());
        }
        let json = self.json(&format!("models/{}", with_prefix(name)))?;
        let mut model = match json.get("parent").and_then(Value::as_str) {
            Some(p) if depth < 16 && !p.starts_with("builtin/") => self.model(p, depth + 1)?,
            _ => Model::default(),
        };
        if let Some(t) = json.get("textures").and_then(Value::as_object) {
            for (k, v) in t {
                if let Some(v) = v.as_str() {
                    model.textures.insert(k.clone(), v.to_owned());
                }
            }
        }
        if let Some(list) = json.get("elements").and_then(Value::as_array) {
            model.elements = list
                .iter()
                .filter_map(parse_element)
                .collect::<Vec<_>>()
                .into();
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
        if r.starts_with('#') {
            return None;
        }
        let name = r.rsplit('/').next()?;
        if let Some(known) = self.names.iter().find(|n| &***n == name) {
            return Some(known.clone());
        }
        let new: Arc<str> = name.into();
        self.names.push(new.clone());
        Some(new)
    }

    fn bake(&mut self, spec: &Value) -> Vec<Quad> {
        let Some(model) = spec
            .get("model")
            .and_then(Value::as_str)
            .and_then(|m| self.model(m, 0))
        else {
            return Vec::new();
        };
        let turn = |k| spec.get(k).and_then(Value::as_u64).unwrap_or(0) as u8 / 90;
        let (x, y) = (turn("x"), turn("y"));
        let mut quads = Vec::new();
        for el in model.elements.iter() {
            for (f, face) in el.faces.iter().enumerate() {
                let Some(face) = face else { continue };
                let Some(tex) = self.texture(&model, &face.tex) else {
                    continue;
                };
                quads.push(bake_quad(el, f, face, tex, (x, y)));
            }
        }
        quads
    }
}

fn with_prefix(name: &str) -> String {
    if name.contains('/') {
        name.to_owned()
    } else {
        format!("block/{name}")
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

/// `FaceBakery.makeBakedQuad` for one face of `el`, then the blockstate rotation `(x, y)` in quarter turns.
fn bake_quad(el: &Element, f: usize, face: &Face, tex: Arc<str>, (x, y): (u8, u8)) -> Quad {
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
    Quad {
        pos,
        uv,
        tex,
        face: turned(NORMALS[f]).expect("a quarter turn maps a face normal to a face normal"),
        cull: face.cull.and_then(|c| turned(NORMALS[c])),
        shade: el.shade,
    }
}

const HORIZONTAL: [&str; 4] = ["south", "west", "north", "east"];
const FRONT: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// The torch, repeater, comparator, lever, pressure plate and tripwire hook states.
fn redstone_variant(id: u16, meta: usize, facing: &str) -> Option<(&'static str, String)> {
    Some(match id {
        50 | 75 | 76 => {
            let name = match id {
                50 => "torch",
                75 => "unlit_redstone_torch",
                _ => "redstone_torch",
            };
            let dir = ["up", "east", "west", "south", "north", "up"][meta.min(5)];
            (name, format!("facing={dir}"))
        }
        93 | 94 => (
            ["unpowered_repeater", "powered_repeater"][usize::from(id == 94)],
            format!("delay={},facing={facing},locked=false", (meta >> 2) + 1),
        ),
        149 | 150 => (
            ["unpowered_comparator", "powered_comparator"][usize::from(id == 150)],
            format!(
                "facing={facing},mode={},powered={}",
                ["compare", "subtract"][meta >> 2 & 1],
                id == 150 || meta & 8 != 0
            ),
        ),
        69 => {
            let dir = [
                "down_x", "east", "west", "south", "north", "up_z", "up_x", "down_z",
            ][meta & 7];
            ("lever", format!("facing={dir},powered={}", meta & 8 != 0))
        }
        70 | 72 => (
            ["stone_pressure_plate", "wooden_pressure_plate"][usize::from(id == 72)],
            format!("powered={}", meta == 1),
        ),
        147 | 148 => (
            [
                "light_weighted_pressure_plate",
                "heavy_weighted_pressure_plate",
            ][usize::from(id == 148)],
            format!("power={meta}"),
        ),
        131 => (
            "tripwire_hook",
            format!(
                "attached={},facing={facing},powered={},suspended=false",
                meta & 4 != 0,
                meta & 8 != 0
            ),
        ),
        _ => return None,
    })
}

/// `(blockstate, variant key)` of a block state: the key lists the properties in the order of the JSON files.
fn variant(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> Option<(&'static str, String)> {
    let (id, meta) = (state >> 4, state as usize & 15);
    let facing = HORIZONTAL[meta & 3];
    if let Some(found) = redstone_variant(id, meta, facing) {
        return Some(found);
    }
    Some(match id {
        127 => (
            "cocoa",
            format!("age={},facing={facing}", (meta >> 2).min(2)),
        ),
        145 => (
            "anvil",
            format!("damage={},facing={facing}", (meta >> 2).min(2)),
        ),
        34 => (
            "piston_head",
            format!(
                "facing={},short=false,type={}",
                FRONT[(meta & 7) % 6],
                ["normal", "sticky"][meta >> 3]
            ),
        ),
        65 => (
            "ladder",
            format!(
                "facing={}",
                ["north", "north", "north", "south", "west", "east"][meta.clamp(2, 5)]
            ),
        ),
        120 => (
            "end_portal_frame",
            format!("eye={},facing={facing}", meta & 4 != 0),
        ),
        64 | 71 | 193..=197 => {
            let name = match id {
                64 => "wooden_door",
                71 => "iron_door",
                _ => WOOD_DOORS[id as usize - 193],
            };
            (name, door_key(state, nb))
        }
        _ => return None,
    })
}

/// `BlockDoor.getActualState`: the lower half has the facing and `open`, the upper the hinge; each reads the other.
fn door_key(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> String {
    let upper = state & 8 != 0;
    let (low, high) = if upper {
        (nb(0, -1, 0), state)
    } else {
        (state, nb(0, 1, 0))
    };
    let same = |s: u16| s >> 4 == state >> 4;
    // `getStateFromMeta` + `rotateYCCW`: 0 east, 1 south, 2 west, 3 north.
    let (facing, open) = if same(low) {
        (
            ["east", "south", "west", "north"][low as usize & 3],
            low & 4 != 0,
        )
    } else {
        ("north", false)
    };
    let hinge = ["left", "right"][usize::from(same(high) && high & 1 != 0)];
    let half = ["lower", "upper"][usize::from(upper)];
    format!("facing={facing},half={half},hinge={hinge},open={open}")
}

impl Models {
    pub fn load(source: &mut AssetSource) -> Models {
        let mut loader = Loader {
            source,
            models: HashMap::new(),
            names: Vec::new(),
        };
        let mut states = HashMap::new();
        for name in BLOCKSTATES.iter().chain(&WOOD_DOORS) {
            let Some(json) = loader.json(&format!("blockstates/{name}")) else {
                continue;
            };
            let Some(variants) = json.get("variants").and_then(Value::as_object) else {
                continue;
            };
            let mut baked = HashMap::new();
            for (key, spec) in variants {
                // A list is a weighted random choice; the first entry stands for it.
                let spec = spec.as_array().and_then(|a| a.first()).unwrap_or(spec);
                baked.insert(key.clone(), loader.bake(spec));
            }
            states.insert((*name).to_owned(), baked);
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
