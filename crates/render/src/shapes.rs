//! Box models of the blocks that are not full cubes: slabs, stairs, fences, panes, doors, torches and more.
//! Coordinates are block-local in `0..=1`; sources are written in 1/16 block units like vanilla JSON models.

use crate::blocks::{look, Kind, Look, Tint, DOWN, EAST, NORTH, SOUTH, UP, WEST};

pub const ALL: u8 = 0b11_1111;
pub const fn bit(face: usize) -> u8 {
    1 << face
}

#[derive(Clone, Debug)]
pub struct Bx {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub tex: [&'static str; 6],
    pub tint: Tint,
    /// Faces to draw (bit per face index).
    pub mask: u8,
    /// Quarter turns applied to the up-face texture (rails).
    pub rot: u8,
    /// Drop faces that lie on the block border when the neighbour is a full cube.
    pub cull: bool,
}

fn px(min: [f32; 3], max: [f32; 3], tex: [&'static str; 6]) -> Bx {
    let s = |v: [f32; 3]| v.map(|c| c / 16.0);
    Bx {
        min: s(min),
        max: s(max),
        tex,
        tint: Tint::None,
        mask: ALL,
        rot: 0,
        cull: true,
    }
}

fn is_fence(id: u16) -> bool {
    matches!(id, 85 | 113 | 188..=192)
}

fn is_gate(id: u16) -> bool {
    matches!(id, 107 | 183..=187)
}

fn is_pane(id: u16) -> bool {
    matches!(id, 101 | 102 | 160)
}

fn full(state: u16) -> bool {
    look(state).kind == Kind::Opaque
}

/// `nb(dx, dy, dz)` returns the state around the block.
pub fn boxes(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> Vec<Bx> {
    let l = look(state);
    let mut out = Vec::new();
    boxes_0(state, nb, &l, &mut out);
    out
}

fn boxes_0(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        // Nether portal (`portal_ns`/`portal_ew`): axis Z when meta & 3 == 2, else X; 4 px thick, broad faces only.
        90 => {
            let mut b = if meta & 3 == 2 {
                px([6.0, 0.0, 0.0], [10.0, 16.0, 16.0], t)
            } else {
                px([0.0, 0.0, 6.0], [16.0, 16.0, 10.0], t)
            };
            b.mask = if meta & 3 == 2 {
                bit(EAST) | bit(WEST)
            } else {
                bit(NORTH) | bit(SOUTH)
            };
            out.push(b);
        }
        44 | 126 | 182 => {
            let (lo, hi) = if meta & 8 != 0 {
                (8.0, 16.0)
            } else {
                (0.0, 8.0)
            };
            out.push(px([0.0, lo, 0.0], [16.0, hi, 16.0], t));
        }
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => {
            let top = meta & 4 != 0;
            let (slab, step) = if top {
                ((8.0, 16.0), (0.0, 8.0))
            } else {
                ((0.0, 8.0), (8.0, 16.0))
            };
            out.push(px([0.0, slab.0, 0.0], [16.0, slab.1, 16.0], t));
            let (x, z) = match meta & 3 {
                0 => ((8.0, 16.0), (0.0, 16.0)),
                1 => ((0.0, 8.0), (0.0, 16.0)),
                2 => ((0.0, 16.0), (8.0, 16.0)),
                _ => ((0.0, 16.0), (0.0, 8.0)),
            };
            out.push(px([x.0, step.0, z.0], [x.1, step.1, z.1], t));
        }
        117 | 118 | 122 | 145 | 154 => decor(state, out),
        _ => boxes_1(state, nb, l, out),
    }
}

/// Turn a box `quarters` clockwise (seen from above) around the block centre, as a blockstate `"y"` rotation does.
fn turn(mut b: Bx, quarters: u8) -> Bx {
    for _ in 0..quarters {
        let (lo, hi) = (b.min, b.max);
        b.min = [1.0 - hi[2], lo[1], lo[0]];
        b.max = [1.0 - lo[2], hi[1], hi[0]];
        // North goes to east, east to south, south to west, west to north.
        let side = [NORTH, EAST, SOUTH, WEST];
        let (tex, mask) = (b.tex, b.mask);
        for i in 0..4 {
            let (from, to) = (side[i], side[(i + 1) % 4]);
            b.tex[to] = tex[from];
            b.mask = b.mask & !bit(to) | (mask >> from & 1) << to;
        }
        b.rot = (b.rot + 1) % 4;
    }
    b
}

/// Water surface height of a cauldron by level, in px (`cauldron_level1..3` models).
pub const CAULDRON_WATER: [f32; 4] = [0.0, 9.0, 12.0, 15.0];

/// The three spokes of a brewing stand beside its box parts: `(tip x, tip z, bottle index)` in 1/16 block.
/// Each spoke is a double-sided plane from the block centre to its tip; bottle `i` shows on spoke `i`.
pub const BREWING_SPOKES: [([f32; 2], usize); 3] =
    [([16.0, 8.0], 0), ([0.0, 0.0], 1), ([0.0, 16.0], 2)];

/// Models of the hopper, cauldron, anvil, brewing stand and dragon egg (1.8.9 `models/block/*.json`).
fn decor(state: u16, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    // Face order: up, down, north, south, west, east.
    match id {
        154 => {
            let (side, top, inside) = ("hopper_outside", "hopper_top", "hopper_inside");
            let facing = meta & 7;
            out.push(px(
                [0.0, 10.0, 0.0],
                [16.0, 11.0, 16.0],
                [inside, side, side, side, side, side],
            ));
            for (min, max) in [
                ([0.0, 11.0, 0.0], [2.0, 16.0, 16.0]),
                ([14.0, 11.0, 0.0], [16.0, 16.0, 16.0]),
                ([2.0, 11.0, 0.0], [14.0, 16.0, 2.0]),
                ([2.0, 11.0, 14.0], [14.0, 16.0, 16.0]),
            ] {
                out.push(px(min, max, [top, side, side, side, side, side]));
            }
            out.push(px([4.0, 4.0, 4.0], [12.0, 10.0, 12.0], [side; 6]));
            if matches!(facing, 2..=5) {
                let spout = px([6.0, 4.0, 0.0], [10.0, 8.0, 4.0], [side; 6]);
                out.push(turn(spout, [0, 0, 0, 2, 3, 1][facing as usize]));
            } else {
                out.push(px([6.0, 0.0, 6.0], [10.0, 4.0, 10.0], [side; 6]));
            }
        }
        118 => {
            let (top, bottom, side, inside) = (
                "cauldron_top",
                "cauldron_bottom",
                "cauldron_side",
                "cauldron_inner",
            );
            for (min, max) in [
                ([0.0, 3.0, 0.0], [2.0, 16.0, 16.0]),
                ([14.0, 3.0, 0.0], [16.0, 16.0, 16.0]),
                ([2.0, 3.0, 0.0], [14.0, 16.0, 2.0]),
                ([2.0, 3.0, 14.0], [14.0, 16.0, 16.0]),
            ] {
                out.push(px(min, max, [top, inside, side, side, side, side]));
            }
            out.push(px(
                [2.0, 3.0, 2.0],
                [14.0, 4.0, 14.0],
                [inside, inside, side, side, side, side],
            ));
            // Four legs, each two boxes that wrap a corner.
            for (min, max) in [
                ([0.0, 0.0, 0.0], [4.0, 3.0, 2.0]),
                ([0.0, 0.0, 2.0], [2.0, 3.0, 4.0]),
                ([12.0, 0.0, 0.0], [16.0, 3.0, 2.0]),
                ([14.0, 0.0, 2.0], [16.0, 3.0, 4.0]),
                ([0.0, 0.0, 14.0], [4.0, 3.0, 16.0]),
                ([0.0, 0.0, 12.0], [2.0, 3.0, 14.0]),
                ([12.0, 0.0, 14.0], [16.0, 3.0, 16.0]),
                ([14.0, 0.0, 12.0], [16.0, 3.0, 14.0]),
            ] {
                out.push(px(min, max, [top, bottom, side, side, side, side]));
            }
            let h = CAULDRON_WATER[(meta & 3) as usize];
            if h > 0.0 {
                let mut w = px([2.0, h, 2.0], [14.0, h, 14.0], ["water_still"; 6]);
                w.mask = bit(UP);
                w.cull = false;
                out.push(w);
            }
        }
        145 => {
            let top = [
                "anvil_top_damaged_0",
                "anvil_top_damaged_1",
                "anvil_top_damaged_2",
            ][(meta >> 2).min(2) as usize];
            let body = "anvil_base";
            let parts = [
                px([2.0, 0.0, 2.0], [14.0, 4.0, 14.0], [body; 6]),
                px([4.0, 4.0, 3.0], [12.0, 5.0, 13.0], [body; 6]),
                px([6.0, 5.0, 4.0], [10.0, 10.0, 12.0], [body; 6]),
                px(
                    [3.0, 10.0, 0.0],
                    [13.0, 16.0, 16.0],
                    [top, body, body, body, body, body],
                ),
            ];
            // `facing`: 0 south, 1 west, 2 north, 3 east, turned 0, 90, 180, 270 degrees.
            out.extend(parts.map(|b| turn(b, (meta & 3) as u8)));
        }
        117 => {
            let (stand, base) = ("brewing_stand", "brewing_stand_base");
            out.push(px([7.0, 0.0, 7.0], [9.0, 14.0, 9.0], [stand; 6]));
            for (min, max) in [
                ([9.0, 0.0, 5.0], [15.0, 2.0, 11.0]),
                ([2.0, 0.0, 1.0], [8.0, 2.0, 7.0]),
                ([2.0, 0.0, 9.0], [8.0, 2.0, 15.0]),
            ] {
                out.push(px(min, max, [base; 6]));
            }
        }
        _ => {
            // Dragon egg: nine stacked boxes.
            for (min, max) in [
                ([6.0, 15.0, 6.0], [10.0, 16.0, 10.0]),
                ([5.0, 14.0, 5.0], [11.0, 15.0, 11.0]),
                ([5.0, 13.0, 5.0], [11.0, 14.0, 11.0]),
                ([3.0, 11.0, 3.0], [13.0, 13.0, 13.0]),
                ([2.0, 8.0, 2.0], [14.0, 11.0, 14.0]),
                ([1.0, 3.0, 1.0], [15.0, 8.0, 15.0]),
                ([2.0, 1.0, 2.0], [14.0, 3.0, 14.0]),
                ([3.0, 0.0, 3.0], [13.0, 1.0, 13.0]),
            ] {
                out.push(px(min, max, ["dragon_egg"; 6]));
            }
        }
    }
}

fn boxes_1(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let id = state >> 4;
    let t = l.tex;
    match id {
        85 | 113 | 188..=192 => {
            out.push(px([6.0, 0.0, 6.0], [10.0, 16.0, 10.0], t));
            let joins = |dx, dz| {
                let n = nb(dx, 0, dz);
                is_fence(n >> 4) || is_gate(n >> 4) || full(n)
            };
            for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                if !joins(dx, dz) {
                    continue;
                }
                for (y0, y1) in [(6.0, 9.0), (12.0, 15.0)] {
                    out.push(match (dx, dz) {
                        (0, -1) => px([7.0, y0, 0.0], [9.0, y1, 6.0], t),
                        (0, _) => px([7.0, y0, 10.0], [9.0, y1, 16.0], t),
                        (-1, _) => px([0.0, y0, 7.0], [6.0, y1, 9.0], t),
                        _ => px([10.0, y0, 7.0], [16.0, y1, 9.0], t),
                    });
                }
            }
        }
        _ => boxes_2(state, nb, l, out),
    }
}

fn boxes_2(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        107 | 183..=187 => {
            let along_x = meta & 1 == 0;
            let at = |a: f32, b: f32, y0: f32, y1: f32, w0: f32, w1: f32| {
                if along_x {
                    px([a, y0, w0], [b, y1, w1], t)
                } else {
                    px([w0, y0, a], [w1, y1, b], t)
                }
            };
            out.push(at(0.0, 2.0, 5.0, 16.0, 7.0, 9.0));
            out.push(at(14.0, 16.0, 5.0, 16.0, 7.0, 9.0));
            if meta & 4 == 0 {
                out.push(at(6.0, 10.0, 6.0, 15.0, 7.0, 9.0));
                out.push(at(2.0, 6.0, 6.0, 9.0, 7.0, 9.0));
                out.push(at(10.0, 14.0, 6.0, 9.0, 7.0, 9.0));
                out.push(at(2.0, 6.0, 12.0, 15.0, 7.0, 9.0));
                out.push(at(10.0, 14.0, 12.0, 15.0, 7.0, 9.0));
            }
        }
        _ => boxes_3(state, nb, l, out),
    }
}

