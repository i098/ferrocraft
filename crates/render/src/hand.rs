//! First-person hand and held item, ported from vanilla 1.8.9 `ItemRenderer.renderItemInFirstPerson`
//! (`transformFirstPersonItem`, the empty-hand arm of `RenderPlayer.renderRightArm`, and
//! `ItemModelGenerator` for the extruded sprite of flat items).

use crate::assets::{Atlas, Image};
use crate::mesh::{mesh_chunk, MeshData, Neighborhood, Vertex, HEIGHT};
use serde_json::Value;
use std::f32::consts::PI;

pub type Mat = [[f32; 4]; 4];

/// What the hand holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldKind {
    /// Block state id (`id << 4 | meta`) drawn as a cube.
    Block(u16),
    /// An item stack: `builtin/generated` models (tools, food, torches, saplings...) draw as the
    /// extruded sprite, other ids below 256 as the block `id << 4 | damage & 15`.
    Item { id: i16, damage: i16 },
}

/// What the engine tells the renderer about the player's hand for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    /// `None` draws the bare arm.
    pub held: Option<HeldKind>,
    /// `getSwingProgress(partial)`, `0..1`.
    pub swing: f32,
    /// `1 - equippedProgress`: 0 with the item fully raised, 1 fully lowered.
    pub lowered: f32,
    /// A timed use in progress (`getItemUseTimer() > 0`).
    pub using: Option<Using>,
    /// `ItemStack.hasEffect` of the held item: draws the enchantment glint over it.
    pub glint: bool,
}

/// `EnumAction` of the item being used, as far as the first-person pose differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseAction {
    /// `EAT` and `DRINK`.
    Consume,
    /// `BLOCK`: swords.
    Block,
    Bow,
}

