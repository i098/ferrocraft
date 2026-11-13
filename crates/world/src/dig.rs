//! Block breaking of Minecraft 1.8.9: the block ray, bare-hand break speed and the dig state
//! machine of `PlayerControllerMP` and `Minecraft.clickMouse`/`sendClickBlockToController`.
//!
//! Not ported yet: held tools and enchantments (the client has no inventory, so the hand is
//! always empty), potion effects, water, entity targets, `CanDestroy` tags of adventure mode
//! (an empty hand never digs there; the swing is kept) and the world border.
//! The break speed of every block is checked against the real 1.8.9 jar through `oracle`.

use crate::{cos, sin};

/// Block ids 0..=197 of 1.8.9: `Block.blockHardness`, -1 for unbreakable.
const HARDNESS: [f32; 198] = [
    0.0, 1.5, 0.6, 0.5, 2.0, 2.0, 0.0, -1.0, 100.0, 100.0, // 0-9
    100.0, 100.0, 0.5, 0.6, 3.0, 3.0, 3.0, 2.0, 0.2, 0.6, // 10-19
    0.3, 3.0, 3.0, 3.5, 0.8, 0.8, 0.2, 0.7, 0.7, 0.5, // 20-29
    4.0, 0.0, 0.0, 0.5, 0.5, 0.8, -1.0, 0.0, 0.0, 0.0, // 30-39
    0.0, 3.0, 5.0, 2.0, 2.0, 2.0, 0.0, 1.5, 2.0, 50.0, // 40-49
    0.0, 0.0, 5.0, 2.0, 2.5, 0.0, 3.0, 5.0, 2.5, 0.0, // 50-59
    0.6, 3.5, 3.5, 1.0, 3.0, 0.4, 0.7, 2.0, 1.0, 0.5, // 60-69
    0.5, 5.0, 0.5, 3.0, 3.0, 0.0, 0.0, 0.5, 0.1, 0.5, // 70-79
    0.2, 0.4, 0.6, 0.0, 2.0, 2.0, 1.0, 0.4, 0.5, 0.3, // 80-89
    -1.0, 1.0, 0.5, 0.0, 0.0, 0.3, 3.0, 0.75, 1.5, 0.2, // 90-99
    0.2, 5.0, 0.3, 1.0, 0.0, 0.0, 0.2, 2.0, 2.0, 1.5, // 100-109
    0.6, 0.0, 2.0, 2.0, 2.0, 0.0, 5.0, 0.5, 2.0, -1.0, // 110-119
    -1.0, 3.0, 3.0, 0.3, 0.3, 2.0, 2.0, 0.2, 0.8, 3.0, // 120-129
    22.5, 0.0, 0.0, 5.0, 2.0, 2.0, 2.0, -1.0, 3.0, 2.0, // 130-139
    0.0, 0.0, 0.0, 0.5, 1.0, 5.0, 2.5, 0.5, 0.5, 0.0, // 140-149
    0.0, 0.2, 5.0, 3.0, 3.0, 0.8, 0.8, 0.7, 3.5, 1.25, // 150-159
    0.3, 0.2, 2.0, 2.0, 2.0, 0.0, -1.0, 5.0, 1.5, 0.3, // 160-169
    0.5, 0.1, 1.25, 5.0, 0.5, 0.0, 1.0, 1.0, 0.2, 0.8, // 170-179
    0.8, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, // 180-189
    2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, // 190-197
];

/// `Block.blockHardness` of a block id; -1 is unbreakable. Ids past 1.8.9's last block count as
/// unbreakable.
pub fn hardness(id: u16) -> f32 {
    HARDNESS.get(usize::from(id)).copied().unwrap_or(-1.0)
}

/// Blocks whose material has `requiresTool` (rock, iron, anvil, snow, web): the bare hand
/// digs them at 1/100 speed and drops nothing.
pub fn requires_tool(id: u16) -> bool {
    matches!(
        id,
        1 | 4 | 7 | 14..=16 | 21..=24 | 30 | 41..=45 | 48 | 49 | 52 | 56 | 57 | 61 | 62 | 67
            | 70 | 71 | 73 | 74 | 78 | 80 | 87 | 98 | 101 | 108 | 109 | 112..=114 | 116 | 117
            | 118 | 120 | 121 | 128..=130 | 133 | 137 | 139 | 145 | 147 | 148 | 152..=156
            | 158 | 159 | 166..=168 | 172 | 173 | 179..=182
    )
}

/// `Block.getPlayerRelativeBlockHardness` for an empty hand: the share of the block that one
/// tick of digging removes. At 1.0 or more the block breaks at once.
pub fn relative_hardness(id: u16, on_ground: bool) -> f32 {
    let h = hardness(id);
    if h < 0.0 {
        return 0.0;
    }
    let mut speed = 1.0_f32;
    if !on_ground {
        speed /= 5.0;
    }
    speed / h / if requires_tool(id) { 100.0 } else { 30.0 }
}