fn boxes_3(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let id = state >> 4;
    let t = l.tex;
    match id {
        139 => {
            out.push(px([4.0, 0.0, 4.0], [12.0, 16.0, 12.0], t));
            for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                let n = nb(dx, 0, dz);
                if !(n >> 4 == 139 || is_gate(n >> 4) || full(n)) {
                    continue;
                }
                out.push(match (dx, dz) {
                    (0, -1) => px([5.0, 0.0, 0.0], [11.0, 13.0, 8.0], t),
                    (0, _) => px([5.0, 0.0, 8.0], [11.0, 13.0, 16.0], t),
                    (-1, _) => px([0.0, 0.0, 5.0], [8.0, 13.0, 11.0], t),
                    _ => px([8.0, 0.0, 5.0], [16.0, 13.0, 11.0], t),
                });
            }
        }
        _ => boxes_4(state, nb, l, out),
    }
}

fn boxes_4(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        101 | 102 | 160 => {
            let top = if id == 101 {
                "iron_bars"
            } else if id == 102 {
                "glass_pane_top"
            } else {
                crate::blocks::pane_top(meta)
            };
            let mut post = px([7.0, 0.0, 7.0], [9.0, 16.0, 9.0], t);
            post.tex[UP] = top;
            post.tex[DOWN] = top;
            let joins = |dx, dz| {
                let n = nb(dx, 0, dz);
                is_pane(n >> 4) || full(n)
            };
            let sides = [(0, -1), (0, 1), (-1, 0), (1, 0)].map(|(dx, dz)| joins(dx, dz));
            let any = sides.iter().any(|s| *s);
            if !any {
                // Isolated pane: both axes, as vanilla.
                out.push(px([7.0, 0.0, 0.0], [9.0, 16.0, 16.0], t));
                out.push(px([0.0, 0.0, 7.0], [16.0, 16.0, 9.0], t));
                out.last_mut().unwrap().tex[UP] = top;
                out.last_mut().unwrap().tex[DOWN] = top;
                out[0].tex[UP] = top;
                out[0].tex[DOWN] = top;
            } else {
                out.push(post);
                let arms = [
                    ([7.0, 0.0, 0.0], [9.0, 16.0, 7.0]),
                    ([7.0, 0.0, 9.0], [9.0, 16.0, 16.0]),
                    ([0.0, 0.0, 7.0], [7.0, 16.0, 9.0]),
                    ([9.0, 0.0, 7.0], [16.0, 16.0, 9.0]),
                ];
                for (a, b) in arms
                    .into_iter()
                    .zip(sides)
                    .filter_map(|(ab, s)| s.then_some(ab))
                {
                    let mut b = px(a, b, t);
                    b.tex[UP] = top;
                    b.tex[DOWN] = top;
                    out.push(b);
                }
            }
        }
        _ => boxes_5(state, nb, l, out),
    }
}

