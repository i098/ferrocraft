//! Paintings (`RenderPainting`) and item frames (`RenderItemFrame`) with their display item, and
//! the 128x128 colour map of a filled map (`MapRenderer`).
//!
//! shortcut: the edges and back of a painting sample one texel of the sheet instead of vanilla's
//! strips, and the map decorations (player and frame markers) are not drawn.

use super::item::{dir, point, ItemModel, Items, Models, Stack};
use super::{Class, EntityDraw, V3};
use crate::hand::{chain, ident, rotate, scale, translate, DialTurns, Mat};
use serde_json::Value;
use std::collections::HashMap;

pub const PAINTING_SHEET: &str = "textures/painting/paintings_kristoffer_zetterstrand.png";
pub const FILLED_MAP: i16 = 358;

/// What a hanging entity shows beyond its class: facing 0 south, 1 west, 2 north, 3 east, the
/// motive of a painting, and the rotation of a frame's item in eighths of a turn.
#[derive(Clone, Debug, Default)]
pub struct Hang {
    pub facing: u8,
    pub title: String,
    pub rotation: u8,
}

pub fn is_frame(d: &EntityDraw) -> bool {
    d.class == Class::Object(71)
}

/// The map texture key of a stack, `None` when it is not a filled map.
pub fn map_key(s: &Stack) -> Option<String> {
    (s.id == FILLED_MAP).then(|| format!("map/{}", s.damage))
}

struct FrameQuad {
    corners: [V3; 4],
    uv: [[f32; 2]; 4],
    tex: String,
    normal: V3,
}

/// The wooden frame models `block/item_frame` and `block/item_frame_map`, in block units centred
/// on the origin.
#[derive(Default)]
pub struct Frames {
    normal: Vec<FrameQuad>,
    map: Vec<FrameQuad>,
}

impl Frames {
    pub fn load(read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Frames {
        let mut model = |name: &str| {
            let json = read(&format!("models/block/{name}.json"))?;
            Some(quads(&serde_json::from_slice(&json).ok()?))
        };
        Frames {
            normal: model("item_frame").unwrap_or_default(),
            map: model("item_frame_map").unwrap_or_default(),
        }
    }
}

/// `FaceBakery`: the corners of face `dir` of the box `from..to`, wound counter-clockwise from
/// outside.
fn face_corners(dir: &str, f: V3, t: V3) -> Option<([V3; 4], V3)> {
    let (x1, y1, z1, x2, y2, z2) = (f[0], f[1], f[2], t[0], t[1], t[2]);
    Some(match dir {
        "down" => (
            [[x1, y1, z2], [x1, y1, z1], [x2, y1, z1], [x2, y1, z2]],
            [0.0, -1.0, 0.0],
        ),
        "up" => (
            [[x1, y2, z1], [x1, y2, z2], [x2, y2, z2], [x2, y2, z1]],
            [0.0, 1.0, 0.0],
        ),
        "north" => (
            [[x2, y2, z1], [x2, y1, z1], [x1, y1, z1], [x1, y2, z1]],
            [0.0, 0.0, -1.0],
        ),
        "south" => (
            [[x1, y2, z2], [x1, y1, z2], [x2, y1, z2], [x2, y2, z2]],
            [0.0, 0.0, 1.0],
        ),
        "west" => (
            [[x1, y2, z1], [x1, y1, z1], [x1, y1, z2], [x1, y2, z2]],
            [-1.0, 0.0, 0.0],
        ),
        "east" => (
            [[x2, y2, z2], [x2, y1, z2], [x2, y1, z1], [x2, y2, z1]],
            [1.0, 0.0, 0.0],
        ),
        _ => return None,
    })
}

fn quads(model: &Value) -> Vec<FrameQuad> {
    let floats = |v: &Value| -> Option<Vec<f32>> {
        v.as_array()?
            .iter()
            .map(|n| n.as_f64().map(|n| n as f32))
            .collect()
    };
    let mut out = Vec::new();
    for e in model["elements"].as_array().into_iter().flatten() {
        let (Some(f), Some(t)) = (floats(&e["from"]), floats(&e["to"])) else {
            continue;
        };
        for (dir, face) in e["faces"].as_object().into_iter().flatten() {
            let (Some(uv), Some(var)) = (floats(&face["uv"]), face["texture"].as_str()) else {
                continue;
            };
            let name = model["textures"][var.trim_start_matches('#')].as_str();
            let Some((corners, normal)) = face_corners(dir, [f[0], f[1], f[2]], [t[0], t[1], t[2]])
            else {
                continue;
            };
            let Some(name) = name else { continue };
            out.push(FrameQuad {
                corners: corners.map(|c| c.map(|v| v / 16.0 - 0.5)),
                uv: [
                    [uv[0], uv[1]],
                    [uv[0], uv[3]],
                    [uv[2], uv[3]],
                    [uv[2], uv[1]],
                ]
                .map(|p| p.map(|v| v / 16.0)),
                tex: format!("textures/{name}.png"),
                normal,
            });
        }
    }
    out
}

/// `MapColor.BY_ID` of 1.8.9: the base colour of each map colour id.
const MAP_COLORS: [u32; 36] = [
    0x000000, 0x7FB238, 0xF7E9A3, 0xC7C7C7, 0xFF0000, 0xA0A0FF, 0xA7A7A7, 0x007C00, 0xFFFFFF,
    0xA4A8B8, 0x976D4D, 0x707070, 0x4040FF, 0x8F7748, 0xFFFCF5, 0xD87F33, 0xB24CD8, 0x6699D8,
    0xE5E533, 0x7FCC19, 0xF27FA5, 0x4C4C4C, 0x999999, 0x4C7F99, 0x7F3FB2, 0x334CB2, 0x664C33,
    0x667F33, 0x993333, 0x191919, 0xFAEE4D, 0x5CDBD5, 0x4A80FF, 0x00D93A, 0x815631, 0x700200,
];

/// `MapRenderer.Texture.updateTexture`: the rgba of colour byte `c`. Colour 0 is
/// a faint checker in vanilla; it is left transparent here.
pub fn map_pixel(c: u8) -> [u8; 4] {
    let Some(&base) = MAP_COLORS.get(usize::from(c / 4)).filter(|_| c >= 4) else {
        return [0; 4];
    };
    // `MapColor.getColor`: shade 0 is 180/255, 1 is 220, 2 is 255, 3 is 135.
    let n = [180, 220, 255, 135][usize::from(c & 3)];
    let ch = |s: u32| ((base >> s & 0xFF) * n / 255) as u8;
    [ch(16), ch(8), ch(0), 255]
}

/// Models of the compasses in frames, by `(id, damage, needle turns)`: the needle depends on the
/// frame, not the player.
pub type Compasses = HashMap<(i16, i16, u64), Option<ItemModel>>;

/// The key of the compass model frame `d` shows, and the turns it is drawn with.
pub fn compass(d: &EntityDraw, dials: &DialTurns) -> Option<((i16, i16, u64), DialTurns)> {
    let s = d
        .stack
        .filter(|s| is_frame(d) && s.id == DialTurns::COMPASS)?;
    let turns = dials.framed([d.pos[0], d.pos[2]], d.hang.facing);
    Some(((s.id, s.damage, turns.compass.to_bits()), turns))
}

/// The stack a frame shows, whose model `draw` needs.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    d.stack
        .filter(|s| is_frame(d) && s.id != DialTurns::COMPASS)
        .map(|s| (s.id, s.damage))
        .into_iter()
}