/// Selectable box of the block at `pos` relative to its cell as `[x0, y0, z0, x1, y1, z1]`, or
/// `None` when the block ray passes through (air, water, lava, fire). Doors, chests, fences,
/// panes, walls and vines read their neighbours. Stairs, which vanilla traces as several boxes,
/// pistons (29, 33, 34), the brewing stand (117), cocoa (127), tripwire hooks and wires (131,
/// 132), anvils (145), skulls (144), double plants (175), banners (176, 177) and end rods (198)
/// are full cubes here.
pub fn selection_box(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3]) -> Option<[f64; 6]> {
    let state = at(pos);
    let id = state >> 4;
    let [x, y, z] = pos;
    // north (z-1), south (z+1), west (x-1), east (x+1)
    let near = || [[x, y, z - 1], [x, y, z + 1], [x - 1, y, z], [x + 1, y, z]].map(|p| at(p) >> 4);
    Some(match id {
        64 | 71 | 193..=197 => door_at(at, pos, state),
        54 | 146 => chest_box(near().map(|n| n == id)),
        85 | 113 | 188..=192 => fence_box(near().map(|n| fence_connects(id, n))),
        101 | 102 | 160 => pane_box(near().map(pane_joins)),
        139 => wall_box(near().map(|n| n == 139 || is_gate(n) || opaque_cube(n))),
        106 => vine_box(state & 15, opaque_cube(at([x, y + 1, z]) >> 4)),
        _ => return state_box(state),
    })
}

fn door_at(at: &dyn Fn([i32; 3]) -> u16, [x, y, z]: [i32; 3], state: u16) -> [f64; 6] {
    let (lower, upper) = if state & 8 != 0 {
        (at([x, y - 1, z]), state)
    } else {
        (state, at([x, y + 1, z]))
    };
    door_box(lower & 3, lower & 4 != 0, upper & 1 != 0)
}

/// `BlockDoor.setBoundBasedOnMeta`: a door leaf is 3/16 thick. Facing picks one of four leaf
/// positions; opening turns it a quarter one way, or three the other when hinged right.
fn door_box(facing: u16, open: bool, hinge_right: bool) -> [f64; 6] {
    const T: f64 = 0.1875;
    let leaves = [
        [0.0, 0.0, 0.0, T, 1.0, 1.0],
        [0.0, 0.0, 0.0, 1.0, 1.0, T],
        [1.0 - T, 0.0, 0.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 1.0 - T, 1.0, 1.0, 1.0],
    ];
    let turn = if open {
        1 + 2 * u16::from(hinge_right)
    } else {
        0
    };
    leaves[usize::from((facing + turn) % 4)]
}

fn is_gate(id: u16) -> bool {
    matches!(id, 107 | 183..=187)
}

/// Full opaque cubes: `material.isOpaque() && isFullCube()`.
pub(crate) fn opaque_cube(id: u16) -> bool {
    matches!(
        id,
        1..=5 | 7 | 12 | 13 | 14..=17 | 19 | 21..=25 | 35 | 41..=43 | 45 | 47..=49 | 56..=58
            | 61 | 62 | 73 | 74 | 80 | 82 | 84 | 87 | 88 | 97..=100 | 110 | 112 | 174
            | 121 | 123 | 124 | 125 | 129 | 133 | 137 | 152 | 153 | 155 | 158 | 159 | 162
            | 168 | 170 | 172 | 173 | 179 | 181
    )
}

/// `BlockPane.canPaneConnectToBlock`: any opaque-cube block (`isFullBlock`), glass or a pane.
pub(crate) fn pane_joins(id: u16) -> bool {
    opaque_cube(id)
        || matches!(
            id,
            20 | 46 | 86 | 89 | 91 | 95 | 101 | 102 | 103 | 160 | 169
        )
}

/// `BlockFence.canConnectTo`: its own kind of fence, any gate or an opaque cube.
pub(crate) fn fence_connects(id: u16, neighbour: u16) -> bool {
    let wood = |n| matches!(n, 85 | 188..=192);
    let same = if id == 113 {
        neighbour == 113
    } else {
        wood(neighbour)
    };
    same || is_gate(neighbour) || opaque_cube(neighbour)
}

/// `BlockChest.setBlockBoundsBasedOnState`: a chest next to a chest of its kind reaches the
/// shared edge. `joined` is north, south, west, east.
fn chest_box(joined: [bool; 4]) -> [f64; 6] {
    let [n, s, w, e] = joined;
    [
        if w { 0.0 } else { 0.0625 },
        0.0,
        if n { 0.0 } else { 0.0625 },
        if e { 1.0 } else { 0.9375 },
        0.875,
        if s { 1.0 } else { 0.9375 },
    ]
}

/// `BlockFence.setBlockBoundsBasedOnState`: a post that reaches the edge of every joined side.
fn fence_box(joined: [bool; 4]) -> [f64; 6] {
    let [n, s, w, e] = joined;
    [
        if w { 0.0 } else { 0.375 },
        0.0,
        if n { 0.0 } else { 0.375 },
        if e { 1.0 } else { 0.625 },
        1.0,
        if s { 1.0 } else { 0.625 },
    ]
}

/// `BlockPane.setBlockBoundsBasedOnState`: a thin post; a straight or lone pane spans the cell.
fn pane_box(joined: [bool; 4]) -> [f64; 6] {
    let [n, s, w, e] = joined;
    let any = n || s || w || e;
    let (x0, x1) = pane_span(w, e, any);
    let (z0, z1) = pane_span(n, s, any);
    [x0, 0.0, z0, x1, 1.0, z1]
}

fn pane_span(low: bool, high: bool, any: bool) -> (f64, f64) {
    match (low, high) {
        (true, false) => (0.0, 0.5625),
        (false, true) => (0.4375, 1.0),
        (false, false) if any => (0.4375, 0.5625),
        _ => (0.0, 1.0),
    }
}