fn boxes_5(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        50 | 75 | 76 => {
            let (dx, dy) = match meta {
                1 => (-6.0, 3.0),
                2 => (6.0, 3.0),
                5 | 0 => (0.0, 0.0),
                _ => (0.0, 3.0),
            };
            let dz = match meta {
                3 => -6.0,
                4 => 6.0,
                _ => 0.0,
            };
            let mut b = px([7.0 + dx, dy, 7.0 + dz], [9.0 + dx, 10.0 + dy, 9.0 + dz], t);
            b.cull = false;
            b.mask = ALL & !bit(DOWN);
            out.push(b);
        }
        26 => out.push(px([0.0, 3.0, 0.0], [16.0, 9.0, 16.0], t)),
        _ => boxes_6(state, nb, l, out),
    }
}

fn boxes_6(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        27 | 28 | 157 | 66 => {
            let mut b = px([0.0, 0.0, 0.0], [16.0, 1.0, 16.0], t);
            b.mask = bit(UP);
            // Only powered, detector and activator rails carry the powered bit 8.
            let shape = if id == 66 { meta } else { meta & 7 };
            b.rot = match (id, shape) {
                (66, m @ 6..=9) => (m - 6) as u8,
                (_, 1..=3) => 1,
                _ => 0,
            };
            out.push(b);
        }
        55 => {
            let mut b = px([0.0, 0.0, 0.0], [16.0, 0.25, 16.0], t);
            b.tint = l.tint;
            b.mask = bit(UP);
            out.push(b);
        }
        54 | 146 | 130 => out.push(px([1.0, 0.0, 1.0], [15.0, 14.0, 15.0], t)),
        60 => out.push(px([0.0, 0.0, 0.0], [16.0, 15.0, 16.0], t)),
        64 | 71 | 193..=197 => door(state, nb, t, out),
        _ => boxes_7(state, nb, l, out),
    }
}