impl UseAction {
    /// `Item.getMaxItemUseDuration`.
    fn duration(self) -> f32 {
        match self {
            UseAction::Consume => 32.0,
            UseAction::Block | UseAction::Bow => 72_000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Using {
    pub action: UseAction,
    /// `getItemUseTimer() - partialTicks + 1`.
    pub timer: f32,
}

/// One frame of hand geometry: vertices in model space and the matrix that places them in eye space.
pub struct Draw {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub model: Mat,
}

pub(crate) fn ident() -> Mat {
    let mut m = [[0.0; 4]; 4];
    for (i, c) in m.iter_mut().enumerate() {
        c[i] = 1.0;
    }
    m
}

pub(crate) fn mul(a: Mat, b: Mat) -> Mat {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

pub(crate) fn translate(x: f32, y: f32, z: f32) -> Mat {
    let mut m = ident();
    m[3] = [x, y, z, 1.0];
    m
}

pub(crate) fn scale(s: f32) -> Mat {
    scale3([s; 3])
}

pub(crate) fn scale3(s: [f32; 3]) -> Mat {
    let mut m = ident();
    for (i, c) in m.iter_mut().enumerate().take(3) {
        c[i] = s[i];
    }
    m
}

/// `glRotatef(deg, axis)` for `axis` 0, 1 or 2.
pub(crate) fn rotate(deg: f32, axis: usize) -> Mat {
    let (s, c) = deg.to_radians().sin_cos();
    let mut m = ident();
    let (a, b) = [(1, 2), (2, 0), (0, 1)][axis];
    m[a][a] = c;
    m[b][b] = c;
    m[a][b] = s;
    m[b][a] = -s;
    m
}

pub(crate) fn chain(ms: &[Mat]) -> Mat {
    ms.iter().fold(ident(), |acc, m| mul(acc, *m))
}

/// `ItemRenderer.transformFirstPersonItem`.
fn item_transform(swing: f32, lowered: f32) -> Mat {
    let root = swing.sqrt();
    chain(&[
        translate(
            -0.4 * (root * PI).sin(),
            0.2 * (root * 2.0 * PI).sin(),
            -0.2 * (swing * PI).sin(),
        ),
        translate(0.56, -0.52, -0.72),
        translate(0.0, lowered * -0.6, 0.0),
        rotate(45.0, 1),
        rotate((swing * swing * PI).sin() * -20.0, 1),
        rotate((root * PI).sin() * -20.0, 2),
        rotate((root * PI).sin() * -80.0, 0),
        scale(0.4),
    ])
}

/// `ItemRenderer.renderItemInFirstPerson`: the pose of the held item. While it is in use the
/// swing is skipped and the action's transform joins the plain first-person one.
fn held_transform(hand: &Hand) -> Mat {
    let Some(Using { action, timer }) = hand.using else {
        return item_transform(hand.swing, hand.lowered);
    };
    let base = item_transform(0.0, hand.lowered);
    match action {
        UseAction::Consume => {
            let progress = timer / action.duration();
            let bob = if progress >= 0.8 {
                0.0
            } else {
                (timer / 4.0 * PI).cos().abs() * 0.1
            };
            let c = 1.0 - progress.powi(27);
            chain(&[
                translate(0.0, bob, 0.0),
                translate(c * 0.6, c * -0.5, 0.0),
                rotate(c * 90.0, 1),
                rotate(c * 10.0, 0),
                rotate(c * 30.0, 2),
                base,
            ])
        }
        UseAction::Block => chain(&[
            base,
            translate(-0.5, 0.2, 0.0),
            rotate(30.0, 1),
            rotate(-80.0, 0),
            rotate(60.0, 1),
        ]),
        UseAction::Bow => {
            let ticks = action.duration() - timer;
            let t = ticks / 20.0;
            let draw = ((t * t + t * 2.0) / 3.0).min(1.0);
            let lift = if draw > 0.1 {
                ((ticks - 0.1) * 1.3).sin() * (draw - 0.1) * 0.01
            } else {
                0.0
            };
            chain(&[
                base,
                rotate(-18.0, 2),
                rotate(-12.0, 1),
                rotate(-8.0, 0),
                translate(-0.9, 0.2, 0.0),
                translate(0.0, lift, 0.0),
                translate(0.0, 0.0, draw * 0.1),
                scale3([1.0, 1.0, 1.0 + draw * 0.2]),
            ])
        }
    }
}

/// A single block centred on the origin, as `RenderItem.renderItem` draws it (`scale(0.5)` follows in the model matrix).
pub fn held_mesh(state: u16, atlas: &Atlas) -> MeshData {
    let mut blocks = vec![0u16; 16 * HEIGHT * 16];
    blocks[crate::mesh::index(8, 128, 8)] = state;
    let hood = Neighborhood {
        origin: (0, 0),
        center: &blocks,
        neighbors: [None; 8],
    };
    let m = mesh_chunk(&hood, atlas);
    let mut out = m.opaque;
    let base = out.vertices.len() as u32;
    out.vertices.extend(m.water.vertices);
    out.indices.extend(m.water.indices.iter().map(|i| i + base));
    for v in &mut out.vertices {
        v.pos = [v.pos[0] - 8.5, v.pos[1] - 128.5, v.pos[2] - 8.5];
    }
    out
}

pub fn block(hand: &Hand, mesh: &MeshData) -> Draw {
    Draw {
        vertices: mesh.vertices.clone(),
        indices: mesh.indices.clone(),
        model: mul(held_transform(hand), scale(0.5)),
    }
}

/// A flat item: the extruded sprite with one normal per quad, and its `firstperson` display transform.
pub struct Flat {
    pub(crate) mesh: MeshData,
    pub(crate) normals: Vec<[f32; 3]>,
    display: Mat,
}

/// `ItemTransformVec3f.Deserializer` and `ItemCameraTransforms.applyTransform` of one display entry:
/// translation in 1/16 clamped to 1.5, scale clamped to 4, then translate, rotate y, x, z, scale.
/// Also returns the scale, which `RenderEntityItem` reads for the ground transform.
fn transform(d: &Value) -> (Mat, [f32; 3]) {
    let v3 = |key: &str, default: f32| -> [f32; 3] {
        let a = d.get(key).and_then(Value::as_array);
        [0, 1, 2].map(|i| {
            a.and_then(|a| a.get(i))
                .and_then(Value::as_f64)
                .map_or(default, |c| c as f32)
        })
    };
    let [t0, t1, t2] = v3("translation", 0.0).map(|c| (c / 16.0).clamp(-1.5, 1.5));
    let r = v3("rotation", 0.0);
    let s = v3("scale", 1.0).map(|c| c.clamp(-4.0, 4.0));
    let mat = chain(&[
        translate(t0, t1, t2),
        rotate(r[1], 1),
        rotate(r[0], 0),
        rotate(r[2], 2),
        scale3(s),
    ]);
    (mat, s)
}

/// The `display` entry `key` (`thirdperson`, `ground`, `fixed`...) of `models/item/<name>.json` or
/// its nearest parent that has one; absent entries are `ItemTransformVec3f.DEFAULT` (identity).
pub(crate) fn display(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    name: &str,
    key: &str,
) -> (Mat, [f32; 3]) {
    let mut next = format!("item/{name}");
    for _ in 0..16 {
        if next.starts_with("builtin/") {
            break;
        }
        let Some(v) = read(&format!("models/{next}.json"))
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            break;
        };
        if let Some(d) = v["display"].get(key) {
            return transform(d);
        }
        let Some(parent) = v["parent"].as_str() else {
            break;
        };
        next = parent.to_string();
    }
    (ident(), [1.0; 3])
}

/// Reads `models/item/<name>.json` and its parents through `read` (paths below `assets/minecraft/`).
/// Returns the `layer0` texture path and the `firstperson` display transform when the chain ends
/// in a `builtin/generated` (or clock/compass) model; `None` for block and entity models.
pub fn flat_model(
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    name: &str,
) -> Option<(String, Mat)> {
    let mut models = Vec::new();
    let mut next = format!("item/{name}");
    loop {
        if matches!(
            next.as_str(),
            "builtin/generated" | "builtin/compass" | "builtin/clock"
        ) {
            break;
        }
        if next.starts_with("builtin/") || models.len() > 16 {
            return None;
        }
        let v: Value = serde_json::from_slice(&read(&format!("models/{next}.json"))?).ok()?;
        next = v["parent"].as_str()?.to_string();
        models.push(v);
    }
    let layer = models
        .iter()
        .find_map(|v| v["textures"]["layer0"].as_str())?;
    let display = models
        .iter()
        .find_map(|v| v["display"].get("firstperson"))
        .map_or(ident(), |d| transform(d).0);
    Some((format!("textures/{layer}.png"), display))
}

impl Flat {
    /// `ItemModelGenerator`: front and back faces 1/16 apart plus a side face on every opaque
    /// pixel edge that borders a transparent pixel, in `0..1` model units. Animated strips use frame 0.
    pub fn new(img: &Image, display: Mat) -> Flat {
        let (w, th) = (img.width, img.height);
        let h = w.min(th);
        let opaque = |x: isize, y: isize| {
            (0..w as isize).contains(&x)
                && (0..h as isize).contains(&y)
                && img.rgba[(y as usize * w + x as usize) * 4 + 3] != 0
        };
        let (z0, z1) = (7.5 / 16.0, 8.5 / 16.0);
        let vmax = h as f32 / th as f32;
        let mut mesh = MeshData::default();
        let mut normals = Vec::new();
        let mut quad = |corners: [[f32; 3]; 4], uv: [[f32; 2]; 4], normal: [f32; 3]| {
            let base = mesh.vertices.len() as u32;
            for (pos, uv) in corners.into_iter().zip(uv) {
                mesh.vertices.push(Vertex {
                    pos,
                    uv,
                    color: [1.0; 4],
                    light: [1.0, 0.0],
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
            normals.push(normal);
        };
        let full = [[0.0, 0.0], [1.0, 0.0], [1.0, vmax], [0.0, vmax]];
        for (z, n) in [(z1, 1.0), (z0, -1.0)] {
            let c = [[0.0, 1.0, z], [1.0, 1.0, z], [1.0, 0.0, z], [0.0, 0.0, z]];
            quad(c, full, [0.0, 0.0, n]);
        }
        let (fw, fh) = (w as f32, h as f32);
        for y in 0..h as isize {
            for x in 0..w as isize {
                if !opaque(x, y) {
                    continue;
                }
                let uv = [[(x as f32 + 0.5) / fw, (y as f32 + 0.5) / th as f32]; 4];
                let (x0, x1) = (x as f32 / fw, (x + 1) as f32 / fw);
                let (y1, y0) = (1.0 - y as f32 / fh, 1.0 - (y + 1) as f32 / fh);
                if !opaque(x - 1, y) {
                    let c = [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]];
                    quad(c, uv, [-1.0, 0.0, 0.0]);
                }
                if !opaque(x + 1, y) {
                    let c = [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]];
                    quad(c, uv, [1.0, 0.0, 0.0]);
                }
                if !opaque(x, y - 1) {
                    let c = [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]];
                    quad(c, uv, [0.0, 1.0, 0.0]);
                }
                if !opaque(x, y + 1) {
                    let c = [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]];
                    quad(c, uv, [0.0, -1.0, 0.0]);
                }
            }
        }
        Flat {
            mesh,
            normals,
            display,
        }
    }
}

/// `transformFirstPersonItem`, the model's `firstperson` transform, then `RenderItem.renderItem`
/// (`scale(0.5)`, `translate(-0.5)`), lit per face like the arm.
pub fn flat(hand: &Hand, f: &Flat) -> Draw {
    let model = chain(&[
        held_transform(hand),
        f.display,
        scale(0.5),
        translate(-0.5, -0.5, -0.5),
    ]);
    let mut vertices = f.mesh.vertices.clone();
    for (quad, n) in vertices.chunks_mut(4).zip(&f.normals) {
        let shade = item_light(&model, *n);
        for v in quad {
            v.color = [shade, shade, shade, 1.0];
        }
    }
    Draw {
        vertices,
        indices: f.mesh.indices.clone(),
        model,
    }
}

/// Right arm box `addBox(-3, -2, -2, 4, 12, 4)` at skin pixel (40, 16); model y points down.
const BOX: [[f32; 2]; 3] = [[-3.0, 1.0], [-2.0, 10.0], [-2.0, 2.0]];

/// Eye-space placement of the right arm: `ItemRenderer` swing, `ModelRenderer` rotation point and z rotation.
fn arm_model(hand: &Hand) -> Mat {
    let (swing, root) = (hand.swing, hand.swing.sqrt());
    let place = chain(&[
        translate(
            -0.3 * (root * PI).sin(),
            0.4 * (root * 2.0 * PI).sin(),
            -0.4 * (swing * PI).sin(),
        ),
        translate(0.64, -0.6 + hand.lowered * -0.6, -0.72),
        rotate(45.0, 1),
        rotate((root * PI).sin() * 70.0, 1),
        rotate((swing * swing * PI).sin() * -20.0, 2),
        translate(-1.0, 3.6, 3.5),
        rotate(120.0, 2),
        rotate(200.0, 0),
        rotate(-135.0, 1),
        translate(5.6, 0.0, 0.0),
    ]);
    // Rotation point (-5, 2, 0) px, then z rotation 0.05 rad from `ModelBiped.setRotationAngles`.
    chain(&[
        place,
        translate(-5.0 / 16.0, 2.0 / 16.0, 0.0),
        rotate(0.05f32.to_degrees(), 2),
    ])
}

/// `RenderPlayer.renderRightArm` seen through `ItemRenderer`: the arm of the default skin.
pub fn arm(hand: &Hand, skin_height: u32) -> Draw {
    let model = arm_model(hand);
    let [[x0, x1], [y0, y1], [z0, z1]] = BOX;
    let (tw, th) = (64.0, skin_height.max(32) as f32);
    // Each face: normal, then four corners as (position, u, v) in skin pixels.
    type Corner = ([f32; 3], f32, f32);
    let face = |normal: [f32; 3], axes: [[f32; 2]; 2], f: &dyn Fn(f32, f32) -> Corner| {
        let [[a0, a1], [b0, b1]] = axes;
        (
            normal,
            [(a0, b0), (a1, b0), (a1, b1), (a0, b1)].map(|(a, b)| f(a, b)),
        )
    };
    let faces = [
        // front (z min), back (z max)
        face([0.0, 0.0, -1.0], [[x0, x1], [y0, y1]], &|x, y| {
            ([x, y, z0], 44.0 + (x - x0), 20.0 + (y - y0))
        }),
        face([0.0, 0.0, 1.0], [[x0, x1], [y0, y1]], &|x, y| {
            ([x, y, z1], 56.0 - (x - x0), 20.0 + (y - y0))
        }),
        // outer (x min), inner (x max)
        face([-1.0, 0.0, 0.0], [[z0, z1], [y0, y1]], &|z, y| {
            ([x0, y, z], 44.0 - (z - z0), 20.0 + (y - y0))
        }),
        face([1.0, 0.0, 0.0], [[z0, z1], [y0, y1]], &|z, y| {
            ([x1, y, z], 48.0 + (z - z0), 20.0 + (y - y0))
        }),
        // top (y min), bottom (y max)
        face([0.0, -1.0, 0.0], [[x0, x1], [z0, z1]], &|x, z| {
            ([x, y0, z], 44.0 + (x - x0), 20.0 - (z - z0))
        }),
        face([0.0, 1.0, 0.0], [[x0, x1], [z0, z1]], &|x, z| {
            ([x, y1, z], 48.0 + (x - x0), 16.0 + (z - z0))
        }),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let shade = item_light(&model, normal);
        let base = vertices.len() as u32;
        for (p, u, v) in corners {
            vertices.push(Vertex {
                pos: p.map(|c| c / 16.0),
                uv: [u / tw, v / th],
                color: [shade, shade, shade, 1.0],
                light: [1.0, 0.0],
            });
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    Draw {
        vertices,
        indices,
        model,
    }
}

/// `RenderHelper.enableStandardItemLighting`: two directional lights of 0.6 plus 0.4 ambient, lights fixed in eye space.
fn item_light(model: &Mat, normal: [f32; 3]) -> f32 {
    let n: [f32; 3] = std::array::from_fn(|r| (0..3).map(|c| model[c][r] * normal[c]).sum());
    let len = n.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let diffuse = |l: [f32; 3]| {
        let ll = l.iter().map(|v| v * v).sum::<f32>().sqrt();
        ((0..3).map(|i| n[i] * l[i]).sum::<f32>() / (len * ll)).max(0.0)
    };
    (0.4 + 0.6 * (diffuse([0.2, 1.0, -0.7]) + diffuse([-0.2, 1.0, 0.7]))).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no swing and the arm fully raised the hand sits at the lower right of the view, in front of the camera.
    #[test]
    fn resting_arm_is_lower_right_in_front_of_the_camera() {
        let d = arm(
            &Hand {
                held: None,
                swing: 0.0,
                lowered: 0.0,
                using: None,
                glint: false,
            },
            64,
        );
        let eye: Vec<[f32; 3]> = d
            .vertices
            .iter()
            .map(|v| {
                std::array::from_fn(|r| {
                    (0..3).map(|c| d.model[c][r] * v.pos[c]).sum::<f32>() + d.model[3][r]
                })
            })
            .collect();
        let n = eye.len() as f32;
        let mean: [f32; 3] = std::array::from_fn(|i| eye.iter().map(|p| p[i]).sum::<f32>() / n);
        assert!(mean[0] > 0.2 && mean[1] < 0.0 && mean[2] < -0.2, "{mean:?}");
    }

    /// One opaque pixel in a 2x2 sprite: front, back and its four edges.
    #[test]
    fn flat_sprite_extrudes_opaque_pixel_edges() {
        let mut rgba = vec![0u8; 16];
        rgba[3] = 255;
        let f = Flat::new(
            &Image {
                width: 2,
                height: 2,
                rgba,
            },
            ident(),
        );
        assert_eq!(f.normals.len(), 6);
        assert_eq!(f.mesh.vertices.len(), 24);
        assert_eq!(f.mesh.indices.len(), 36);
    }

    /// Eating drives the item toward the mouth: at the start of a use it matches the plain pose,
    /// near its end the translation and rotation of `applyConsuming` are in full effect.
    #[test]
    fn consuming_pose_starts_at_rest_and_moves_the_item_toward_the_mouth() {
        let at = |timer| {
            held_transform(&Hand {
                held: None,
                swing: 0.0,
                lowered: 0.0,
                using: Some(Using {
                    action: UseAction::Consume,
                    timer,
                }),
                glint: false,
            })
        };
        let rest = item_transform(0.0, 0.0);
        assert!((at(32.0)[3][1] - rest[3][1]).abs() < 0.1);
        assert!(at(8.0)[3][0] < rest[3][0] + 0.6 * 0.4 + 1e-3 && at(8.0)[3] != rest[3]);
    }
}