/// `BlockWall.setBlockBoundsBasedOnState`: a post, lower and slimmer on a straight run.
fn wall_box(joined: [bool; 4]) -> [f64; 6] {
    let [n, s, w, e] = joined;
    let pick = |j: bool, edge: f64| if j { edge } else { 0.25 + 0.5 * edge };
    let (mut x0, mut x1, mut z0, mut z1, mut top) =
        (pick(w, 0.0), pick(e, 1.0), pick(n, 0.0), pick(s, 1.0), 1.0);
    if joined == [true, true, false, false] {
        (top, x0, x1) = (0.8125, 0.3125, 0.6875);
    } else if joined == [false, false, true, true] {
        (top, z0, z1) = (0.8125, 0.3125, 0.6875);
    }
    [x0, 0.0, z0, x1, top, z1]
}

/// `BlockVine.setBlockBoundsBasedOnState`: a 1/16 slab on each side the vine clings to, and one
/// under a solid block above; a vine with neither shows the slab under the block above.
fn vine_box(meta: u16, up: bool) -> [f64; 6] {
    let slabs = [
        (1, [0.0, 0.0, 0.9375, 1.0, 1.0, 1.0]),
        (2, [0.0, 0.0, 0.0, 0.0625, 1.0, 1.0]),
        (4, [0.0, 0.0, 0.0, 1.0, 1.0, 0.0625]),
        (8, [0.9375, 0.0, 0.0, 1.0, 1.0, 1.0]),
        (16, [0.0, 0.9375, 0.0, 1.0, 1.0, 1.0]),
    ];
    let clings = meta | if up { 16 } else { 0 };
    let mut sides = slabs
        .iter()
        .filter(|(bit, _)| clings & bit != 0)
        .map(|s| s.1);
    let Some(first) = sides.next() else {
        return [0.0, 0.9375, 0.0, 1.0, 1.0, 1.0];
    };
    sides.fold(first, |a, b| {
        std::array::from_fn(|i| {
            if i < 3 {
                a[i].min(b[i])
            } else {
                a[i].max(b[i])
            }
        })
    })
}

fn state_box(state: u16) -> Option<[f64; 6]> {
    let (id, meta) = (state >> 4, state & 15);
    if matches!(id, 0 | 8..=11 | 51) {
        return None;
    }
    let full = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    Some(
        mounted_box(id, meta)
            .or_else(|| flat_box(id, meta))
            .or_else(|| standing_box(id))
            .or_else(|| crop_box(id, meta))
            .unwrap_or(full),
    )
}

/// Blocks attached to a side or face of the neighbour.
fn mounted_box(id: u16, meta: u16) -> Option<[f64; 6]> {
    Some(match id {
        50 | 75 | 76 => torch_box(meta),
        65 => ladder_box(meta),
        68 => wall_sign_box(meta),
        69 => lever_box(meta & 7),
        77 | 143 => button_box(meta),
        96 | 167 => trapdoor_box(meta),
        107 | 183..=187 => gate_box(meta),
        90 => portal_box(meta),
        _ => return None,
    })
}

fn torch_box(meta: u16) -> [f64; 6] {
    match meta {
        1 => [0.0, 0.2, 0.35, 0.3, 0.8, 0.65],
        2 => [0.7, 0.2, 0.35, 1.0, 0.8, 0.65],
        3 => [0.35, 0.2, 0.0, 0.65, 0.8, 0.3],
        4 => [0.35, 0.2, 0.7, 0.65, 0.8, 1.0],
        _ => [0.4, 0.0, 0.4, 0.6, 0.6, 0.6],
    }
}

pub(crate) fn ladder_box(meta: u16) -> [f64; 6] {
    match meta {
        2 => [0.0, 0.0, 0.875, 1.0, 1.0, 1.0],
        3 => [0.0, 0.0, 0.0, 1.0, 1.0, 0.125],
        4 => [0.875, 0.0, 0.0, 1.0, 1.0, 1.0],
        _ => [0.0, 0.0, 0.0, 0.125, 1.0, 1.0],
    }
}

fn wall_sign_box(meta: u16) -> [f64; 6] {
    match meta {
        2 => [0.0, 0.28125, 0.875, 1.0, 0.78125, 1.0],
        3 => [0.0, 0.28125, 0.0, 1.0, 0.78125, 0.125],
        4 => [0.875, 0.28125, 0.0, 1.0, 0.78125, 1.0],
        _ => [0.0, 0.28125, 0.0, 0.125, 0.78125, 1.0],
    }
}

fn lever_box(facing: u16) -> [f64; 6] {
    match facing {
        1 => [0.0, 0.2, 0.3125, 0.375, 0.8, 0.6875],
        2 => [0.625, 0.2, 0.3125, 1.0, 0.8, 0.6875],
        3 => [0.3125, 0.2, 0.0, 0.6875, 0.8, 0.375],
        4 => [0.3125, 0.2, 0.625, 0.6875, 0.8, 1.0],
        5 | 6 => [0.25, 0.0, 0.25, 0.75, 0.6, 0.75],
        _ => [0.25, 0.4, 0.25, 0.75, 1.0, 0.75],
    }
}

fn button_box(meta: u16) -> [f64; 6] {
    let t = if meta & 8 != 0 { 0.0625 } else { 0.125 };
    match meta & 7 {
        1 => [0.0, 0.375, 0.3125, t, 0.625, 0.6875],
        2 => [1.0 - t, 0.375, 0.3125, 1.0, 0.625, 0.6875],
        3 => [0.3125, 0.375, 0.0, 0.6875, 0.625, t],
        4 => [0.3125, 0.375, 1.0 - t, 0.6875, 0.625, 1.0],
        5 => [0.3125, 0.0, 0.375, 0.6875, t, 0.625],
        _ => [0.3125, 1.0 - t, 0.375, 0.6875, 1.0, 0.625],
    }
}