fn boxes_7(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        65 => {
            let (a, b) = match meta {
                2 => ([0.0, 0.0, 14.0], [16.0, 16.0, 16.0]),
                3 => ([0.0, 0.0, 0.0], [16.0, 16.0, 2.0]),
                4 => ([14.0, 0.0, 0.0], [16.0, 16.0, 16.0]),
                _ => ([0.0, 0.0, 0.0], [2.0, 16.0, 16.0]),
            };
            let mut bx = px(a, b, t);
            bx.cull = false;
            out.push(bx);
        }
        70 | 72 => out.push(px([1.0, 0.0, 1.0], [15.0, 1.0, 15.0], t)),
        78 => out.push(px(
            [0.0, 0.0, 0.0],
            [16.0, ((meta & 7) as f32 + 1.0) * 2.0, 16.0],
            t,
        )),
        81 => {
            let mut a = px([0.0, 0.0, 1.0], [16.0, 16.0, 15.0], t);
            a.mask = bit(WEST) | bit(EAST);
            let mut b = px([1.0, 0.0, 0.0], [15.0, 16.0, 16.0], t);
            b.mask = bit(NORTH) | bit(SOUTH);
            let mut c = px([1.0, 0.0, 1.0], [15.0, 16.0, 15.0], t);
            c.mask = bit(UP) | bit(DOWN);
            for bx in [&mut a, &mut b, &mut c] {
                bx.cull = false;
            }
            out.extend([a, b, c]);
        }
        92 => out.push(px(
            [1.0 + 2.0 * (meta.min(6) as f32), 0.0, 1.0],
            [15.0, 8.0, 15.0],
            t,
        )),
        _ => boxes_8(state, nb, l, out),
    }
}

fn boxes_8(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        93 | 94 => out.push(px([0.0, 0.0, 0.0], [16.0, 2.0, 16.0], t)),
        96 | 167 => {
            let (a, b) = if meta & 4 != 0 {
                match meta & 3 {
                    0 => ([0.0, 0.0, 13.0], [16.0, 16.0, 16.0]),
                    1 => ([0.0, 0.0, 0.0], [16.0, 16.0, 3.0]),
                    2 => ([13.0, 0.0, 0.0], [16.0, 16.0, 16.0]),
                    _ => ([0.0, 0.0, 0.0], [3.0, 16.0, 16.0]),
                }
            } else if meta & 8 != 0 {
                ([0.0, 13.0, 0.0], [16.0, 16.0, 16.0])
            } else {
                ([0.0, 0.0, 0.0], [16.0, 3.0, 16.0])
            };
            let mut bx = px(a, b, t);
            bx.cull = false;
            out.push(bx);
        }
        _ => boxes_9(state, nb, l, out),
    }
}

