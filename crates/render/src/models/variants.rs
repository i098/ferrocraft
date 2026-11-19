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
        _ => None,
    }
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