fn trapdoor_box(meta: u16) -> [f64; 6] {
    let open = [
        [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.1875],
        [0.8125, 0.0, 0.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 0.1875, 1.0, 1.0],
    ];
    if meta & 4 != 0 {
        open[usize::from(meta & 3)]
    } else if meta & 8 != 0 {
        [0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]
    }
}

/// Blocks lying flat on the ground or filling only part of the height.
fn flat_box(id: u16, meta: u16) -> Option<[f64; 6]> {
    let top = |h: f64| [0.0, 0.0, 0.0, 1.0, h, 1.0];
    let inset = |x: f64, h: f64| [x, 0.0, x, 1.0 - x, h, 1.0 - x];
    Some(match id {
        27 | 28 | 66 | 157 => top(rail_height(meta & 7)),
        44 | 126 | 182 if meta & 8 != 0 => [0.0, 0.5, 0.0, 1.0, 1.0, 1.0],
        44 | 126 | 182 => top(0.5),
        70 | 72 | 147 | 148 => inset(0.0625, if meta > 0 { 0.03125 } else { 0.0625 }),
        78 => top(f64::from((meta & 7) + 1) / 8.0),
        92 => [
            f64::from(1 + 2 * meta) / 16.0,
            0.0,
            0.0625,
            0.9375,
            0.5,
            0.9375,
        ],
        111 => top(0.015625),
        _ => return fixed_height(id).map(top),
    })
}

/// Rails rise to 5/8 on a slope and lie 1/8 high on the flat.
fn rail_height(shape: u16) -> f64 {
    if (2..=5).contains(&shape) {
        0.625
    } else {
        0.125
    }
}

/// Height of the blocks that fill their cell floor to a height fixed by their id.
fn fixed_height(id: u16) -> Option<f64> {
    Some(match id {
        26 => 0.5625,
        55 | 119 => 0.0625,
        60 => 0.9375,
        93 | 94 | 149 | 150 => 0.125,
        116 => 0.75,
        120 => 0.8125,
        151 | 178 => 0.375,
        171 => 0.0625,
        _ => return None,
    })
}

/// The nether portal pane is thin across its axis: meta 1 runs along x, meta 2 along z.
fn portal_box(meta: u16) -> [f64; 6] {
    if meta == 1 {
        [0.0, 0.0, 0.375, 1.0, 1.0, 0.625]
    } else {
        [0.375, 0.0, 0.0, 0.625, 1.0, 1.0]
    }
}

fn gate_box(meta: u16) -> [f64; 6] {
    if meta & 1 == 0 {
        [0.0, 0.0, 0.375, 1.0, 1.0, 0.625]
    } else {
        [0.375, 0.0, 0.0, 0.625, 1.0, 1.0]
    }
}

/// Crops and nether wart are 1/4 high; stems grow from 2/16 to 16/16 with their age.
fn crop_box(id: u16, meta: u16) -> Option<[f64; 6]> {
    Some(match id {
        59 | 115 | 141 | 142 => [0.0, 0.0, 0.0, 1.0, 0.25, 1.0],
        104 | 105 => [
            0.375,
            0.0,
            0.375,
            0.625,
            f64::from((meta & 7) * 2 + 2) / 16.0,
            0.625,
        ],
        _ => return None,
    })
}

/// Plants and posts that stand in the middle of their cell.
fn standing_box(id: u16) -> Option<[f64; 6]> {
    Some(match id {
        6 | 31 | 32 => [0.1, 0.0, 0.1, 0.9, 0.8, 0.9],
        37 | 38 => [0.3, 0.0, 0.3, 0.7, 0.6, 0.7],
        39 | 40 => [0.3, 0.0, 0.3, 0.7, 0.4, 0.7],
        63 => [0.25, 0.0, 0.25, 0.75, 1.0, 0.75],
        81 => [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375],
        83 => [0.125, 0.0, 0.125, 0.875, 1.0, 0.875],
        140 => [0.3125, 0.0, 0.3125, 0.6875, 0.375, 0.6875],
        130 => [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375],
        122 => [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375],
        _ => return None,
    })
}

/// A block under the crosshair: its cell and the face the ray enters (0 down, 1 up, 2 north,
/// 3 south, 4 west, 5 east: the `EnumFacing` index that PlayerDigging carries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub pos: [i32; 3],
    pub face: u8,
}

/// `Entity.getVectorForRotation`, with the vanilla sine table.
pub fn look_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let k = 0.017453292_f32;
    let f = cos(-yaw * k - std::f32::consts::PI);
    let f1 = sin(-yaw * k - std::f32::consts::PI);
    let f2 = -cos(-pitch * k);
    let f3 = sin(-pitch * k);
    [f64::from(f1 * f2), f64::from(f3), f64::from(f * f2)]
}

/// `PlayerControllerMP.getBlockReachDistance`.
pub fn reach(creative: bool) -> f64 {
    if creative {
        5.0
    } else {
        4.5
    }
}