fn boxes_9(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        106 => {
            for (bit_mask, a, b) in [
                (1, [0.0, 0.0, 15.2], [16.0, 16.0, 16.0]),
                (2, [0.0, 0.0, 0.0], [0.8, 16.0, 16.0]),
                (4, [0.0, 0.0, 0.0], [16.0, 16.0, 0.8]),
                (8, [15.2, 0.0, 0.0], [16.0, 16.0, 16.0]),
            ] {
                if meta & bit_mask == 0 {
                    continue;
                }
                let mut bx = px(a, b, t);
                bx.tint = l.tint;
                bx.cull = false;
                bx.mask = match bit_mask {
                    1 | 4 => bit(NORTH) | bit(SOUTH),
                    _ => bit(WEST) | bit(EAST),
                };
                out.push(bx);
            }
        }
        111 => {
            let mut bx = px([0.0, 0.0, 0.0], [16.0, 0.25, 16.0], t);
            bx.tint = l.tint;
            bx.mask = bit(UP);
            out.push(bx);
        }
        116 => out.push(px([0.0, 0.0, 0.0], [16.0, 12.0, 16.0], t)),
        120 => out.push(px([0.0, 0.0, 0.0], [16.0, 13.0, 16.0], t)),
        140 => out.push(px([5.0, 0.0, 5.0], [11.0, 6.0, 11.0], t)),
        151 => out.push(px([0.0, 0.0, 0.0], [16.0, 6.0, 16.0], t)),
        _ => boxes_10(state, nb, l, out),
    }
}

fn boxes_10(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, l: &Look, out: &mut Vec<Bx>) {
    let (id, meta) = (state >> 4, state & 15);
    let t = l.tex;
    match id {
        171 => out.push(px([0.0, 0.0, 0.0], [16.0, 1.0, 16.0], t)),
        34 => piston_head(meta, out),
        69 => lever(meta, out),
        77 | 143 => button(meta, t[0], out),
        127 => cocoa(meta, out),
        131 => hook(meta, out),
        132 => wire(meta, nb, out),
        147 | 148 => {
            // `pressure_plate_up`/`_down`: 14x14 footprint, 1/16 high, half that when pressed.
            let h = if meta > 0 { 0.5 } else { 1.0 };
            out.push(px([1.0, 0.0, 1.0], [15.0, h, 15.0], t));
        }
        149 | 150 => comparator(id == 150 || meta & 8 != 0, meta, out),
        _ => {}
    }
}

/// Texture on every face of a piece, drawing only the faces in `mask`.
fn part(min: [f32; 3], max: [f32; 3], tex: &'static str, mask: u8) -> Bx {
    Bx {
        mask,
        ..px(min, max, [tex; 6])
    }
}

/// `part` with a zero-height box: a flat sheet drawn from above and below.
fn sheet(min: [f32; 3], max: [f32; 3], tex: &'static str) -> Bx {
    Bx {
        cull: false,
        ..part(min, max, tex, bit(UP) | bit(DOWN))
    }
}

const NORMALS: [[f32; 3]; 6] = [
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, -1.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
];

/// Blockstate rotation `x` then `y` in quarter turns, as vanilla applies it to a model.
fn quarter(mut v: [f32; 3], x: u8, y: u8) -> [f32; 3] {
    for _ in 0..x {
        v = [v[0], v[2], -v[1]];
    }
    for _ in 0..y {
        v = [-v[2], v[1], v[0]];
    }
    v
}

/// Turns a box about the block centre; its face textures and mask follow the faces.
fn turned(b: Bx, x: u8, y: u8) -> Bx {
    let at = |p: [f32; 3]| quarter(p.map(|c| c - 0.5), x, y).map(|c| c + 0.5);
    let (p, q) = (at(b.min), at(b.max));
    let mut out = Bx {
        min: [0, 1, 2].map(|a| p[a].min(q[a])),
        max: [0, 1, 2].map(|a| p[a].max(q[a])),
        mask: 0,
        ..b.clone()
    };
    for f in 0..6 {
        let to = NORMALS.iter().position(|n| *n == quarter(NORMALS[f], x, y));
        let to = to.expect("a quarter turn maps a face normal to a face normal");
        out.tex[to] = b.tex[f];
        out.mask |= (b.mask >> f & 1) << to;
    }
    out
}

fn push_turned(parts: Vec<Bx>, x: u8, y: u8, out: &mut Vec<Bx>) {
    out.extend(parts.into_iter().map(|b| turned(b, x, y)));
}