/// Adds every painting and item frame of `draws` to `out`; `light` gives the `[sky, block]` level.
pub fn build(
    out: &mut Items,
    draws: &[EntityDraw],
    frames: &Frames,
    models: &Models,
    compasses: &Compasses,
    dials: &DialTurns,
    light: impl Fn([f64; 3]) -> [f32; 2],
) {
    for d in draws {
        draw(out, d, frames, models, compasses, dials, &light);
    }
}

/// Draws the painting or item frame `d`.
fn draw(
    out: &mut Items,
    d: &EntityDraw,
    frames: &Frames,
    models: &Models,
    compasses: &Compasses,
    dials: &DialTurns,
    light: &impl Fn([f64; 3]) -> [f32; 2],
) {
    let facing = d.hang.facing;
    // `rotatef(180 - yaw)`, the yaw of a hanging entity being its facing times 90.
    let turn = rotate(180.0 - f32::from(facing) * 90.0, 1);
    if d.class == Class::Painting {
        painting(out, d, &turn, light);
    } else if is_frame(d) {
        frame(out, d, &turn, frames, models, compasses, dials, light);
    }
}

/// `RenderPainting.renderPainting`, a cell of 16 pixels at a time with the light of the cell.
fn painting(out: &mut Items, d: &EntityDraw, turn: &Mat, light: &impl Fn([f64; 3]) -> [f32; 2]) {
    let Some(&(_, w, h, u, v)) = world::entities::PAINTINGS
        .iter()
        .find(|p| p.0 == d.hang.title)
    else {
        return;
    };
    let (w, h) = (f32::from(w), f32::from(h));
    let (u, v) = (f32::from(u), f32::from(v));
    let mat = chain(&[*turn, scale(1.0 / 16.0)]);
    let world = |p: V3| {
        let [x, y, z] = point(&mat, p);
        [
            (d.pos[0] + f64::from(x)) as f32,
            (d.pos[1] + f64::from(y)) as f32,
            (d.pos[2] + f64::from(z)) as f32,
        ]
    };
    let (f, f1) = (-w / 2.0, -h / 2.0);
    let edge = [[0.75, 0.0], [0.8125, 0.0], [0.8125, 0.0625], [0.75, 0.0625]].map(|p: [f32; 2]| p);
    let sheet = |p: [f32; 2]| p.map(|c| c / 256.0);
    for i in 0..(w as i32 / 16) {
        for j in 0..(h as i32 / 16) {
            let (f2, f3) = (f + (i + 1) as f32 * 16.0, f + i as f32 * 16.0);
            let (f4, f5) = (f1 + (j + 1) as f32 * 16.0, f1 + j as f32 * 16.0);
            let centre = world([(f2 + f3) / 2.0, (f4 + f5) / 2.0, 0.0]);
            let level = light(centre.map(f64::from));
            let (u6, u7) = (
                (u + w - i as f32 * 16.0) / 256.0,
                (u + w - (i + 1) as f32 * 16.0) / 256.0,
            );
            let (v8, v9) = (
                (v + h - j as f32 * 16.0) / 256.0,
                (v + h - (j + 1) as f32 * 16.0) / 256.0,
            );
            let normal = dir(&mat, [0.0, 0.0, -1.0]);
            let s = super::item::shade(normal);
            out.quad(
                PAINTING_SHEET,
                [
                    [f2, f5, -0.5],
                    [f3, f5, -0.5],
                    [f3, f4, -0.5],
                    [f2, f4, -0.5],
                ]
                .map(world),
                [[u7, v8], [u6, v8], [u6, v9], [u7, v9]],
                s,
                level,
            );
            let back = [[f2, f4, 0.5], [f3, f4, 0.5], [f3, f5, 0.5], [f2, f5, 0.5]];
            let sides = [
                [[f2, f4, -0.5], [f3, f4, -0.5], [f3, f4, 0.5], [f2, f4, 0.5]],
                [[f2, f5, 0.5], [f3, f5, 0.5], [f3, f5, -0.5], [f2, f5, -0.5]],
                [[f2, f4, 0.5], [f2, f5, 0.5], [f2, f5, -0.5], [f2, f4, -0.5]],
                [[f3, f4, -0.5], [f3, f5, -0.5], [f3, f5, 0.5], [f3, f4, 0.5]],
            ];
            for c in std::iter::once(back).chain(sides) {
                let corners = c.map(world);
                let uv = edge.map(sheet);
                // Double-sided: the edges are one pixel deep and wind either way.
                out.quad(PAINTING_SHEET, corners, uv, 0.8, level);
                out.quad(
                    PAINTING_SHEET,
                    [corners[3], corners[2], corners[1], corners[0]],
                    [uv[3], uv[2], uv[1], uv[0]],
                    0.8,
                    level,
                );
            }
        }
    }
}