/// Entry distance and face of the ray `eye + t * dir` into a box, if it enters from outside.
fn enter_box(eye: [f64; 3], dir: [f64; 3], cell: [i32; 3], b: [f64; 6]) -> Option<(f64, u8)> {
    let (mut near, mut far, mut face) = (f64::NEG_INFINITY, f64::INFINITY, 0);
    for a in 0..3 {
        let (lo, hi) = (f64::from(cell[a]) + b[a], f64::from(cell[a]) + b[a + 3]);
        if dir[a] == 0.0 {
            if eye[a] < lo || eye[a] > hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo - eye[a]) / dir[a], (hi - eye[a]) / dir[a]);
        let (t_in, t_out) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if t_in > near {
            near = t_in;
            // EnumFacing index of the face the ray enters: x west/east, y down/up, z north/south.
            face = [4, 0, 2][a] + u8::from(dir[a] < 0.0);
        }
        far = far.min(t_out);
    }
    (near >= 0.0 && near <= far).then_some((near, face))
}

/// First selectable block along the look ray within `reach`, walking the cells in ray order.
pub fn raycast(
    state: &dyn Fn([i32; 3]) -> u16,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    reach: f64,
) -> Option<Hit> {
    raycast_dist(state, eye, yaw, pitch, reach).map(|(hit, _)| hit)
}

/// [`raycast`] plus the distance from `eye` to the hit point.
pub fn raycast_dist(
    state: &dyn Fn([i32; 3]) -> u16,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    reach: f64,
) -> Option<(Hit, f64)> {
    let dir = look_vector(yaw, pitch);
    let mut cell = eye.map(|v| v.floor() as i32);
    let step = dir.map(|d| if d < 0.0 { -1 } else { 1 });
    let delta = dir.map(|d| {
        if d == 0.0 {
            f64::INFINITY
        } else {
            (1.0 / d).abs()
        }
    });
    let mut t_max = [0.0; 3];
    for a in 0..3 {
        t_max[a] = if dir[a] == 0.0 {
            f64::INFINITY
        } else {
            let edge = f64::from(cell[a] + i32::from(dir[a] > 0.0));
            (edge - eye[a]) / dir[a]
        };
    }
    loop {
        if let Some(b) = selection_box(state, cell) {
            if let Some((t, face)) = enter_box(eye, dir, cell, b) {
                if t <= reach {
                    return Some((Hit { pos: cell, face }, t));
                }
            }
        }
        let a = (0..3)
            .min_by(|&i, &j| t_max[i].total_cmp(&t_max[j]))
            .unwrap_or(0);
        if t_max[a] > reach {
            return None;
        }
        cell[a] += step[a];
        t_max[a] += delta[a];
    }
}

/// `PlayerDigging` status byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Start = 0,
    Abort = 1,
    Stop = 2,
}

/// What the dig state machine asks the client to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Dig {
        status: Status,
        pos: [i32; 3],
        face: u8,
    },
    /// The `Animation` arm swing packet.
    Swing,
    /// `UseEntity` attack on the entity under the crosshair.
    Attack(i32),
}

/// The loaded world as the dig state machine sees it.
pub trait DigWorld {
    fn state(&self, pos: [i32; 3]) -> u16;
    /// Replace the block with air, locally. The server's `BlockChange` stays the truth.
    fn remove(&mut self, pos: [i32; 3]);
}

/// Inputs of one client tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Attack key is down with the game focused.
    pub held: bool,
    /// Attack key presses since the last tick.
    pub presses: u32,
    /// The mouse was grabbed by a click this tick: `setIngameFocus`.
    pub grabbed: bool,
    pub creative: bool,
    /// Adventure or spectator: `clickBlock` refuses, but the swing still happens.
    pub no_dig: bool,
    pub on_ground: bool,
    pub target: Option<Hit>,
    /// The entity under the crosshair; a click attacks it and `target` is then `None`.
    pub entity: Option<i32>,
}

/// `PlayerControllerMP` dig fields plus `Minecraft.leftClickCounter`.
#[derive(Debug, Default)]
pub struct Digger {
    hitting: bool,
    current: [i32; 3],
    progress: f32,
    hit_delay: i32,
    click_delay: i32,
}

impl Digger {
    /// `PlayerControllerMP.getIsHittingBlock`.
    pub fn is_hitting(&self) -> bool {
        self.hitting
    }

    /// The block being dug and its destroy stage `(int)(curBlockDamageMP * 10) - 1`, once that is 0 or more.
    pub fn stage(&self) -> Option<([i32; 3], u8)> {
        let stage = (self.progress * 10.0) as i32 - 1;
        (self.hitting && (0..10).contains(&stage)).then_some((self.current, stage as u8))
    }
}

/// Ticks to wait after a creative break or a finished dig: `blockHitDelay`.
const HIT_DELAY: i32 = 5;
/// `leftClickCounter` after a swing at nothing in survival.
const MISS_DELAY: i32 = 10;
/// `leftClickCounter` after the click that grabs the mouse: blocks digging until the button is
/// released, which resets it.
const FOCUS_DELAY: i32 = 10000;

impl Digger {
    /// One client tick, in vanilla order: focus, counter, clicks, then the held key.
    pub fn tick(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if f.grabbed {
            self.click_delay = FOCUS_DELAY;
        }
        if self.click_delay > 0 {
            self.click_delay -= 1;
        }
        for _ in 0..f.presses {
            self.click_mouse(f, w, out);
        }
        self.send_click_block(f, w, out);
    }

    fn click_mouse(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if self.click_delay > 0 {
            return;
        }
        out.push(Action::Swing);
        if let Some(id) = f.entity {
            out.push(Action::Attack(id));
            return;
        }
        match f.target {
            Some(hit) if w.state(hit.pos) >> 4 != 0 => self.click_block(f, hit, w, out),
            _ if !f.creative => self.click_delay = MISS_DELAY,
            _ => {}
        }
    }