/// `lever`/`lever_off`: a cobblestone base and a handle that leans 45 degrees along z. A box cannot
/// lean, so the handle is two offset boxes along the lean. Vanilla rotation (x, y) per orientation.
fn lever(meta: u16, out: &mut Vec<Bx>) {
    let (x, y) = [
        (2, 1),
        (1, 1),
        (1, 3),
        (1, 2),
        (1, 0),
        (0, 0),
        (0, 3),
        (2, 2),
    ][meta as usize & 7];
    let lean = if meta & 8 != 0 { -1.0 } else { 1.0 };
    let handle = |y0: f32, y1: f32, z: f32| {
        part(
            [7.0, y0, 7.0 + lean * z],
            [9.0, y1, 9.0 + lean * z],
            "lever",
            ALL,
        )
    };
    let parts = vec![
        part([5.0, 0.0, 4.0], [11.0, 3.0, 12.0], "cobblestone", ALL),
        handle(1.0, 4.5, 1.75),
        handle(4.5, 8.0, 5.25),
    ];
    push_turned(parts, x, y, out);
}

/// `button`/`button_pressed`: a 6x4 pad, 2/16 high, 1/16 when pressed.
fn button(meta: u16, tex: &'static str, out: &mut Vec<Bx>) {
    let h = if meta & 8 != 0 { 1.0 } else { 2.0 };
    let (x, y) = [(2, 0), (1, 1), (1, 3), (1, 2), (1, 0), (0, 0)][(meta as usize & 7).min(5)];
    push_turned(
        vec![part([5.0, 0.0, 6.0], [11.0, h, 10.0], tex, ALL)],
        x,
        y,
        out,
    );
}

/// `cocoa_age{0,1,2}_s`: the pod and the stem sheet, turned to the facing in `meta & 3`
/// (0 south, 1 west, 2 north, 3 east).
fn cocoa(meta: u16, out: &mut Vec<Bx>) {
    let age = (meta >> 2 & 3).min(2) as usize;
    let tex = ["cocoa_stage_0", "cocoa_stage_1", "cocoa_stage_2"][age];
    let (min, max) = [
        ([6.0, 7.0, 11.0], [10.0, 12.0, 15.0]),
        ([5.0, 5.0, 9.0], [11.0, 12.0, 15.0]),
        ([4.0, 3.0, 7.0], [12.0, 12.0, 15.0]),
    ][age];
    let stem = Bx {
        cull: false,
        ..part(
            [8.0, 12.0, 12.0],
            [8.0, 16.0, 16.0],
            tex,
            bit(WEST) | bit(EAST),
        )
    };
    push_turned(
        vec![part(min, max, tex, ALL), stem],
        0,
        (meta & 3) as u8,
        out,
    );
}

/// `tripwire_hook*`: wall plate, arm and hook. Bit 4 attached, bit 8 powered; facing in `meta & 3`.
/// The unattached hook and the attached arm lean in vanilla; here they stay upright.
fn hook(meta: u16, out: &mut Vec<Bx>) {
    let (attached, powered) = (meta & 4 != 0, meta & 8 != 0);
    let mut parts = vec![
        part([6.0, 1.0, 14.0], [10.0, 9.0, 16.0], "planks_oak", ALL),
        part([7.4, 5.2, 10.0], [8.8, 6.8, 14.0], "planks_oak", ALL),
    ];
    parts.push(if attached || powered {
        part([6.2, 4.2, 6.7], [9.8, 5.0, 10.3], "trip_wire_source", ALL)
    } else {
        part([6.2, 3.8, 7.9], [9.8, 4.6, 11.5], "trip_wire_source", ALL)
    });
    if attached {
        let y = if powered { 0.5 } else { 1.5 };
        parts.push(sheet([7.75, y, 0.0], [8.25, y, 6.7], "trip_wire"));
    }
    push_turned(parts, 0, ((meta & 3) as u8 + 2) % 4, out);
}

/// `tripwire_*`: a 1/2 px wide string on every side that joins a wire or a hook facing it, else
/// north-south. Bit 2 is the suspended string, 3.5 px up instead of 1.5.
fn wire(meta: u16, nb: &dyn Fn(i32, i32, i32) -> u16, out: &mut Vec<Bx>) {
    // North, east, south, west, with the `EnumFacing.getHorizontal` index of each (south 0, west 1,
    // north 2, east 3): a hook joins when it faces back at the wire.
    const SIDES: [(i32, i32, u16); 4] = [(0, -1, 2), (1, 0, 3), (0, 1, 0), (-1, 0, 1)];
    let joined: [bool; 4] = std::array::from_fn(|i| {
        let (dx, dz, facing) = SIDES[i];
        let n = nb(dx, 0, dz);
        n >> 4 == 132 || (n >> 4 == 131 && n & 3 == (facing + 2) % 4)
    });
    let y = if meta & 2 != 0 { 3.5 } else { 1.5 };
    // A string from the centre `from` px out to `to` px along `side`.
    let arm = |side: usize, from: f32, to: f32| {
        let (lo, hi) = (8.0 - to, 8.0 - from);
        let (near, far) = (8.0 + from, 8.0 + to);
        match side {
            0 => sheet([7.75, y, lo], [8.25, y, hi], "trip_wire"),
            1 => sheet([near, y, 7.75], [far, y, 8.25], "trip_wire"),
            2 => sheet([7.75, y, near], [8.25, y, far], "trip_wire"),
            _ => sheet([lo, y, 7.75], [hi, y, 8.25], "trip_wire"),
        }
    };
    let count = joined.iter().filter(|j| **j).count();
    for (side, &on) in joined.iter().enumerate() {
        if on || (count == 0 && side % 2 == 0) {
            out.push(arm(side, 0.0, 8.0));
        }
    }
    if let (1, Some(side)) = (count, joined.iter().position(|j| *j)) {
        // A lone string runs 4 px past the centre.
        out.push(arm((side + 2) % 4, 0.0, 4.0));
    }
}