/// `RenderItemFrame.render` and `renderDisplayItem`.
fn frame(
    out: &mut Items,
    d: &EntityDraw,
    turn: &Mat,
    frames: &Frames,
    models: &Models,
    compasses: &Compasses,
    dials: &DialTurns,
    light: &impl Fn([f64; 3]) -> [f32; 2],
) {
    // The frame is drawn at the centre of the block its entity stands in.
    let origin = d.pos.map(|c| c.floor() + 0.5);
    let level = light(origin);
    let world = |m: &Mat, p: V3| {
        let [x, y, z] = point(m, p);
        [
            (origin[0] + f64::from(x)) as f32,
            (origin[1] + f64::from(y)) as f32,
            (origin[2] + f64::from(z)) as f32,
        ]
    };
    let stack = d.stack;
    let map = stack.as_ref().and_then(map_key);
    let wood = if map.is_some() {
        &frames.map
    } else {
        &frames.normal
    };
    for q in wood {
        out.quad(
            &q.tex,
            q.corners.map(|c| world(turn, c)),
            q.uv,
            super::item::shade(dir(turn, q.normal)),
            level,
        );
    }
    let Some(stack) = stack else { return };
    // `translatef(0, 0, 0.4375)` into the frame, then the item turns about z.
    let rot = f32::from(d.hang.rotation);
    let base = chain(&[*turn, translate(0.0, 0.0, 0.4375)]);
    if let Some(key) = map {
        // Rotation of a map is in quarter turns; the quad is 128 map pixels wide.
        let turn_z = rotate(rot % 4.0 * 2.0 * 360.0 / 8.0, 2);
        let mat = chain(&[
            base,
            turn_z,
            rotate(180.0, 2),
            scale(0.007_812_5),
            translate(-64.0, -64.0, -1.0),
        ]);
        let z = -0.01;
        let corners = [
            [0.0, 128.0, z],
            [128.0, 128.0, z],
            [128.0, 0.0, z],
            [0.0, 0.0, z],
        ];
        out.quad(
            &key,
            corners.map(|c| world(&mat, c)),
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            1.0,
            [1.0, 1.0],
        );
        return;
    }
    let model = match compass(d, dials) {
        Some((key, _)) => compasses.get(&key),
        None => models.get(&(stack.id, stack.damage)),
    };
    let Some(Some(m)) = model else {
        return;
    };
    let prepare = if m.gui3d() { ident() } else { rotate(180.0, 1) };
    let mat = chain(&[
        base,
        rotate(rot * 360.0 / 8.0, 2),
        scale(0.5),
        prepare,
        m.fixed(),
        scale(0.5),
    ]);
    out.piece(
        m,
        &|p| world(&mat, p),
        &|n| dir(&mat, n),
        level,
        true,
        stack.glint.then_some(d.age * 50.0),
    );
}