    fn send_click_block(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if !f.held {
            self.click_delay = 0;
        }
        if self.click_delay > 0 {
            return;
        }
        match f.target {
            Some(hit) if f.held => {
                if w.state(hit.pos) >> 4 != 0 && self.damage_block(f, hit, w, out) {
                    out.push(Action::Swing);
                }
            }
            _ => self.reset_block_removing(out),
        }
    }

    /// `PlayerControllerMP.clickBlock`.
    fn click_block(&mut self, f: &Frame, hit: Hit, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if f.no_dig {
            return;
        }
        let start = |status, pos| Action::Dig {
            status,
            pos,
            face: hit.face,
        };
        if f.creative {
            out.push(start(Status::Start, hit.pos));
            self.creative_break(hit, w);
            self.hit_delay = HIT_DELAY;
        } else if !self.hitting || self.current != hit.pos {
            if self.hitting {
                out.push(start(Status::Abort, self.current));
            }
            out.push(start(Status::Start, hit.pos));
            let id = w.state(hit.pos) >> 4;
            if id != 0 && relative_hardness(id, f.on_ground) >= 1.0 {
                self.destroy(hit.pos, w);
            } else {
                self.hitting = true;
                self.current = hit.pos;
                self.progress = 0.0;
            }
        }
    }

    /// `PlayerControllerMP.onPlayerDamageBlock`: true when the swing counts.
    fn damage_block(
        &mut self,
        f: &Frame,
        hit: Hit,
        w: &mut impl DigWorld,
        out: &mut Vec<Action>,
    ) -> bool {
        if f.no_dig {
            return false;
        }
        if self.hit_delay > 0 {
            self.hit_delay -= 1;
            return true;
        }
        if f.creative {
            self.hit_delay = HIT_DELAY;
            out.push(Action::Dig {
                status: Status::Start,
                pos: hit.pos,
                face: hit.face,
            });
            self.creative_break(hit, w);
        } else if self.hitting && self.current == hit.pos {
            let id = w.state(hit.pos) >> 4;
            if id == 0 {
                self.hitting = false;
                return false;
            }
            self.progress += relative_hardness(id, f.on_ground);
            if self.progress >= 1.0 {
                self.hitting = false;
                out.push(Action::Dig {
                    status: Status::Stop,
                    pos: hit.pos,
                    face: hit.face,
                });
                self.destroy(hit.pos, w);
                self.progress = 0.0;
                self.hit_delay = HIT_DELAY;
            }
        } else {
            self.click_block(f, hit, w, out);
        }
        true
    }

    /// `PlayerControllerMP.resetBlockRemoving`.
    fn reset_block_removing(&mut self, out: &mut Vec<Action>) {
        if self.hitting {
            out.push(Action::Dig {
                status: Status::Abort,
                pos: self.current,
                face: 0,
            });
            self.hitting = false;
            self.progress = 0.0;
        }
    }

    /// `PlayerControllerMP.clickBlockCreative`: `World.extinguishFire` puts out fire beside the
    /// clicked face and leaves the block; otherwise the block goes.
    fn creative_break(&mut self, hit: Hit, w: &mut impl DigWorld) {
        let [x, y, z] = hit.pos;
        let fire = match hit.face {
            0 => [x, y - 1, z],
            1 => [x, y + 1, z],
            2 => [x, y, z - 1],
            3 => [x, y, z + 1],
            4 => [x - 1, y, z],
            _ => [x + 1, y, z],
        };
        if w.state(fire) >> 4 == 51 {
            w.remove(fire);
        } else {
            self.destroy(hit.pos, w);
        }
    }