/// `comparator_*`: slab, two rear torches and a front torch, turned to the facing in `meta & 3`
/// (0 south, 1 west, 2 north, 3 east). A lit torch is drawn as its crossed lit boxes.
fn comparator(powered: bool, meta: u16, out: &mut Vec<Bx>) {
    let top = if powered {
        "comparator_on"
    } else {
        "comparator_off"
    };
    let mut slab = px([0.0, 0.0, 0.0], [16.0, 2.0, 16.0], ["stone_slab_top"; 6]);
    slab.tex[UP] = top;
    let mut parts = vec![slab];
    let side = bit(WEST) | bit(EAST);
    let front = bit(NORTH) | bit(SOUTH);
    for x in [4.0, 10.0] {
        if powered {
            parts.push(part(
                [x, 7.0, 11.0],
                [x + 2.0, 7.0, 13.0],
                "redstone_torch_on",
                bit(UP),
            ));
            parts.push(part(
                [x, 2.0, 10.0],
                [x + 2.0, 8.0, 14.0],
                "redstone_torch_on",
                side,
            ));
            parts.push(part(
                [x - 1.0, 2.0, 11.0],
                [x + 3.0, 8.0, 13.0],
                "redstone_torch_on",
                front,
            ));
        } else {
            parts.push(part(
                [x, 2.0, 11.0],
                [x + 2.0, 7.0, 13.0],
                "redstone_torch_off",
                ALL,
            ));
        }
    }
    if meta & 4 != 0 {
        parts.push(part(
            [7.0, 5.0, 2.0],
            [9.0, 5.0, 4.0],
            "redstone_torch_on",
            bit(UP),
        ));
        parts.push(part(
            [7.0, 2.0, 1.0],
            [9.0, 6.0, 5.0],
            "redstone_torch_on",
            side,
        ));
        parts.push(part(
            [6.0, 2.0, 2.0],
            [10.0, 6.0, 4.0],
            "redstone_torch_on",
            front,
        ));
    } else {
        parts.push(part(
            [7.0, 2.0, 2.0],
            [9.0, 4.0, 4.0],
            "redstone_torch_off",
            ALL,
        ));
    }
    push_turned(parts, 0, (meta & 3) as u8, out);
}

/// `piston_head_{normal,sticky}`: platform and shaft facing `meta & 7` (down, up, north, south,
/// west, east); bit 8 is the sticky head. The 4 px of shaft past the cell edge belong to the base.
fn piston_head(meta: u16, out: &mut Vec<Bx>) {
    let (x, y) = [(1, 0), (3, 0), (0, 0), (0, 2), (0, 3), (0, 1)][(meta as usize & 7).min(5)];
    let front = if meta & 8 != 0 {
        "piston_top_sticky"
    } else {
        "piston_top_normal"
    };
    let mut plate = px([0.0, 0.0, 0.0], [16.0, 16.0, 4.0], ["piston_side"; 6]);
    plate.tex[NORTH] = front;
    plate.tex[SOUTH] = "piston_top_normal";
    let shaft = part([6.0, 6.0, 4.0], [10.0, 10.0, 16.0], "piston_side", ALL);
    push_turned(vec![plate, shaft], x, y, out);
}

