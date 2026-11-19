//! `(blockstate, variant key)` of a block state: the key lists the properties in the order of the JSON files.

use super::WOOD_DOORS;

type Nb<'a> = &'a dyn Fn(i32, i32, i32) -> u16;
type Found = Option<(&'static str, String)>;

const HORIZONTAL: [&str; 4] = ["south", "west", "north", "east"];
const FRONT: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

pub(super) fn variant(state: u16, nb: Nb) -> Found {
    let (id, meta) = (state >> 4, state as usize & 15);
    let facing = HORIZONTAL[meta & 3];
    match id {
        50 | 75 | 76 => torch(id, meta),
        93 | 94 => repeater(id, meta, facing),
        149 | 150 => comparator(id, meta, facing),
        127 => cocoa(meta, facing),
        145 => anvil(meta, facing),
        34 => piston_head(meta),
        69 => lever(meta),
        70 | 72 => pressure_plate(id, meta),
        147 | 148 => weighted_plate(id, meta),
        65 => ladder(meta),
        120 => end_portal_frame(meta, facing),
        131 => tripwire_hook(meta, facing),
        64 | 71 | 193..=197 => door(id, state, nb),
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => stairs(id, meta, nb),
        107 | 183..=187 => fence_gate(id, meta, facing, nb),
        139 => wall(meta, nb),
        26 => Some((
            "bed",
            format!("facing={facing},part={}", ["foot", "head"][meta >> 3 & 1]),
        )),
        _ => mechanism(id, meta, nb),
    }
}

fn mechanism(id: u16, meta: usize, nb: Nb) -> Found {
    match id {
        29 | 33 => piston(id, meta),
        27 | 28 | 66 | 157 => rail(id, meta),
        55 => redstone_wire(nb),
        _ => plant_or_block(id, meta, nb),
    }
}

fn plant_or_block(id: u16, meta: usize, nb: Nb) -> Found {
    match id {
        81 => Some(("cactus", "normal".into())),
        138 => Some(("beacon", "normal".into())),
        106 => Some(("vine", vine_key(meta, nb))),
        140 | 255 => flower_pot(id, meta),
        _ => None,
    }
}

/// `BlockVine.getActualState`: `up` is set when the block above is a normal cube; the side bits are south 1, west 2, north 4, east 8.
fn vine_key(meta: usize, nb: Nb) -> String {
    let side = |bit| meta & bit != 0;
    format!(
        "east={},north={},south={},up={},west={}",
        side(8),
        side(4),
        side(1),
        crate::blocks::is_opaque_cube(nb(0, 1, 0)),
        side(2)
    )
}

/// The contents of a pot are in its renderer state (`blocks::flower_pot_state`).
fn flower_pot(id: u16, meta: usize) -> Found {
    let n = if id == 140 { meta } else { 16 + meta };
    let contents = crate::blocks::POT_CONTENTS.get(n)?;
    Some(("flower_pot", format!("contents={contents}")))
}

fn torch(id: u16, meta: usize) -> Found {
    let name = match id {
        50 => "torch",
        75 => "unlit_redstone_torch",
        _ => "redstone_torch",
    };
    let dir = ["up", "east", "west", "south", "north", "up"][meta.min(5)];
    Some((name, format!("facing={dir}")))
}

fn repeater(id: u16, meta: usize, facing: &str) -> Found {
    Some((
        ["unpowered_repeater", "powered_repeater"][usize::from(id == 94)],
        format!("delay={},facing={facing},locked=false", (meta >> 2) + 1),
    ))
}

fn comparator(id: u16, meta: usize, facing: &str) -> Found {
    Some((
        ["unpowered_comparator", "powered_comparator"][usize::from(id == 150)],
        format!(
            "facing={facing},mode={},powered={}",
            ["compare", "subtract"][meta >> 2 & 1],
            id == 150 || meta & 8 != 0
        ),
    ))
}

fn cocoa(meta: usize, facing: &str) -> Found {
    Some((
        "cocoa",
        format!("age={},facing={facing}", (meta >> 2).min(2)),
    ))
}

fn anvil(meta: usize, facing: &str) -> Found {
    Some((
        "anvil",
        format!("damage={},facing={facing}", (meta >> 2).min(2)),
    ))
}

fn piston_head(meta: usize) -> Found {
    Some((
        "piston_head",
        format!(
            "facing={},short=false,type={}",
            FRONT[(meta & 7) % 6],
            ["normal", "sticky"][meta >> 3]
        ),
    ))
}

/// `BlockPistonBase`: bit 8 is `extended`, the low bits the facing. Id 33 is the plain piston, 29 the sticky one.
fn piston(id: u16, meta: usize) -> Found {
    Some((
        ["sticky_piston", "piston"][usize::from(id == 33)],
        format!(
            "extended={},facing={}",
            meta & 8 != 0,
            FRONT[(meta & 7) % 6]
        ),
    ))
}

/// `BlockRailBase.EnumRailDirection` by metadata; only the plain rail has the four curves (6..9).
const RAIL_SHAPES: [&str; 10] = [
    "north_south",
    "east_west",
    "ascending_east",
    "ascending_west",
    "ascending_north",
    "ascending_south",
    "south_east",
    "south_west",
    "north_west",
    "north_east",
];

/// Rails 27 (powered), 28 (detector) and 157 (activator) carry `powered` in bit 8 and the shape in bits 0..2.
fn rail(id: u16, meta: usize) -> Found {
    if id == 66 {
        return Some(("rail", format!("shape={}", RAIL_SHAPES[meta.min(9)])));
    }
    let name = match id {
        27 => "golden_rail",
        28 => "detector_rail",
        _ => "activator_rail",
    };
    Some((
        name,
        format!("powered={},shape={}", meta & 8 != 0, RAIL_SHAPES[meta & 7]),
    ))
}

/// Horizontal offsets in the order of the wire properties `east, north, south, west`, with the
/// direction a neighbour in that offset is seen from the wire (`EnumFacing` index in `HORIZONTAL`).
const WIRE_SIDES: [(&str, [i32; 2], usize); 4] = [
    ("east", [1, 0], 3),
    ("north", [0, -1], 2),
    ("south", [0, 1], 0),
    ("west", [-1, 0], 1),
];

const WIRE_POWER_SOURCES: [u16; 17] = [
    28, 69, 70, 72, 75, 76, 77, 143, 131, 146, 147, 148, 149, 150, 151, 152, 178,
];

/// `BlockRedstoneWire.canConnectTo`: wire, a repeater lying along the line, or any block that
/// provides power. `side` is the `HORIZONTAL` index of the direction from the wire, `None` for the
/// check from above or below.
fn wire_connects(state: u16, side: Option<usize>) -> bool {
    let id = state >> 4;
    match id {
        55 => true,
        93 | 94 => side.is_some_and(|s| {
            let facing = state as usize & 3;
            facing == s || facing == (s + 2) % 4
        }),
        // Torches, lever, buttons, plates, tripwire hook, detector rail, trapped chest,
        // redstone block, repeaters/comparators, daylight sensors.
        _ => side.is_some() && WIRE_POWER_SOURCES.contains(&id),
    }
}

fn is_opaque(state: u16) -> bool {
    crate::blocks::look(state).kind == crate::blocks::Kind::Opaque
}

/// `BlockRedstoneWire.getAttachPosition` for the neighbour at `[dx, dz]`.
fn wire_attach(nb: Nb, [dx, dz]: [i32; 2], side: usize) -> &'static str {
    let next = nb(dx, 0, dz);
    if wire_connects(next, Some(side)) {
        return "side";
    }
    if !is_opaque(next) && wire_connects(nb(dx, -1, dz), None) {
        return "side";
    }
    let climbs = is_opaque(next) && !is_opaque(nb(0, 1, 0)) && wire_connects(nb(dx, 1, dz), None);
    ["none", "up"][usize::from(climbs)]
}