    /// `PlayerControllerMP.onPlayerDestroyBlock`: the block goes at once, the server confirms.
    fn destroy(&mut self, pos: [i32; 3], w: &mut impl DigWorld) {
        if w.state(pos) >> 4 == 0 {
            return;
        }
        w.remove(pos);
        self.current[1] = -1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Blocks(HashMap<[i32; 3], u16>);

    impl DigWorld for Blocks {
        fn state(&self, pos: [i32; 3]) -> u16 {
            self.0.get(&pos).copied().unwrap_or(0)
        }
        fn remove(&mut self, pos: [i32; 3]) {
            self.0.remove(&pos);
        }
    }

    const GRASS: u16 = 2 << 4;
    const STONE: u16 = 1 << 4;
    const AT: Hit = Hit {
        pos: [0, 63, 0],
        face: 1,
    };

    fn held(creative: bool) -> Frame {
        Frame {
            held: true,
            creative,
            on_ground: true,
            target: Some(AT),
            ..Frame::default()
        }
    }

    /// Ticks from the press to the `Stop` packet, counting the press tick as tick 1.
    fn ticks_to_break(state: u16) -> Option<u32> {
        let mut w = Blocks(HashMap::from([(AT.pos, state)]));
        let mut d = Digger::default();
        let mut out = vec![];
        for n in 1..=2000 {
            let f = Frame {
                presses: u32::from(n == 1),
                ..held(false)
            };
            d.tick(&f, &mut w, &mut out);
            if out.iter().any(|a| {
                matches!(
                    a,
                    Action::Dig {
                        status: Status::Stop,
                        ..
                    }
                )
            }) {
                return Some(n);
            }
        }
        None
    }

    #[test]
    fn bare_hand_break_times_follow_hardness() {
        // The press tick already adds one tick of progress, like vanilla (clickMouse, then the held key).
        // Grass 0.6: 1/0.6/30 per tick. Stone 1.5 needs a tool: 1/1.5/100 per tick.
        assert_eq!(ticks_to_break(GRASS), Some(18));
        assert_eq!(ticks_to_break(STONE), Some(151));
        assert_eq!(ticks_to_break(7 << 4), None, "bedrock never breaks");
    }

    #[test]
    fn instant_blocks_break_on_the_press() {
        let mut w = Blocks(HashMap::from([(AT.pos, 31 << 4)]));
        let mut out = vec![];
        let f = Frame {
            presses: 1,
            ..held(false)
        };
        Digger::default().tick(&f, &mut w, &mut out);
        assert_eq!(w.state(AT.pos), 0);
        assert!(out.contains(&Action::Dig {
            status: Status::Start,
            pos: AT.pos,
            face: 1
        }));
    }

    #[test]
    fn creative_breaks_with_a_five_tick_delay() {
        let mut w = Blocks::default();
        let mut d = Digger::default();
        let mut broken = vec![];
        for n in 0..14 {
            // Refill the cell so each break is visible.
            if w.state(AT.pos) == 0 {
                w.0.insert(AT.pos, STONE);
            }
            let f = Frame {
                presses: u32::from(n == 0),
                ..held(true)
            };
            d.tick(&f, &mut w, &mut vec![]);
            if w.state(AT.pos) == 0 {
                broken.push(n);
            }
        }
        assert_eq!(broken, [0, 6, 12]);
    }

    #[test]
    fn releasing_aborts_the_dig_and_the_grab_click_does_not_dig() {
        let mut w = Blocks(HashMap::from([(AT.pos, GRASS)]));
        let mut d = Digger::default();
        let mut out = vec![];
        let press = Frame {
            presses: 1,
            ..held(false)
        };
        d.tick(&press, &mut w, &mut out);
        d.tick(&held(false), &mut w, &mut out);
        out.clear();
        d.tick(&Frame::default(), &mut w, &mut out);
        assert_eq!(
            out,
            [Action::Dig {
                status: Status::Abort,
                pos: AT.pos,
                face: 0
            }]
        );

        // The click that grabbed the mouse blocks digging until the button is released.
        let mut out = vec![];
        let grab = Frame {
            grabbed: true,
            ..held(false)
        };
        d.tick(&grab, &mut w, &mut out);
        for _ in 0..30 {
            d.tick(&held(false), &mut w, &mut out);
        }
        assert!(out.is_empty());
        d.tick(&Frame::default(), &mut w, &mut out);
        d.tick(&held(false), &mut w, &mut out);
        assert!(!out.is_empty());
    }

    /// Every block's hardness, tool requirement and empty-hand speed against the real client.
    #[test]
    fn hardness_and_hand_speed_match_the_real_client() {
        let mut script = String::from("start 0 64 0 0\n");
        for id in 0..HARDNESS.len() {
            script += &format!("mine {id}\n");
        }
        let lines = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(lines.len(), HARDNESS.len());
        let mut wrong = vec![];
        for (id, line) in lines.iter().enumerate() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let bits = |s: &str| u32::from_str_radix(s, 16).unwrap();
            let id = id as u16;
            let got = (
                hardness(id).to_bits(),
                relative_hardness(id, true).to_bits(),
                relative_hardness(id, false).to_bits(),
                u32::from(!requires_tool(id)),
            );
            let want = (bits(f[0]), bits(f[1]), bits(f[2]), bits(f[3]));
            if got != want {
                wrong.push(format!(
                    "id {id}: hardness {} want {}, requires_tool {} want {}",
                    hardness(id),
                    f32::from_bits(want.0),
                    requires_tool(id),
                    want.3 == 0
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    fn cell_state(pos: [i32; 3]) -> u16 {
        if pos[1] <= 63 {
            STONE
        } else if pos == [3, 65, 0] {
            // A flower: its small box is missed by a ray through the cell's corner.
            37 << 4
        } else {
            0
        }
    }

    #[test]
    fn ray_hits_the_first_block_face_within_reach() {
        // Straight down from 1.62 above the floor top: top face of the floor block.
        let hit = raycast(&cell_state, [0.5, 65.62, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(
            hit,
            Some(Hit {
                pos: [0, 63, 0],
                face: 1
            })
        );
        // Yaw 0 looks toward +z: a wall at z = 4 is entered through its north face.
        let wall = |p: [i32; 3]| if p[2] == 4 { STONE } else { 0 };
        let hit = raycast(&wall, [0.5, 65.0, 0.5], 0.0, 0.0, 4.5);
        assert_eq!(
            hit,
            Some(Hit {
                pos: [0, 65, 4],
                face: 2
            })
        );
        // Reach 4.5 stops 3.5 blocks away from the wall face at z = 4 only when it is closer.
        assert_eq!(raycast(&wall, [0.5, 65.0, -1.0], 0.0, 0.0, 4.5), None);
        assert!(raycast(&wall, [0.5, 65.0, -1.0], 0.0, 0.0, 5.5).is_some());
        // Looking straight up at open sky hits nothing.
        assert_eq!(
            raycast(&cell_state, [0.5, 65.62, 0.5], 0.0, -90.0, 4.5),
            None
        );
    }

    #[test]
    fn ray_passes_air_and_liquid_and_hits_a_slab_half() {
        let water_over_stone = |p: [i32; 3]| match p[1] {
            64 => 9 << 4,
            ..=63 => STONE,
            _ => 0,
        };
        let hit = raycast(&water_over_stone, [0.5, 66.0, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 63, 0]));
        // A bottom slab is only half a block tall: a level ray at y 64.7 misses it.
        let slab = |p: [i32; 3]| if p == [0, 64, 2] { 44 << 4 } else { 0 };
        assert_eq!(raycast(&slab, [0.5, 64.7, 0.5], 0.0, 0.0, 4.5), None);
        assert!(raycast(&slab, [0.5, 64.3, 0.5], 0.0, 0.0, 4.5).is_some());
    }

    #[test]
    fn ray_passes_fire_and_the_empty_part_of_a_wall_torch() {
        // Fire (51) is not collidable: the ray targets the stone under it, entering through the top.
        let fire = |p: [i32; 3]| match p[1] {
            64 => 51 << 4,
            ..=63 => STONE,
            _ => 0,
        };
        let hit = raycast(&fire, [0.5, 66.0, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(hit.map(|h| (h.pos, h.face)), Some(([0, 63, 0], 1)));
        // A torch on the west wall of its cell (meta 1) fills x 0..0.3: a ray down x = 0.8 misses it.
        let torch = |p: [i32; 3]| match p {
            [0, 64, 0] => (50 << 4) | 1,
            [0, ..=63, 0] => STONE,
            _ => 0,
        };
        let hit = raycast(&torch, [0.8, 66.0, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 63, 0]));
        let hit = raycast(&torch, [0.15, 66.0, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 64, 0]));
    }

    #[test]
    fn door_leaf_is_thin_and_read_from_both_halves() {
        // Closed door with facing 0: the leaf fills x 0..0.1875 of the cell.
        let door = |p: [i32; 3]| match p {
            [0, 64, 2] => 64 << 4,
            [0, 65, 2] => (64 << 4) | 8,
            _ => 0,
        };
        assert_eq!(raycast(&door, [0.8, 64.5, 0.5], 0.0, 0.0, 4.5), None);
        let hit = raycast(&door, [0.1, 65.5, 0.5], 0.0, 0.0, 4.5);
        assert_eq!(hit.map(|h| (h.pos, h.face)), Some(([0, 65, 2], 2)));
    }

    #[test]
    fn ray_passes_over_a_young_crop() {
        // Wheat (59) is 1/4 high: a level ray at y 64.5 passes over it, one at 64.1 hits it.
        let wheat = |p: [i32; 3]| if p == [0, 64, 2] { 59 << 4 } else { 0 };
        assert_eq!(raycast(&wheat, [0.5, 64.5, 0.5], 0.0, 0.0, 4.5), None);
        assert!(raycast(&wheat, [0.5, 64.1, 0.5], 0.0, 0.0, 4.5).is_some());
    }

    #[test]
    fn fence_reaches_its_joined_side_and_a_lone_pane_spans_the_cell() {
        // A ray along +z at x 0.9 passes a lone fence post (x 0.375..0.625) but hits one joined to stone on the east.
        let lone = |p: [i32; 3]| if p == [0, 64, 2] { 85 << 4 } else { 0 };
        assert_eq!(raycast(&lone, [0.9, 64.5, 0.5], 0.0, 0.0, 4.5), None);
        let joined = |p: [i32; 3]| match p {
            [0, 64, 2] => 85 << 4,
            [1, 64, 2] => STONE,
            _ => 0,
        };
        let hit = raycast(&joined, [0.9, 64.5, 0.5], 0.0, 0.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 64, 2]));
        // A lone glass pane (102) spans its whole cell, like a straight one.
        let pane = |p: [i32; 3]| if p == [0, 64, 2] { 102 << 4 } else { 0 };
        assert!(raycast(&pane, [0.9, 64.5, 0.5], 0.0, 0.0, 4.5).is_some());
    }

    #[test]
    fn chest_stretches_toward_a_chest_next_to_it() {
        // West neighbour chest: the box starts at x 0 instead of 0.0625.
        let at_edge = [0.03, 64.5, 0.5];
        let single = |p: [i32; 3]| if p == [0, 64, 2] { 54 << 4 } else { 0 };
        assert_eq!(raycast(&single, at_edge, 0.0, 0.0, 4.5), None);
        let double = |p: [i32; 3]| match p {
            [0, 64, 2] | [-1, 64, 2] => 54 << 4,
            _ => 0,
        };
        let hit = raycast(&double, at_edge, 0.0, 0.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 64, 2]));
    }

    #[test]
    fn creative_punch_puts_out_fire_and_keeps_the_block() {
        // Fire above netherrack: the ray lands on the netherrack's top face, the server only extinguishes.
        let mut w = Blocks(HashMap::from([(AT.pos, 87 << 4), ([0, 64, 0], 51 << 4)]));
        let mut out = vec![];
        let f = Frame {
            presses: 1,
            ..held(true)
        };
        Digger::default().tick(&f, &mut w, &mut out);
        assert_eq!(w.state(AT.pos), 87 << 4);
        assert_eq!(w.state([0, 64, 0]), 0);
        assert!(out.contains(&Action::Dig {
            status: Status::Start,
            pos: AT.pos,
            face: 1
        }));
    }

    #[test]
    fn fence_does_not_join_a_melon() {
        // A fence post beside a melon stays a post: a ray along +z at x 0.9 passes it.
        let at = |p: [i32; 3]| match p {
            [0, 64, 2] => 85 << 4,
            [1, 64, 2] => 103 << 4,
            _ => 0,
        };
        assert_eq!(raycast(&at, [0.9, 64.5, 0.5], 0.0, 0.0, 4.5), None);
    }
}
