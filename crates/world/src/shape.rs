//! Block collision of Minecraft 1.8.9 for entity movement: `Block.addCollisionBoxesToList` of
//! every block state (boxes relative to the block cell) and the `Material` and `slipperiness`
//! values that the movement code reads.
//!
//! Stairs, fences, panes, walls and doors read their neighbours; every other block depends on
//! its state only, and `SHAPES` lists the boxes the real jar returns per metadata value.
//! `dig::selection_box` supplies the neighbour logic that collision shares with the selection
//! outline. The differential tests compare every state and the neighbour cases with the jar.

use crate::dig::{fence_connects, pane_joins, selection_box};
use crate::place::is_stairs;

type Cell = [f64; 6];
type At<'a> = &'a dyn Fn([i32; 3]) -> u16;

/// Blocks without a collision box in every state. Ids past the last block of 1.8.9 count too.
const NO_COLLISION: &[u16] = &[
    0, 6, 8, 9, 10, 11, 27, 28, 30, 31, 32, 36, 37, 38, 39, 40, 50, 51, 55, 59, 63, 66, 68, 69, 70,
    72, 75, 76, 77, 83, 90, 104, 105, 106, 115, 119, 131, 132, 141, 142, 143, 147, 148, 157, 175,
    176, 177,
];

/// `(block id, [(set of metadata values as a bit mask, boxes)])` for the blocks that are neither
/// a full cube nor empty and do not read their neighbours.
type Shapes = &'static [(u16, &'static [(u16, &'static [Cell])])];

const SHAPES: Shapes = &[
    (26, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.5625, 1.0]])]),
    (
        29,
        &[
            (0x003f, &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x0100, &[[0.0, 0.25, 0.0, 1.0, 1.0, 1.0]]),
            (0x0200, &[[0.0, 0.0, 0.0, 1.0, 0.75, 1.0]]),
            (0x0400, &[[0.0, 0.0, 0.25, 1.0, 1.0, 1.0]]),
            (0x0800, &[[0.0, 0.0, 0.0, 1.0, 1.0, 0.75]]),
            (0x1000, &[[0.25, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x2000, &[[0.0, 0.0, 0.0, 0.75, 1.0, 1.0]]),
        ],
    ),
    (
        33,
        &[
            (0x003f, &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x0100, &[[0.0, 0.25, 0.0, 1.0, 1.0, 1.0]]),
            (0x0200, &[[0.0, 0.0, 0.0, 1.0, 0.75, 1.0]]),
            (0x0400, &[[0.0, 0.0, 0.25, 1.0, 1.0, 1.0]]),
            (0x0800, &[[0.0, 0.0, 0.0, 1.0, 1.0, 0.75]]),
            (0x1000, &[[0.25, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x2000, &[[0.0, 0.0, 0.0, 0.75, 1.0, 1.0]]),
        ],
    ),
    (
        34,
        &[
            (
                0x0101,
                &[
                    [0.0, 0.0, 0.0, 1.0, 0.25, 1.0],
                    [0.375, 0.25, 0.375, 0.625, 1.0, 0.625],
                ],
            ),
            (
                0x0202,
                &[
                    [0.0, 0.75, 0.0, 1.0, 1.0, 1.0],
                    [0.375, 0.0, 0.375, 0.625, 0.75, 0.625],
                ],
            ),
            (
                0x0404,
                &[
                    [0.0, 0.0, 0.0, 1.0, 1.0, 0.25],
                    [0.25, 0.375, 0.25, 0.75, 0.625, 1.0],
                ],
            ),
            (
                0x0808,
                &[
                    [0.0, 0.0, 0.75, 1.0, 1.0, 1.0],
                    [0.25, 0.375, 0.0, 0.75, 0.625, 0.75],
                ],
            ),
            (
                0x1010,
                &[
                    [0.0, 0.0, 0.0, 0.25, 1.0, 1.0],
                    [0.375, 0.25, 0.25, 0.625, 0.75, 1.0],
                ],
            ),
            (
                0x2020,
                &[
                    [0.0, 0.375, 0.25, 0.75, 0.625, 0.75],
                    [0.75, 0.0, 0.0, 1.0, 1.0, 1.0],
                ],
            ),
        ],
    ),
    (
        44,
        &[
            (0x00ff, &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            (0xff00, &[[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]]),
        ],
    ),
    (
        54,
        &[(0xffff, &[[0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]])],
    ),
    (
        65,
        &[
            (0x71c7, &[[0.0, 0.0, 0.875, 1.0, 1.0, 1.0]]),
            (0x8208, &[[0.0, 0.0, 0.0, 1.0, 1.0, 0.125]]),
            (0x0410, &[[0.875, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x0820, &[[0.0, 0.0, 0.0, 0.125, 1.0, 1.0]]),
        ],
    ),
    (
        78,
        &[
            (0x0101, &[]),
            (0x0202, &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]]),
            (0x0404, &[[0.0, 0.0, 0.0, 1.0, 0.25, 1.0]]),
            (0x0808, &[[0.0, 0.0, 0.0, 1.0, 0.375, 1.0]]),
            (0x1010, &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            (0x2020, &[[0.0, 0.0, 0.0, 1.0, 0.625, 1.0]]),
            (0x4040, &[[0.0, 0.0, 0.0, 1.0, 0.75, 1.0]]),
            (0x8080, &[[0.0, 0.0, 0.0, 1.0, 0.875, 1.0]]),
        ],
    ),
    (
        81,
        &[(0xffff, &[[0.0625, 0.0, 0.0625, 0.9375, 0.9375, 0.9375]])],
    ),
    (88, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.875, 1.0]])]),
    (
        92,
        &[
            (0x0001, &[[0.0625, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0002, &[[0.1875, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0004, &[[0.3125, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0008, &[[0.4375, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0010, &[[0.5625, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0020, &[[0.6875, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
            (0x0040, &[[0.8125, 0.0, 0.0625, 0.9375, 0.5, 0.9375]]),
        ],
    ),
    (93, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]])]),
    (94, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]])]),
    (
        96,
        &[
            (0x000f, &[[0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]]),
            (0x1010, &[[0.0, 0.0, 0.8125, 1.0, 1.0, 1.0]]),
            (0x2020, &[[0.0, 0.0, 0.0, 1.0, 1.0, 0.1875]]),
            (0x4040, &[[0.8125, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x8080, &[[0.0, 0.0, 0.0, 0.1875, 1.0, 1.0]]),
            (0x0f00, &[[0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]]),
        ],
    ),
    (
        107,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
    (111, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.015625, 1.0]])]),
    (116, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.75, 1.0]])]),
    (
        117,
        &[(
            0xffff,
            &[
                [0.0, 0.0, 0.0, 1.0, 0.125, 1.0],
                [0.4375, 0.0, 0.4375, 0.5625, 0.875, 0.5625],
            ],
        )],
    ),
    (
        118,
        &[(
            0x000f,
            &[
                [0.0, 0.0, 0.0, 0.125, 1.0, 1.0],
                [0.0, 0.0, 0.0, 1.0, 0.3125, 1.0],
                [0.0, 0.0, 0.0, 1.0, 1.0, 0.125],
                [0.0, 0.0, 0.875, 1.0, 1.0, 1.0],
                [0.875, 0.0, 0.0, 1.0, 1.0, 1.0],
            ],
        )],
    ),
    (
        120,
        &[
            (0x0f0f, &[[0.0, 0.0, 0.0, 1.0, 0.8125, 1.0]]),
            (
                0xf0f0,
                &[
                    [0.0, 0.0, 0.0, 1.0, 0.8125, 1.0],
                    [0.3125, 0.8125, 0.3125, 0.6875, 1.0, 0.6875],
                ],
            ),
        ],
    ),
    (
        122,
        &[(0xffff, &[[0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375]])],
    ),
    (
        126,
        &[
            (0x00ff, &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            (0xff00, &[[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]]),
        ],
    ),
    (
        127,
        &[
            (0x0001, &[[0.375, 0.4375, 0.6875, 0.625, 0.75, 0.9375]]),
            (0x0002, &[[0.0625, 0.4375, 0.375, 0.3125, 0.75, 0.625]]),
            (0x0004, &[[0.375, 0.4375, 0.0625, 0.625, 0.75, 0.3125]]),
            (0x0008, &[[0.6875, 0.4375, 0.375, 0.9375, 0.75, 0.625]]),
            (0x0010, &[[0.3125, 0.3125, 0.5625, 0.6875, 0.75, 0.9375]]),
            (0x0020, &[[0.0625, 0.3125, 0.3125, 0.4375, 0.75, 0.6875]]),
            (0x0040, &[[0.3125, 0.3125, 0.0625, 0.6875, 0.75, 0.4375]]),
            (0x0080, &[[0.5625, 0.3125, 0.3125, 0.9375, 0.75, 0.6875]]),
            (0x0100, &[[0.25, 0.1875, 0.4375, 0.75, 0.75, 0.9375]]),
            (0x0200, &[[0.0625, 0.1875, 0.25, 0.5625, 0.75, 0.75]]),
            (0x0400, &[[0.25, 0.1875, 0.0625, 0.75, 0.75, 0.5625]]),
            (0x0800, &[[0.4375, 0.1875, 0.25, 0.9375, 0.75, 0.75]]),
        ],
    ),
    (
        130,
        &[(0xffff, &[[0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]])],
    ),
    (
        140,
        &[(0xffff, &[[0.3125, 0.0, 0.3125, 0.6875, 0.375, 0.6875]])],
    ),
    (
        144,
        &[
            (0xc3c3, &[[0.25, 0.0, 0.25, 0.75, 0.5, 0.75]]),
            (0x0404, &[[0.25, 0.25, 0.5, 0.75, 0.75, 1.0]]),
            (0x0808, &[[0.25, 0.25, 0.0, 0.75, 0.75, 0.5]]),
            (0x1010, &[[0.5, 0.25, 0.25, 1.0, 0.75, 0.75]]),
            (0x2020, &[[0.0, 0.25, 0.25, 0.5, 0.75, 0.75]]),
        ],
    ),
    (
        146,
        &[(0xffff, &[[0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]])],
    ),
    (149, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]])]),
    (150, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]])]),
    (151, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.375, 1.0]])]),
    (
        154,
        &[(
            0x7d7d,
            &[
                [0.0, 0.0, 0.0, 0.125, 1.0, 1.0],
                [0.0, 0.0, 0.0, 1.0, 0.625, 1.0],
                [0.0, 0.0, 0.0, 1.0, 1.0, 0.125],
                [0.0, 0.0, 0.875, 1.0, 1.0, 1.0],
                [0.875, 0.0, 0.0, 1.0, 1.0, 1.0],
            ],
        )],
    ),
    (
        167,
        &[
            (0x000f, &[[0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]]),
            (0x1010, &[[0.0, 0.0, 0.8125, 1.0, 1.0, 1.0]]),
            (0x2020, &[[0.0, 0.0, 0.0, 1.0, 1.0, 0.1875]]),
            (0x4040, &[[0.8125, 0.0, 0.0, 1.0, 1.0, 1.0]]),
            (0x8080, &[[0.0, 0.0, 0.0, 0.1875, 1.0, 1.0]]),
            (0x0f00, &[[0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]]),
        ],
    ),
    (171, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]])]),
    (178, &[(0xffff, &[[0.0, 0.0, 0.0, 1.0, 0.375, 1.0]])]),
    (
        182,
        &[
            (0x00ff, &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            (0xff00, &[[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]]),
        ],
    ),
    (
        183,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
    (
        184,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
    (
        185,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
    (
        186,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
    (
        187,
        &[
            (0x0505, &[[0.0, 0.0, 0.375, 1.0, 1.5, 0.625]]),
            (0x0a0a, &[[0.375, 0.0, 0.0, 0.625, 1.5, 1.0]]),
            (0xf0f0, &[]),
        ],
    ),
];

/// `Material.isLiquid`: water and lava, flowing and still.
pub(crate) fn is_liquid(id: u16) -> bool {
    matches!(id, 8..=11)
}

/// `Material.isSolid`: false for air, plants, circuits, carpet, liquids and the other
/// `MaterialLogic` and `MaterialTransparent` blocks.
pub(crate) fn is_solid_material(id: u16) -> bool {
    !matches!(
        id,
        0 | 6 | 8..=11 | 27..=28 | 31..=32 | 37..=40 | 50..=51 | 55 | 59 | 65..=66 | 69 | 75..=78
            | 83 | 90 | 93..=94 | 104..=106 | 111 | 115 | 119 | 127 | 131..=132 | 140..=144
            | 149..=150 | 157 | 171 | 175
    )
}

/// `Material.blocksMovement`: as `is_solid_material`, but cobweb does not block.
pub(crate) fn blocks_movement(id: u16) -> bool {
    id != 30 && is_solid_material(id)
}

/// `Block.slipperiness`: 0.98 for ice and packed ice, 0.8 for slime, 0.6 otherwise.
pub(crate) fn slipperiness(id: u16) -> f32 {
    match id {
        79 | 174 => 0.98,
        165 => 0.8,
        _ => 0.6,
    }
}

/// Add the collision boxes of the block at `pos`, relative to its cell, in vanilla order.
pub(crate) fn push_boxes(at: At, pos: [i32; 3], out: &mut Vec<Cell>) {
    let state = at(pos);
    let (id, meta) = (state >> 4, state & 15);
    match id {
        _ if is_stairs(id) => stairs(at, pos, state, out),
        85 | 113 | 188..=192 => fence(at, pos, id, out),
        101 | 102 | 160 => pane(at, pos, out),
        139 => out.extend(selection_box(at, pos).map(|b| [b[0], b[1], b[2], b[3], 1.5, b[5]])),
        64 | 71 | 193..=197 => out.extend(selection_box(at, pos)),
        _ if id > 197 || NO_COLLISION.contains(&id) => {}
        _ => table_boxes(id, meta, out),
    }
}

/// The boxes of a block state from `SHAPES`; a full cube for ids without an entry.
fn table_boxes(id: u16, meta: u16, out: &mut Vec<Cell>) {
    match SHAPES.iter().find(|s| s.0 == id) {
        None => out.push([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
        Some((_, groups)) => {
            // A metadata value that vanilla rejects falls back to the first group.
            let group = groups
                .iter()
                .find(|g| g.0 >> meta & 1 != 0)
                .unwrap_or(&groups[0]);
            out.extend_from_slice(group.1);
        }
    }
}

/// `BlockFence.addCollisionBoxesToList`: one 1.5 high box along z and one along x, or a post.
fn fence(at: At, [x, y, z]: [i32; 3], id: u16, out: &mut Vec<Cell>) {
    let joins = |p: [i32; 3]| fence_connects(id, at(p) >> 4);
    let (n, s) = (joins([x, y, z - 1]), joins([x, y, z + 1]));
    let (w, e) = (joins([x - 1, y, z]), joins([x + 1, y, z]));
    let (z0, z1) = (if n { 0.0 } else { 0.375 }, if s { 1.0 } else { 0.625 });
    if n || s {
        out.push([0.375, 0.0, z0, 0.625, 1.5, z1]);
    }
    let (x0, x1) = (if w { 0.0 } else { 0.375 }, if e { 1.0 } else { 0.625 });
    if w || e || (!n && !s) {
        out.push([x0, 0.0, 0.375, x1, 1.5, 0.625]);
    }
}

/// `BlockPane.addCollisionBoxesToList`: one slab along x, one along z. `lo` and `hi` are the
/// joins at the low and high end of the axis, `any` a join on either axis.
fn pane_arm(lo: bool, hi: bool, any: bool, along_x: bool, out: &mut Vec<Cell>) {
    let (a, b) = if (!lo || !hi) && any {
        if lo {
            (0.0, 0.5)
        } else if hi {
            (0.5, 1.0)
        } else {
            return;
        }
    } else {
        (0.0, 1.0)
    };
    out.push(if along_x {
        [a, 0.0, 0.4375, b, 1.0, 0.5625]
    } else {
        [0.4375, 0.0, a, 0.5625, 1.0, b]
    });
}

fn pane(at: At, [x, y, z]: [i32; 3], out: &mut Vec<Cell>) {
    let joins = |p: [i32; 3]| pane_joins(at(p) >> 4);
    let (n, s) = (joins([x, y, z - 1]), joins([x, y, z + 1]));
    let (w, e) = (joins([x - 1, y, z]), joins([x + 1, y, z]));
    let any = n || s || w || e;
    pane_arm(w, e, any, true, out);
    pane_arm(n, s, any, false, out);
}

const EAST: u16 = 0;
const WEST: u16 = 1;
const SOUTH: u16 = 2;
const NORTH: u16 = 3;

/// Neighbour lookups of one stair: same half, and the facing it asks about.
struct Stair<'a> {
    at: At<'a>,
    facing: u16,
    top: bool,
}

impl Stair<'_> {
    /// A neighbouring stair of the same half and `facing`, as `func_176307_f`.
    fn same(&self, p: [i32; 3]) -> bool {
        self.other(p) == Some(self.facing)
    }

    /// The facing of a neighbouring stair of the same half.
    fn other(&self, p: [i32; 3]) -> Option<u16> {
        let s = (self.at)(p);
        (is_stairs(s >> 4) && (s & 4 != 0) == self.top).then_some(s & 3)
    }

    /// `func_176306_h`: the step on the side the stair faces, cut where a stair beside it turns.
    /// Returns `[x0, x1, z0, z1]` and whether the step is uncut.
    fn step(&self, pos: [i32; 3]) -> ([f64; 4], bool) {
        if self.facing <= WEST {
            self.step_x(pos)
        } else {
            self.step_z(pos)
        }
    }

    fn step_x(&self, [x, y, z]: [i32; 3]) -> ([f64; 4], bool) {
        let east = self.facing == EAST;
        let mut b = [0.0, 1.0, 0.0, 1.0];
        b[usize::from(!east)] = 0.5;
        let cut = match self.other([if east { x + 1 } else { x - 1 }, y, z]) {
            Some(NORTH) if !self.same([x, y, z + 1]) => Some(3),
            Some(SOUTH) if !self.same([x, y, z - 1]) => Some(2),
            _ => None,
        };
        if let Some(i) = cut {
            b[i] = 0.5;
        }
        (b, cut.is_none())
    }

    fn step_z(&self, [x, y, z]: [i32; 3]) -> ([f64; 4], bool) {
        let south = self.facing == SOUTH;
        let mut b = [0.0, 1.0, 0.0, 0.5];
        if south {
            (b[2], b[3]) = (0.5, 1.0);
        }
        let cut = match self.other([x, y, if south { z + 1 } else { z - 1 }]) {
            Some(EAST) if !self.same([x + 1, y, z]) => Some(1),
            Some(WEST) if !self.same([x - 1, y, z]) => Some(0),
            _ => None,
        };
        if let Some(i) = cut {
            b[i] = 0.5;
        }
        (b, cut.is_none())
    }

    /// `func_176304_i`: the second step of an inner corner, on the side opposite the stair beside.
    /// Returns `[x0, z0, x1, z1]`.
    fn corner(&self, pos: [i32; 3]) -> Option<[f64; 4]> {
        if self.facing <= WEST {
            self.corner_x(pos)
        } else {
            self.corner_z(pos)
        }
    }

    fn corner_x(&self, [x, y, z]: [i32; 3]) -> Option<[f64; 4]> {
        let east = self.facing == EAST;
        let (c0, c1) = if east { (0.0, 0.5) } else { (0.5, 1.0) };
        match self.other([if east { x - 1 } else { x + 1 }, y, z]) {
            Some(NORTH) if !self.same([x, y, z - 1]) => Some([c0, 0.0, c1, 0.5]),
            Some(SOUTH) if !self.same([x, y, z + 1]) => Some([c0, 0.5, c1, 1.0]),
            _ => None,
        }
    }

    fn corner_z(&self, [x, y, z]: [i32; 3]) -> Option<[f64; 4]> {
        let south = self.facing == SOUTH;
        let (c0, c1) = if south { (0.0, 0.5) } else { (0.5, 1.0) };
        match self.other([x, y, if south { z - 1 } else { z + 1 }]) {
            Some(EAST) if !self.same([x + 1, y, z]) => Some([0.0, c0, 0.5, c1]),
            Some(WEST) if !self.same([x - 1, y, z]) => Some([0.5, c0, 1.0, c1]),
            _ => None,
        }
    }
}

/// `BlockStairs.addCollisionBoxesToList`: the slab half, the step, and the second step of an
/// inner corner. Facing of metadata bits 0 and 1: east, west, south, north.
fn stairs(at: At, pos: [i32; 3], state: u16, out: &mut Vec<Cell>) {
    let stair = Stair {
        at,
        facing: state & 3,
        top: state & 4 != 0,
    };
    let (lo, hi) = if stair.top { (0.0, 0.5) } else { (0.5, 1.0) };
    out.push(if stair.top {
        [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]
    });
    let ([x0, x1, z0, z1], plain) = stair.step(pos);
    out.push([x0, lo, z0, x1, hi, z1]);
    if !plain {
        return;
    }
    if let Some([a, b, c, d]) = stair.corner(pos) {
        out.push([a, lo, b, c, hi, d]);
    }
}
