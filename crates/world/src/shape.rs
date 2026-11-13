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
        _ => match SHAPES.iter().find(|s| s.0 == id) {
            None => out.push([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
            Some((_, groups)) => {
                // A metadata value that vanilla rejects falls back to the first group.
                let group = groups
                    .iter()
                    .find(|g| g.0 >> meta & 1 != 0)
                    .unwrap_or(&groups[0]);
                out.extend_from_slice(group.1);
            }
        },
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

/// `BlockPane.addCollisionBoxesToList`.
fn pane(at: At, [x, y, z]: [i32; 3], out: &mut Vec<Cell>) {
    let joins = |p: [i32; 3]| pane_joins(at(p) >> 4);
    let (n, s) = (joins([x, y, z - 1]), joins([x, y, z + 1]));
    let (w, e) = (joins([x - 1, y, z]), joins([x + 1, y, z]));
    let any = n || s || w || e;
    if (!w || !e) && any {
        if w {
            out.push([0.0, 0.0, 0.4375, 0.5, 1.0, 0.5625]);
        } else if e {
            out.push([0.5, 0.0, 0.4375, 1.0, 1.0, 0.5625]);
        }
    } else {
        out.push([0.0, 0.0, 0.4375, 1.0, 1.0, 0.5625]);
    }
    if (!n || !s) && any {
        if n {
            out.push([0.4375, 0.0, 0.0, 0.5625, 1.0, 0.5]);
        } else if s {
            out.push([0.4375, 0.0, 0.5, 0.5625, 1.0, 1.0]);
        }
    } else {
        out.push([0.4375, 0.0, 0.0, 0.5625, 1.0, 1.0]);
    }
}

/// `BlockStairs.addCollisionBoxesToList`: the slab half, the step, and the second step of an
/// inner corner. Facing of metadata bits 0 and 1: east, west, south, north.
fn stairs(at: At, [x, y, z]: [i32; 3], state: u16, out: &mut Vec<Cell>) {
    const EAST: u16 = 0;
    const WEST: u16 = 1;
    const SOUTH: u16 = 2;
    const NORTH: u16 = 3;
    let (facing, top) = (state & 3, state & 4 != 0);
    let (lo, hi) = if top { (0.0, 0.5) } else { (0.5, 1.0) };
    out.push(if top {
        [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]
    });
    // A neighbouring stair of the same half and `facing`, as `func_176307_f`.
    let same = |p: [i32; 3]| {
        let s = at(p);
        is_stairs(s >> 4) && s & 3 == facing && (s & 4 != 0) == top
    };
    // The facing of a neighbouring stair of the same half.
    let other = |p: [i32; 3]| {
        let s = at(p);
        (is_stairs(s >> 4) && (s & 4 != 0) == top).then_some(s & 3)
    };
    // `func_176306_h`: the step on the side the stair faces, cut where a stair beside it turns.
    let (mut x0, mut x1, mut z0, mut z1) = (0.0, 1.0, 0.0, 0.5);
    let mut plain = true;
    match facing {
        EAST | WEST => {
            let side = if facing == EAST { 1 } else { -1 };
            if facing == EAST {
                x0 = 0.5;
            } else {
                x1 = 0.5;
            }
            z1 = 1.0;
            match other([x + side, y, z]) {
                Some(NORTH) if !same([x, y, z + 1]) => (z1, plain) = (0.5, false),
                Some(SOUTH) if !same([x, y, z - 1]) => (z0, plain) = (0.5, false),
                _ => {}
            }
        }
        _ => {
            let side = if facing == SOUTH { 1 } else { -1 };
            if facing == SOUTH {
                (z0, z1) = (0.5, 1.0);
            }
            match other([x, y, z + side]) {
                Some(EAST) if !same([x + 1, y, z]) => (x1, plain) = (0.5, false),
                Some(WEST) if !same([x - 1, y, z]) => (x0, plain) = (0.5, false),
                _ => {}
            }
        }
    }
    out.push([x0, lo, z0, x1, hi, z1]);
    if !plain {
        return;
    }
    // `func_176304_i`: the second step of an inner corner, on the side opposite the stair beside.
    let corner = match facing {
        EAST | WEST => {
            let side = if facing == EAST { -1 } else { 1 };
            let (cx0, cx1) = if facing == EAST {
                (0.0, 0.5)
            } else {
                (0.5, 1.0)
            };
            match other([x + side, y, z]) {
                Some(NORTH) if !same([x, y, z - 1]) => Some([cx0, 0.0, cx1, 0.5]),
                Some(SOUTH) if !same([x, y, z + 1]) => Some([cx0, 0.5, cx1, 1.0]),
                _ => None,
            }
        }
        _ => {
            let side = if facing == SOUTH { -1 } else { 1 };
            let (cz0, cz1) = if facing == SOUTH {
                (0.0, 0.5)
            } else {
                (0.5, 1.0)
            };
            match other([x, y, z + side]) {
                Some(EAST) if !same([x + 1, y, z]) => Some([0.0, cz0, 0.5, cz1]),
                Some(WEST) if !same([x - 1, y, z]) => Some([0.5, cz0, 1.0, cz1]),
                _ => None,
            }
        }
    };
    if let Some([a, b, c, d]) = corner {
        out.push([a, lo, b, c, hi, d]);
    }
}