/// `BlockRedstoneWire.getActualState`: per side `none`, `side` or `up`.
fn redstone_wire(nb: Nb) -> Found {
    let [e, n, s, w] = WIRE_SIDES.map(|(_, d, side)| wire_attach(nb, d, side));
    Some((
        "redstone_wire",
        format!("east={e},north={n},south={s},west={w}"),
    ))
}

fn lever(meta: usize) -> Found {
    let dir = [
        "down_x", "east", "west", "south", "north", "up_z", "up_x", "down_z",
    ][meta & 7];
    Some(("lever", format!("facing={dir},powered={}", meta & 8 != 0)))
}

fn pressure_plate(id: u16, meta: usize) -> Found {
    Some((
        ["stone_pressure_plate", "wooden_pressure_plate"][usize::from(id == 72)],
        format!("powered={}", meta == 1),
    ))
}

fn weighted_plate(id: u16, meta: usize) -> Found {
    Some((
        [
            "light_weighted_pressure_plate",
            "heavy_weighted_pressure_plate",
        ][usize::from(id == 148)],
        format!("power={meta}"),
    ))
}

fn ladder(meta: usize) -> Found {
    Some((
        "ladder",
        format!(
            "facing={}",
            ["north", "north", "north", "south", "west", "east"][meta.clamp(2, 5)]
        ),
    ))
}

