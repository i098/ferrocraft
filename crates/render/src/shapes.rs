//! Box models of the blocks that are not full cubes: slabs, stairs, fences, panes, doors, torches and more.
//! Coordinates are block-local in `0..=1`; sources are written in 1/16 block units like vanilla JSON models.

use crate::blocks::{look, Kind, Tint, DOWN, EAST, NORTH, SOUTH, UP, WEST};

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
    let (id, meta) = (state >> 4, state & 15);
    let l = look(state);
    let t = l.tex;
    let mut out = Vec::new();
    match id {
        44 | 126 | 182 => {
            let (lo, hi) = if meta & 8 != 0 { (8.0, 16.0) } else { (0.0, 8.0) };
            out.push(px([0.0, lo, 0.0], [16.0, hi, 16.0], t));
        }
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => {
            let top = meta & 4 != 0;
            let (slab, step) = if top { ((8.0, 16.0), (0.0, 8.0)) } else { ((0.0, 8.0), (8.0, 16.0)) };
            out.push(px([0.0, slab.0, 0.0], [16.0, slab.1, 16.0], t));
            let (x, z) = match meta & 3 {
                0 => ((8.0, 16.0), (0.0, 16.0)),
                1 => ((0.0, 8.0), (0.0, 16.0)),
                2 => ((0.0, 16.0), (8.0, 16.0)),
                _ => ((0.0, 16.0), (0.0, 8.0)),
            };
            out.push(px([x.0, step.0, z.0], [x.1, step.1, z.1], t));
        }
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
        101 | 102 | 160 => {
            let top = if id == 101 { "iron_bars" } else if id == 102 { "glass_pane_top" } else { crate::blocks::pane_top(meta) };
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
                for (i, (dx, dz)) in [(0, -1), (0, 1), (-1, 0), (1, 0)].into_iter().enumerate() {
                    if !sides[i] {
                        continue;
                    }
                    let mut b = match (dx, dz) {
                        (0, -1) => px([7.0, 0.0, 0.0], [9.0, 16.0, 7.0], t),
                        (0, _) => px([7.0, 0.0, 9.0], [9.0, 16.0, 16.0], t),
                        (-1, _) => px([0.0, 0.0, 7.0], [7.0, 16.0, 9.0], t),
                        _ => px([9.0, 0.0, 7.0], [16.0, 16.0, 9.0], t),
                    };
                    b.tex[UP] = top;
                    b.tex[DOWN] = top;
                    out.push(b);
                }
            }
        }
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
        27 | 28 | 157 | 66 => {
            let mut b = px([0.0, 0.0, 0.0], [16.0, 1.0, 16.0], t);
            b.mask = bit(UP);
            b.rot = match (id, meta & 7) {
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
        64 | 71 | 193..=197 => door(state, nb, t, &mut out),
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
        78 => out.push(px([0.0, 0.0, 0.0], [16.0, ((meta & 7) as f32 + 1.0) * 2.0, 16.0], t)),
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
        92 => out.push(px([1.0 + 2.0 * (meta.min(6) as f32), 0.0, 1.0], [15.0, 8.0, 15.0], t)),
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
        171 => out.push(px([0.0, 0.0, 0.0], [16.0, 1.0, 16.0], t)),
        _ => {}
    }
    out
}

fn door(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, tex: [&'static str; 6], out: &mut Vec<Bx>) {
    let meta = state & 15;
    let upper = meta & 8 != 0;
    let (low, high) = if upper { (nb(0, -1, 0), state) } else { (state, nb(0, 1, 0)) };
    let facing = low & 3;
    let open = low & 4 != 0;
    let hinge_right = high & 1 != 0;
    // Slab sides: 0 west, 1 north, 2 east, 3 south.
    let side = if !open {
        facing
    } else {
        match (facing, hinge_right) {
            (0, false) => 3,
            (0, true) => 1,
            (1, false) => 2,
            (1, true) => 0,
            (2, false) => 1,
            (2, true) => 3,
            (3, false) => 0,
            _ => 2,
        }
    };
    let (a, b) = match side {
        0 => ([0.0, 0.0, 0.0], [3.0, 16.0, 16.0]),
        1 => ([0.0, 0.0, 0.0], [16.0, 16.0, 3.0]),
        2 => ([13.0, 0.0, 0.0], [16.0, 16.0, 16.0]),
        _ => ([0.0, 0.0, 13.0], [16.0, 16.0, 16.0]),
    };
    let mut bx = px(a, b, tex);
    if upper {
        bx.tex = [crate::blocks::door_upper(tex[0]); 6];
    }
    bx.cull = false;
    out.push(bx);
}