fn door(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, tex: [&'static str; 6], out: &mut Vec<Bx>) {
    let meta = state & 15;
    let upper = meta & 8 != 0;
    let (low, high) = if upper {
        (nb(0, -1, 0), state)
    } else {
        (state, nb(0, 1, 0))
    };
    let facing = low & 3;
    let open = low & 4 != 0;
    let hinge_right = high & 1 != 0;
    // Slab sides: 0 west, 1 north, 2 east, 3 south.
    let side = if !open {
        facing as usize
    } else {
        [[1, 3], [2, 0], [3, 1], [0, 2]][facing as usize][hinge_right as usize]
    };
    let (a, b) = [
        ([0.0, 0.0, 0.0], [3.0, 16.0, 16.0]),
        ([0.0, 0.0, 0.0], [16.0, 16.0, 3.0]),
        ([13.0, 0.0, 0.0], [16.0, 16.0, 16.0]),
        ([0.0, 0.0, 13.0], [16.0, 16.0, 16.0]),
    ][side];
    let mut bx = px(a, b, tex);
    if upper {
        bx.tex = [crate::blocks::door_upper(tex[0]); 6];
    }
    bx.cull = false;
    out.push(bx);
}

#[cfg(test)]
mod tests {
    #[test]
    fn cauldron_box_list_has_water_only_when_filled() {
        let list = |level: u16| super::boxes(118 << 4 | level, &|_, _, _| 0);
        let (empty, full) = (list(0), list(3));
        // Four walls, floor and eight leg boxes.
        assert_eq!(empty.len(), 13);
        assert!(empty.iter().all(|b| b.min[1] != b.max[1]));
        assert_eq!(full.len(), 14);
        let water = full.last().unwrap();
        assert_eq!(
            (water.min, water.max, water.tex[0]),
            (
                [0.125, 0.9375, 0.125],
                [0.875, 0.9375, 0.875],
                "water_still"
            )
        );
        assert_eq!(water.mask, super::bit(super::UP));
        assert_eq!(list(1).last().unwrap().min[1], 0.5625);
    }

    #[test]
    fn hopper_spout_and_anvil_follow_facing() {
        let spout = |meta: u16| super::boxes(154 << 4 | meta, &|_, _, _| 0).pop().unwrap();
        assert_eq!(spout(0).min, [0.375, 0.0, 0.375]);
        assert_eq!(
            (spout(2).min, spout(2).max),
            ([0.375, 0.25, 0.0], [0.625, 0.5, 0.25])
        );
        assert_eq!(
            (spout(3).min, spout(3).max),
            ([0.375, 0.25, 0.75], [0.625, 0.5, 1.0])
        );
        assert_eq!(
            (spout(4).min, spout(4).max),
            ([0.0, 0.25, 0.375], [0.25, 0.5, 0.625])
        );
        assert_eq!(
            (spout(5).min, spout(5).max),
            ([0.75, 0.25, 0.375], [1.0, 0.5, 0.625])
        );
        // Anvil head runs along z when facing south or north, along x when facing west or east.
        let head = |meta: u16| super::boxes(145 << 4 | meta, &|_, _, _| 0)[3].clone();
        assert_eq!(head(0).max, [0.8125, 1.0, 1.0]);
        assert_eq!(
            (head(1).min, head(1).max),
            ([0.0, 0.625, 0.1875], [1.0, 1.0, 0.8125])
        );
        assert_eq!(head(4).tex[0], "anvil_top_damaged_1");
    }

    #[test]
    fn plain_rail_keeps_curve_rotation_for_meta_8_and_9() {
        let rot = |meta: u16| super::boxes(66 << 4 | meta, &|_, _, _| 0)[0].rot;
        assert_eq!([rot(6), rot(7), rot(8), rot(9)], [0, 1, 2, 3]);
    }

    #[test]
    fn open_door_east_facing_swings_to_north_or_south_by_hinge() {
        let bounds = |lower: u16, upper: u16| {
            let b = &super::boxes(64 << 4 | lower, &|_, _, _| 64 << 4 | upper)[0];
            (b.min, b.max)
        };
        assert_eq!(bounds(0, 8), ([0.0; 3], [0.1875, 1.0, 1.0]));
        assert_eq!(bounds(4, 8), ([0.0; 3], [1.0, 1.0, 0.1875]));
        assert_eq!(bounds(4, 9), ([0.0, 0.0, 0.8125], [1.0; 3]));
    }

    fn hull(state: u16) -> ([f32; 3], [f32; 3]) {
        let list = super::boxes(state, &|_, _, _| 0);
        let lo = [0, 1, 2].map(|a| list.iter().map(|b| b.min[a]).fold(1.0, f32::min));
        let hi = [0, 1, 2].map(|a| list.iter().map(|b| b.max[a]).fold(0.0, f32::max));
        (lo, hi)
    }

    #[test]
    fn lever_boxes_follow_wall_and_power() {
        // East-facing lever sits on the west wall: base 3/16 deep, handle inside the selection box.
        let off = super::boxes(69 << 4 | 1, &|_, _, _| 0);
        let on = super::boxes(69 << 4 | 9, &|_, _, _| 0);
        assert_eq!(off.len(), 3);
        assert_eq!(
            (off[0].min, off[0].max),
            ([0.0, 0.25, 0.3125], [0.1875, 0.75, 0.6875])
        );
        let (lo, hi) = hull(69 << 4 | 1);
        assert!(lo[0] >= 0.0 && hi[0] <= 0.5 + 1e-6);
        // Powered and unpowered handles lean to opposite sides of the base centre (z = 0.5 on this wall).
        assert!(off[2].min[1] > 0.5 && on[2].min[1] < 0.5);
    }

    #[test]
    fn pressed_button_is_flat_against_its_wall() {
        let boxes = |meta: u16| super::boxes(77 << 4 | meta, &|_, _, _| 0);
        // South-facing button hangs on the north wall (z = 0): 2/16 deep, 1/16 pressed.
        assert_eq!(
            (boxes(3)[0].min, boxes(3)[0].max),
            ([0.3125, 0.375, 0.0], [0.6875, 0.625, 0.125])
        );
        assert_eq!(
            (boxes(11)[0].min, boxes(11)[0].max),
            ([0.3125, 0.375, 0.0], [0.6875, 0.625, 0.0625])
        );
    }
}