fn end_portal_frame(meta: usize, facing: &str) -> Found {
    Some((
        "end_portal_frame",
        format!("eye={},facing={facing}", meta & 4 != 0),
    ))
}

fn tripwire_hook(meta: usize, facing: &str) -> Found {
    Some((
        "tripwire_hook",
        format!(
            "attached={},facing={facing},powered={},suspended=false",
            meta & 4 != 0,
            meta & 8 != 0
        ),
    ))
}

fn door(id: u16, state: u16, nb: Nb) -> Found {
    let name = match id {
        64 => "wooden_door",
        71 => "iron_door",
        _ => WOOD_DOORS[id as usize - 193],
    };
    Some((name, door_key(state, nb)))
}

/// `BlockDoor.getActualState`: the lower half has the facing and `open`, the upper the hinge; each reads the other.
fn door_key(state: u16, nb: Nb) -> String {
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

pub(super) const STAIRS: [&str; 13] = [
    "oak_stairs",
    "stone_stairs",
    "brick_stairs",
    "stone_brick_stairs",
    "nether_brick_stairs",
    "sandstone_stairs",
    "spruce_stairs",
    "birch_stairs",
    "jungle_stairs",
    "quartz_stairs",
    "acacia_stairs",
    "dark_oak_stairs",
    "red_sandstone_stairs",
];

pub(super) const GATES: [&str; 6] = [
    "fence_gate",
    "spruce_fence_gate",
    "birch_fence_gate",
    "jungle_fence_gate",
    "dark_oak_fence_gate",
    "acacia_fence_gate",
];

const STAIR_IDS: [u16; 13] = [
    53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180,
];

fn is_stairs(id: u16) -> bool {
    STAIR_IDS.contains(&id)
}

/// Stair facing of meta bits 0..2 (`BlockStairs.getStateFromMeta`) as `0 east, 1 west, 2 south, 3 north`.
const STAIR_FACING: [&str; 4] = ["east", "west", "south", "north"];
/// Block offset `(dx, dz)` of each stair facing.
const STEP: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const EAST: usize = 0;
const WEST: usize = 1;
const SOUTH: usize = 2;
const NORTH: usize = 3;

/// `(neighbour facing, offset that must not hold an equal stair, value for a bottom stair)` per own facing, from the
/// bytecode of `BlockStairs.func_176307_f` (outer corner, neighbour in front) and `func_176305_g` (inner, behind).
type Corner = [(usize, usize, u8); 2];
const OUTER: [Corner; 4] = [
    [(NORTH, SOUTH, 1), (SOUTH, NORTH, 2)],
    [(NORTH, SOUTH, 2), (SOUTH, NORTH, 1)],
    [(WEST, EAST, 2), (EAST, WEST, 1)],
    [(WEST, EAST, 1), (EAST, WEST, 2)],
];
const INNER: [Corner; 4] = [
    [(NORTH, NORTH, 1), (SOUTH, SOUTH, 2)],
    [(NORTH, NORTH, 2), (SOUTH, SOUTH, 1)],
    [(WEST, WEST, 2), (EAST, EAST, 1)],
    [(WEST, WEST, 1), (EAST, EAST, 2)],
];

/// `BlockStairs.getActualState`: the outer corner wins over the inner one; 1 is the right and 2 the left variant.
fn stairs(id: u16, meta: usize, nb: Nb) -> Found {
    let (facing, top) = (meta & 3, meta & 4 != 0);
    let stair = |side: usize| {
        let (dx, dz) = STEP[side];
        let s = nb(dx, 0, dz);
        (is_stairs(s >> 4) && (s as usize >> 2 & 1 == top as usize)).then_some(s as usize & 3)
    };
    let same = |side: usize| stair(side) == Some(facing);
    let find = |neighbour: usize, table: &Corner| {
        let f2 = stair(neighbour)?;
        table
            .iter()
            .find(|(n, off, _)| *n == f2 && !same(*off))
            .map(|(_, _, v)| if top { 3 - v } else { *v })
    };
    let opposite = [WEST, EAST, NORTH, SOUTH][facing];
    let shape = match (find(facing, &OUTER[facing]), find(opposite, &INNER[facing])) {
        (Some(v), _) => ["", "outer_right", "outer_left"][v as usize],
        (None, Some(v)) => ["", "inner_right", "inner_left"][v as usize],
        (None, None) => "straight",
    };
    Some((
        STAIRS[STAIR_IDS.iter().position(|i| *i == id).unwrap_or(0)],
        format!(
            "facing={},half={},shape={shape}",
            STAIR_FACING[facing],
            ["bottom", "top"][usize::from(top)]
        ),
    ))
}

/// `BlockFenceGate.getActualState`: `in_wall` when a cobblestone wall stands beside the gate's hinge axis.
fn fence_gate(id: u16, meta: usize, facing: &str, nb: Nb) -> Found {
    let name = match id {
        107 => GATES[0],
        _ => GATES[id as usize - 182],
    };
    let wall = |dx, dz| nb(dx, 0, dz) >> 4 == 139;
    let in_wall = if meta & 1 == 0 {
        wall(-1, 0) || wall(1, 0)
    } else {
        wall(0, -1) || wall(0, 1)
    };
    Some((
        name,
        format!("facing={facing},in_wall={in_wall},open={}", meta & 4 != 0),
    ))
}

/// `BlockWall.canConnectTo`: walls and gates, and full opaque cubes other than barriers and pumpkins.
fn wall_joins(s: u16) -> bool {
    let id = s >> 4;
    id == 139
        || matches!(id, 107 | 183..=187)
        || (crate::blocks::look(s).kind == crate::blocks::Kind::Opaque
            && !matches!(id, 86 | 91 | 166))
}

/// `BlockWall.getActualState`: `up` is true unless the block above is air.
fn wall(meta: usize, nb: Nb) -> Found {
    let join = |dx, dz| wall_joins(nb(dx, 0, dz));
    Some((
        ["cobblestone_wall", "mossy_cobblestone_wall"][meta & 1],
        format!(
            "east={},north={},south={},up={},west={}",
            join(1, 0),
            join(0, -1),
            join(0, 1),
            nb(0, 1, 0) >> 4 != 0,
            join(-1, 0)
        ),
    ))
}
