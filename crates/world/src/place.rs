//! Block placing of Minecraft 1.8.9 on the client: `PlayerControllerMP.onPlayerRightClick` asks
//! `ItemBlock.canPlaceBlockOnSide` before it sends C08, then runs `ItemBlock.onItemUse` (and the
//! `ItemSlab`, `ItemDoor` and `ItemSign` overrides) on the client world. The server's
//! `BlockChange` stays the truth and overwrites any wrong guess.
//!
//! Orientation (`onBlockPlaced`) is exact for stairs, slabs, logs, torches, ladders, buttons and
//! doors. Every other block takes its item damage as metadata and the replaceable-target rule;
//! the support rules of rails, plates, carpet, lily pads, plants, ladders and buttons are
//! included, those of levers, trapdoors, vines, cactus, mushrooms and snow layers are not.
//! Blocks that face the placer in `onBlockPlacedBy` (chests, furnaces, pistons, pumpkins, anvils)
//! keep metadata 0 until the server answers. The local player and every entity with
//! `Entity.preventEntitySpawning` block a placement.

use crate::dig::{opaque_cube, selection_box};

/// Where and how a click hit a block: `hitVec - pos` as vanilla's `float` locals.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    pub pos: [i32; 3],
    /// `EnumFacing` index: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east.
    pub face: u8,
    pub cursor: [f32; 3],
}

/// The result of `ItemStack.onItemUse` on the client.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Returns false: no swing, the use falls through to the item's air use.
    Refused,
    /// Returns true but the client world did not change and no item is consumed.
    Used,
    /// Returns true and these blocks were set; one item is consumed outside creative.
    Placed(Vec<([i32; 3], u16)>),
}

const NORTH: u8 = 2;
const EAST: u8 = 5;
const SOUTH: u8 = 3;
const WEST: u8 = 4;
const UP: u8 = 1;
/// `EnumFacing.Plane.HORIZONTAL` in iteration order.
const HORIZONTAL: [u8; 4] = [NORTH, EAST, SOUTH, WEST];

const OFFSET: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

fn offset(pos: [i32; 3], face: u8) -> [i32; 3] {
    let d = OFFSET[usize::from(face)];
    [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]
}

fn opposite(face: u8) -> u8 {
    face ^ 1
}

pub(crate) fn is_stairs(id: u16) -> bool {
    matches!(
        id,
        53 | 67 | 108 | 109 | 114 | 128 | 134 | 135 | 136 | 156 | 163 | 164 | 180
    )
}

fn is_slab(id: u16) -> bool {
    matches!(id, 44 | 126 | 182)
}

/// `Block.isNormalCube`: opaque full cube that cannot power (redstone block only; redstone ore
/// does not override `canProvidePower`). Torches use `World.isBlockNormalCube(pos, true)`
/// instead: plain `opaque_cube`.
fn normal_cube(state: u16) -> bool {
    let id = state >> 4;
    opaque_cube(id) && id != 152
}

/// `Material.circuits`: torches, redstone parts, levers, buttons, rails, hooks and ladders.
fn circuits(id: u16) -> bool {
    matches!(
        id,
        27 | 28
            | 50
            | 55
            | 65
            | 66
            | 69
            | 75
            | 76
            | 77
            | 93
            | 94
            | 131
            | 132
            | 143
            | 149
            | 150
            | 157
    )
}

/// `World.doesBlockHaveSolidTopSurface`.
fn solid_top(state: u16) -> bool {
    let (id, meta) = (state >> 4, state & 15);
    opaque_cube(id)
        || (is_stairs(id) && meta & 4 != 0)
        || (is_slab(id) && meta & 8 != 0)
        || id == 154
        || (id == 78 && meta & 7 == 7)
}

/// `BlockFence` ids; `BlockBasePressurePlate.canBePlacedOn` accepts one below the plate.
fn is_fence(id: u16) -> bool {
    matches!(id, 85 | 113 | 188..=192)
}

/// `Block.isReplaceable`: a click on such a block places into it, not against it.
fn block_replaceable(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3]) -> bool {
    let state = at(pos);
    let (id, meta) = (state >> 4, state & 15);
    match id {
        0 | 8..=11 | 31 | 32 | 51 | 106 => true,
        78 => meta & 7 == 0,
        // `BlockDoublePlant.getActualState`: an upper half takes its variant from the lower half.
        175 => {
            let base = if meta & 8 != 0 {
                at(offset(pos, 0))
            } else {
                state
            };
            base >> 4 == 175 && matches!(base & 7, 2 | 3)
        }
        _ => false,
    }
}

/// `Material.isReplaceable`: air, liquids, fire, snow layer and the plants of `Material.vine`.
fn material_replaceable(id: u16) -> bool {
    matches!(id, 0 | 8..=11 | 31 | 32 | 51 | 78 | 106 | 175)
}

/// Blocks whose `getCollisionBoundingBox` is null: no entity ever blocks them.
const NO_COLLISION: [u16; 37] = [
    6, 27, 28, 30, 31, 32, 37, 38, 39, 40, 50, 55, 59, 63, 66, 68, 69, 70, 72, 75, 76, 77, 83, 90,
    104, 105, 106, 115, 131, 132, 141, 142, 143, 147, 148, 157, 175,
];

/// Blocks whose `getCollisionBoundingBox` is not a function of the block: fences, panes,
/// single slabs, chests, ladders, trapdoors and anvils. None of them overrides it (`BlockLadder`
/// does, but its `setBlockBoundsBasedOnState` guard fails on the air target), so vanilla reads the
/// bounds the last caller left on the shared block instance. A body in the unit cube therefore
/// neither refuses the click nor predicts a placement.
fn stale_bounds(id: u16) -> bool {
    matches!(
        id,
        44 | 54 | 65 | 85 | 96 | 101 | 102 | 113 | 126 | 145 | 146 | 160 | 167 | 182 | 188..=192
    )
}

/// Blocks with one fixed box smaller than the cell: slabs, chests, cactus, soul sand, carpet.
const FIXED: &[(&[u16], [f64; 6])] = &[
    (&[130], [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]),
    (&[81], [0.0625, 0.0, 0.0625, 0.9375, 0.9375, 0.9375]),
    (&[88], [0.0, 0.0, 0.0, 1.0, 0.875, 1.0]),
    (&[93, 94, 149, 150], [0.0, 0.0, 0.0, 1.0, 0.125, 1.0]),
    (&[111], [0.0, 0.0, 0.0, 1.0, 0.015625, 1.0]),
    (&[78], [0.0, 0.0, 0.0, 1.0, 0.0, 1.0]),
    (&[116], [0.0, 0.0, 0.0, 1.0, 0.75, 1.0]),
    (&[120], [0.0, 0.0, 0.0, 1.0, 0.8125, 1.0]),
    (&[122], [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375]),
    (&[151, 178], [0.0, 0.0, 0.0, 1.0, 0.375, 1.0]),
    (&[171], [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]),
];

/// `Block.getCollisionBoundingBox` of the default state at `pos`, `None` when it has none.
/// Walls and gates read their neighbours and stand 1.5 high. The hopper resets its bounds to
/// the unit cube after adding its boxes. Stale-bounds blocks answer the unit cube here.
fn collision(at: &dyn Fn([i32; 3]) -> u16, id: u16, pos: [i32; 3]) -> Option<[f64; 6]> {
    if NO_COLLISION.contains(&id) {
        return None;
    }
    if let Some((_, b)) = FIXED.iter().find(|(ids, _)| ids.contains(&id)) {
        return Some(*b);
    }
    match id {
        107 | 139 | 183..=187 => {
            let here = |p: [i32; 3]| if p == pos { id << 4 } else { at(p) };
            selection_box(&here, pos).map(|b| [b[0], b[1], b[2], b[3], 1.5, b[5]])
        }
        _ => Some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
    }
}

/// `World.checkNoEntityCollision`: a body overlaps the box, so the placement is refused.
fn blocked(b: [f64; 6], pos: [i32; 3], bodies: &[[f64; 6]]) -> bool {
    bodies.iter().any(|body| {
        (0..3).all(|a| {
            let o = f64::from(pos[a]);
            body[a + 3] > o + b[a] && body[a] < o + b[a + 3]
        })
    })
}

/// `EnumFacing.getHorizontal(floor(yaw * 4 / 360 + 0.5) & 3)` as an index: 0 south, 1 west,
/// 2 north, 3 east.
fn horizontal_index(yaw: f32) -> i32 {
    ((f64::from(yaw * 4.0 / 360.0) + 0.5).floor() as i32) & 3
}

/// `EnumFacing.fromAngle`, which `ItemDoor` uses.
fn angle_index(yaw: f32) -> i32 {
    (f64::from(yaw) / 90.0 + 0.5).floor() as i32 & 3
}

/// `EnumFacing` index of a horizontal index.
fn facing_of(index: i32) -> u8 {
    [SOUTH, WEST, NORTH, EAST][(index & 3) as usize]
}

/// `BlockTorch.canPlaceAt`.
fn torch_can_place(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], face: u8) -> bool {
    let support = at(offset(pos, opposite(face)));
    if face == UP {
        let id = support >> 4;
        solid_top(support) || matches!(id, 85 | 113 | 188..=192 | 20 | 95 | 139)
    } else {
        face != 0 && opaque_cube(support >> 4)
    }
}

fn ladder_can_stay(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], face: u8) -> bool {
    normal_cube(at(offset(pos, opposite(face))))
}

/// `BlockBush.canPlaceBlockAt`: saplings, grass, flowers, double plants and dead bushes.
fn plant_supported(at: &dyn Fn([i32; 3]) -> u16, id: u16, pos: [i32; 3], below: u16) -> bool {
    let soil = matches!(below >> 4, 2 | 3 | 60);
    match id {
        175 => soil && at(offset(pos, UP)) >> 4 == 0,
        32 => matches!(below >> 4, 3 | 12 | 159 | 172),
        _ => soil,
    }
}

/// `BlockButton.func_181088_a`: the neighbour in `face` holds the button up.
fn button_support(state: u16, face: u8) -> bool {
    if face == 0 {
        solid_top(state)
    } else {
        normal_cube(state)
    }
}

/// `Block.canPlaceBlockAt` of `id` into a cell whose material is replaceable.
fn supported(at: &dyn Fn([i32; 3]) -> u16, id: u16, pos: [i32; 3], side: u8) -> bool {
    let below = at(offset(pos, 0));
    match id {
        50 | 75 | 76 => [NORTH, EAST, SOUTH, WEST, UP]
            .into_iter()
            .any(|f| torch_can_place(at, pos, f)),
        65 => HORIZONTAL.into_iter().any(|f| ladder_can_stay(at, pos, f)),
        // `BlockButton.canPlaceBlockOnSide`: only the clicked side counts.
        77 | 143 => button_support(at(offset(pos, opposite(side))), opposite(side)),
        27 | 28 | 66 | 157 => solid_top(below),
        // `BlockBasePressurePlate.canBePlacedOn`: a solid top surface or a fence.
        70 | 72 | 147 | 148 => solid_top(below) || is_fence(below >> 4),
        171 => below >> 4 != 0,
        111 => matches!(below >> 4, 8 | 9) && below & 15 == 0,
        6 | 31 | 37 | 38 | 175 | 32 => plant_supported(at, id, pos, below),
        _ => true,
    }
}

/// `Item.getMetadata` of an `ItemBlock` for the item damage.
fn item_meta(id: u16, damage: i16) -> u16 {
    let d = damage as u16;
    match id {
        18 | 161 => (d & 3) | 4,
        1 | 3 | 5 | 6 | 12 | 17 | 19 | 24 | 31 | 35 | 38 | 95 | 97 | 98 | 139 | 155 | 159 | 160
        | 162 | 168 | 171 | 175 | 179 => d & 15,
        _ => 0,
    }
}

/// Stairs and slabs: the lower half unless the click hit the top half of a side face.
fn bottom_half(face: u8, cursor_y: f32) -> bool {
    face != 0 && (face == UP || cursor_y <= 0.5)
}

/// `BlockLog.onBlockPlaced`: the axis bits of the clicked face.
fn log_axis(face: u8) -> u16 {
    match face {
        WEST | EAST => 4,
        NORTH | SOUTH => 8,
        _ => 0,
    }
}

/// `BlockTorch.onBlockPlaced`: the clicked face, else the first side with support.
fn torch_meta(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], face: u8) -> u16 {
    let facing = if torch_can_place(at, pos, face) {
        face
    } else {
        HORIZONTAL
            .into_iter()
            .find(|&f| opaque_cube(at(offset(pos, opposite(f))) >> 4))
            .unwrap_or(UP)
    };
    [0_u16, 5, 4, 3, 2, 1][usize::from(facing)]
}

/// `BlockLadder.onBlockPlaced`: the clicked side face, else the first side with support.
fn ladder_facing(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], face: u8) -> u16 {
    let facing = if (NORTH..=EAST).contains(&face) && ladder_can_stay(at, pos, face) {
        face
    } else {
        HORIZONTAL
            .into_iter()
            .find(|&f| ladder_can_stay(at, pos, f))
            .unwrap_or(NORTH)
    };
    u16::from(facing)
}

/// `onBlockPlaced` for the block at `pos` placed against `face`.
fn placed_state(
    at: &dyn Fn([i32; 3]) -> u16,
    id: u16,
    damage: i16,
    pos: [i32; 3],
    face: u8,
    cursor_y: f32,
    yaw: f32,
) -> u16 {
    let meta = item_meta(id, damage);
    let meta = match id {
        _ if is_stairs(id) => {
            let facing = [2_u16, 1, 3, 0][horizontal_index(yaw) as usize];
            facing | if bottom_half(face, cursor_y) { 0 } else { 4 }
        }
        44 | 126 | 182 => (damage as u16 & 7) | if bottom_half(face, cursor_y) { 0 } else { 8 },
        17 | 162 => meta | log_axis(face),
        50 | 75 | 76 => torch_meta(at, pos, face),
        65 => ladder_facing(at, pos, face),
        // `BlockButton.onBlockPlaced` and `getMetaFromState`: DOWN 0, EAST 1, WEST 2, SOUTH 3,
        // NORTH 4, UP 5 - the same table as `BlockTorch`.
        77 | 143 => [0_u16, 5, 4, 3, 2, 1][usize::from(face)],
        _ => meta,
    };
    id << 4 | meta
}

/// `ItemBlock.canPlaceBlockOnSide` plus the `ItemSlab` override: whether
/// `PlayerControllerMP` sends the click at all.
pub fn can_place_on_side(
    at: &dyn Fn([i32; 3]) -> u16,
    id: i16,
    damage: i16,
    click: Click,
    bodies: &[[f64; 6]],
) -> bool {
    let block = id as u16;
    if is_slab(block) && slab_target(at, block, damage, click).is_some() {
        return true;
    }
    // `canPlaceBlockOnSide` asks the clicked snow layer cell however many layers it holds.
    let pos = if at(click.pos) >> 4 == 78 || block_replaceable(at, click.pos) {
        click.pos
    } else {
        offset(click.pos, click.face)
    };
    // `ItemBlock.canPlaceBlockOnSide` clicks a snow layer from above.
    let side = if pos == click.pos && at(pos) >> 4 == 78 {
        UP
    } else {
        click.face
    };
    can_block_be_placed(at, block, pos, side, bodies)
}

/// `World.canBlockBePlaced` with no placing entity; `side` is the face `Block.canReplace` gets.
fn can_block_be_placed(
    at: &dyn Fn([i32; 3]) -> u16,
    block: u16,
    pos: [i32; 3],
    side: u8,
    bodies: &[[f64; 6]],
) -> bool {
    let overlaps = collision(at, block, pos).is_some_and(|b| blocked(b, pos, bodies));
    if overlaps && !stale_bounds(block) {
        return false;
    }
    // An anvil lands on circuits before `isReplaceable` and `canReplace` are asked.
    if block == 145 && circuits(at(pos) >> 4) {
        return true;
    }
    material_replaceable(at(pos) >> 4) && supported(at, block, pos, side)
}

/// `ItemStack.onItemUse` of an item block (id 1 to 255) and the items that place like one:
/// doors (324, 330, 427 to 431) and signs (323).
pub fn use_item(
    at: &dyn Fn([i32; 3]) -> u16,
    id: i16,
    damage: i16,
    click: Click,
    yaw: f32,
    bodies: &[[f64; 6]],
) -> Outcome {
    match id {
        323 => use_sign(at, click),
        324 | 330 | 427..=431 => use_door(at, id, click, yaw),
        1..=255 => use_block(at, id as u16, damage, click, yaw, bodies),
        _ => Outcome::Refused,
    }
}

fn use_block(
    at: &dyn Fn([i32; 3]) -> u16,
    id: u16,
    damage: i16,
    click: Click,
    yaw: f32,
    bodies: &[[f64; 6]],
) -> Outcome {
    if is_slab(id) {
        if let Some(out) = merge_slab(at, id, damage, click, bodies) {
            return out;
        }
    }
    let pos = if block_replaceable(at, click.pos) {
        click.pos
    } else {
        offset(click.pos, click.face)
    };
    let side = if pos == click.pos && at(pos) >> 4 == 78 {
        UP
    } else {
        click.face
    };
    if !can_block_be_placed(at, id, pos, side, bodies) {
        return Outcome::Refused;
    }
    if stale_bounds(id) && collision(at, id, pos).is_some_and(|b| blocked(b, pos, bodies)) {
        return Outcome::Used;
    }
    if !(0..256).contains(&pos[1]) {
        return Outcome::Used;
    }
    let state = placed_state(at, id, damage, pos, click.face, click.cursor[1], yaw);
    Outcome::Placed(vec![(pos, state)])
}

/// The cell a slab click would double up: the clicked slab or the one beside it.
fn slab_target(
    at: &dyn Fn([i32; 3]) -> u16,
    id: u16,
    damage: i16,
    click: Click,
) -> Option<[i32; 3]> {
    let var = damage as u16 & 7;
    let state = at(click.pos);
    let top = state & 8 != 0;
    let toward_slab = (click.face == UP && !top) || (click.face == 0 && top);
    let next = offset(click.pos, click.face);
    let same = |s: u16| s >> 4 == id && s & 7 == var;
    if toward_slab && same(state) {
        Some(click.pos)
    } else if same(at(next)) {
        Some(next)
    } else {
        None
    }
}

/// `ItemSlab.onItemUse`: a second slab of the same kind makes a double slab, even when the
/// player stands in it (the click then counts as used but changes nothing).
fn merge_slab(
    at: &dyn Fn([i32; 3]) -> u16,
    id: u16,
    damage: i16,
    click: Click,
    bodies: &[[f64; 6]],
) -> Option<Outcome> {
    let var = damage as u16 & 7;
    let pos = slab_target(at, id, damage, click)?;
    if blocked([0.0, 0.0, 0.0, 1.0, 1.0, 1.0], pos, bodies) {
        return Some(Outcome::Used);
    }
    Some(Outcome::Placed(vec![(pos, (id - 1) << 4 | var)]))
}

/// Blocks a sign cannot be hung on.
const NON_SOLID_FOR_SIGN: [u16; 48] = [
    0, 6, 8, 9, 10, 11, 27, 28, 31, 32, 37, 38, 39, 40, 50, 51, 55, 59, 65, 66, 69, 75, 76, 77, 78,
    83, 90, 93, 94, 104, 105, 106, 111, 115, 119, 127, 131, 132, 140, 141, 142, 143, 144, 149, 150,
    157, 171, 175,
];

/// `ItemSign.onItemUse` on the client: true when the sign can go there; the server places it.
fn use_sign(at: &dyn Fn([i32; 3]) -> u16, click: Click) -> Outcome {
    let clicked = at(click.pos) >> 4;
    let non_solid = NON_SOLID_FOR_SIGN.contains(&clicked);
    if click.face == 0 || non_solid {
        return Outcome::Refused;
    }
    let pos = offset(click.pos, click.face);
    if material_replaceable(at(pos) >> 4) {
        Outcome::Used
    } else {
        Outcome::Refused
    }
}

/// `ItemDoor.placeDoor`: the hinge goes right next to a left door or the more solid side.
fn hinges_right(at: &dyn Fn([i32; 3]) -> u16, door: u16, pos: [i32; 3], index: i32) -> bool {
    // `rotateY` and `rotateYCCW` of the facing, as horizontal indices.
    let side = |i: i32| offset(pos, facing_of(i));
    let (right, left) = (side(index + 1), side(index - 1));
    let normals =
        |p: [i32; 3]| u8::from(normal_cube(at(p))) + u8::from(normal_cube(at(offset(p, UP))));
    let has_door = |p: [i32; 3]| at(p) >> 4 == door || at(offset(p, UP)) >> 4 == door;
    (has_door(left) && !has_door(right)) || normals(right) > normals(left)
}

/// `ItemDoor.onItemUse` and `placeDoor`.
fn use_door(at: &dyn Fn([i32; 3]) -> u16, item: i16, click: Click, yaw: f32) -> Outcome {
    if click.face != UP {
        return Outcome::Refused;
    }
    let door = match item {
        324 => 64,
        330 => 71,
        n => (n - 427 + 193) as u16,
    };
    let pos = if block_replaceable(at, click.pos) {
        click.pos
    } else {
        offset(click.pos, UP)
    };
    let upper = offset(pos, UP);
    let fits = pos[1] < 255
        && solid_top(at(offset(pos, 0)))
        && material_replaceable(at(pos) >> 4)
        && material_replaceable(at(upper) >> 4);
    if !fits {
        return Outcome::Refused;
    }
    let index = angle_index(yaw);
    let hinge_right = hinges_right(at, door, pos, index);
    let lower_meta = ((index + 1) & 3) as u16;
    let upper_meta = 8 | u16::from(hinge_right);
    Outcome::Placed(vec![
        (pos, door << 4 | lower_meta),
        (upper, door << 4 | upper_meta),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const FAR: [f64; 6] = [100.0, 100.0, 100.0, 100.6, 101.8, 100.6];

    fn flat(extra: &[([i32; 3], u16)]) -> impl Fn([i32; 3]) -> u16 {
        let m: HashMap<[i32; 3], u16> = extra.iter().copied().collect();
        move |p| match m.get(&p) {
            Some(s) => *s,
            None if p[1] <= 3 && p[1] >= 0 => 3 << 4,
            None => 0,
        }
    }

    fn top_of_ground() -> Click {
        Click {
            pos: [0, 3, 0],
            face: 1,
            cursor: [0.5, 1.0, 0.5],
        }
    }

    fn placed(out: Outcome) -> Vec<([i32; 3], u16)> {
        match out {
            Outcome::Placed(b) => b,
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn stairs_face_the_placer_and_flip_by_cursor() {
        let at = flat(&[([0, 4, 5], 1 << 4)]);
        // yaw 0 faces south: stairs meta 2
        let s = placed(use_item(&at, 53, 0, top_of_ground(), 0.0, &[FAR]));
        assert_eq!(s, vec![([0, 4, 0], 53 << 4 | 2)]);
        // yaw 90 faces west: meta 1; click on the upper half of a side face: top half
        let side = Click {
            pos: [0, 4, 5],
            face: 5,
            cursor: [1.0, 0.8, 0.5],
        };
        let s = placed(use_item(&at, 53, 0, side, 90.0, &[FAR]));
        assert_eq!(s, vec![([1, 4, 5], 53 << 4 | 1 | 4)]);
    }

    #[test]
    fn slabs_merge_and_choose_half() {
        let at = flat(&[([0, 4, 0], 44 << 4 | 3)]);
        let on = Click {
            pos: [0, 4, 0],
            face: 1,
            cursor: [0.5, 0.5, 0.5],
        };
        assert_eq!(
            use_item(&at, 44, 3, on, 0.0, &[FAR]),
            Outcome::Placed(vec![([0, 4, 0], 43 << 4 | 3)])
        );
        // another variant does not merge: placed on top at y=5
        let s = placed(use_item(&at, 44, 1, on, 0.0, &[FAR]));
        assert_eq!(s, vec![([0, 5, 0], 44 << 4 | 1)]);
        // side click in the upper half gives a top slab
        let side = Click {
            pos: [0, 4, 0],
            face: 3,
            cursor: [0.5, 0.75, 1.0],
        };
        let s = placed(use_item(&at, 44, 0, side, 0.0, &[FAR]));
        assert_eq!(s, vec![([0, 4, 1], 44 << 4 | 8)]);
    }

    #[test]
    fn anvils_replace_circuits_and_torches_stand_on_redstone_blocks() {
        let rail = flat(&[([0, 4, 0], 66 << 4)]);
        assert!(matches!(
            use_item(&rail, 145, 0, top_of_ground(), 0.0, &[FAR]),
            Outcome::Placed(_)
        ));
        assert_eq!(
            use_item(&rail, 1, 0, top_of_ground(), 0.0, &[FAR]),
            Outcome::Refused
        );
        let block = flat(&[([0, 4, 5], 152 << 4)]);
        let side = Click {
            pos: [0, 4, 5],
            face: 2,
            cursor: [0.5, 0.5, 0.0],
        };
        assert!(matches!(
            use_item(&block, 50, 0, side, 0.0, &[FAR]),
            Outcome::Placed(_)
        ));
    }

    #[test]
    fn the_local_player_blocks_a_cube_but_not_a_torch() {
        let at = flat(&[]);
        let body = [-0.3, 4.0, -0.3, 0.3, 5.8, 0.3];
        assert_eq!(
            use_item(&at, 4, 0, top_of_ground(), 0.0, &[body]),
            Outcome::Refused
        );
        assert!(!can_place_on_side(&at, 4, 0, top_of_ground(), &[body]));
        assert!(!placed(use_item(&at, 50, 0, top_of_ground(), 0.0, &[body])).is_empty());
        // A cobweb has no collision box, so a body in its cell does not refuse it.
        assert!(matches!(
            use_item(&at, 30, 0, top_of_ground(), 0.0, &[body]),
            Outcome::Placed(_)
        ));
        // `BlockChest` has no `getCollisionBoundingBox`: the box is stale, so a body in the
        // cell neither refuses nor places (`Used`); an ender chest has its own fixed box.
        let above = [-0.3, 4.875, -0.3, 0.3, 6.675, 0.3];
        for id in [54, 146] {
            assert_eq!(
                use_item(&at, id, 0, top_of_ground(), 0.0, &[above]),
                Outcome::Used
            );
        }
        assert!(matches!(
            use_item(&at, 130, 0, top_of_ground(), 0.0, &[above]),
            Outcome::Placed(_)
        ));
        // `BlockRailBase` has no collision box: an activator rail goes into the player's cell.
        let rail = flat(&[([0, 3, 0], 1 << 4)]);
        assert!(matches!(
            use_item(&rail, 157, 0, top_of_ground(), 0.0, &[body]),
            Outcome::Placed(_)
        ));
    }

    #[test]
    fn a_second_body_blocks_the_cell() {
        let at = flat(&[]);
        let far = [100.0, 100.0, 100.0, 100.6, 101.8, 100.6];
        let cow = [0.1, 4.0, 0.1, 0.9, 5.3, 0.9];
        assert_eq!(
            use_item(&at, 1, 0, top_of_ground(), 0.0, &[far, cow]),
            Outcome::Refused
        );
        assert!(!can_place_on_side(&at, 1, 0, top_of_ground(), &[far, cow]));
        assert!(matches!(
            use_item(&at, 1, 0, top_of_ground(), 0.0, &[far]),
            Outcome::Placed(_)
        ));
    }

    fn placed_with(at: &dyn Fn([i32; 3]) -> u16, id: i16, body: [f64; 6]) -> bool {
        matches!(
            use_item(at, id, 0, top_of_ground(), 0.0, &[body]),
            Outcome::Placed(_)
        )
    }

    #[test]
    fn a_body_beside_a_post_leaves_the_cell_free_for_walls() {
        // A lone wall post spans x 0.25..0.75: a body on x 0.8..0.9 leaves the cell free for
        // it but not for a cube. Fences defer to the server instead.
        let at = flat(&[]);
        let beside = [0.8, 4.0, 0.3, 0.9, 5.8, 0.7];
        assert!(placed_with(&at, 139, beside));
        assert!(can_place_on_side(&at, 139, 0, top_of_ground(), &[beside]));
        assert!(!placed_with(&at, 4, beside));
    }

    #[test]
    fn walls_and_gates_stand_one_and_a_half_high() {
        // A body on y 5.2 is inside them and above a cube.
        let at = flat(&[]);
        let above = [0.4, 5.2, 0.4, 0.6, 6.0, 0.6];
        for id in [139, 107, 183, 187] {
            assert!(!placed_with(&at, id, above), "{id}");
            assert!(!can_place_on_side(&at, id, 0, top_of_ground(), &[above]));
        }
        assert!(placed_with(&at, 4, above));
    }

    #[test]
    fn a_closed_gate_faces_north() {
        // A closed gate's panel spans z 0.375..0.625 across the cell.
        let at = flat(&[]);
        assert!(placed_with(&at, 107, [0.1, 4.0, 0.1, 0.9, 5.8, 0.3]));
        assert!(!placed_with(&at, 107, [0.1, 4.0, 0.4, 0.9, 5.8, 0.6]));
    }

    #[test]
    fn hoppers_fill_the_cell_and_fences_trapdoors_and_anvils_defer_to_the_server() {
        let at = flat(&[]);
        let click = top_of_ground();
        let corner = [0.0, 4.0, 0.0, 0.05, 5.8, 0.05];
        assert_eq!(
            use_item(&at, 154, 0, click, 0.0, &[corner]),
            Outcome::Refused
        );
        let inside = [0.4, 4.0, 0.4, 0.6, 5.8, 0.6];
        for id in [85, 113, 188, 192, 96, 145, 167] {
            assert!(can_place_on_side(&at, id, 0, click, &[inside]), "{id}");
            assert_eq!(
                use_item(&at, id, 0, click, 0.0, &[inside]),
                Outcome::Used,
                "{id}"
            );
            assert!(matches!(
                use_item(&at, id, 0, click, 0.0, &[FAR]),
                Outcome::Placed(_)
            ));
        }
    }

    #[test]
    fn a_lily_pad_fills_the_cell_floor_and_one_snow_layer_is_a_zero_height_slice() {
        let pond = flat(&[([0, 3, 0], 9 << 4), ([1, 4, 0], 1 << 4)]);
        let side = Click {
            pos: [1, 4, 0],
            face: WEST,
            cursor: [0.0, 0.5, 0.5],
        };
        let edge = [0.1, 4.0, 0.1, 0.3, 5.8, 0.3];
        assert!(!can_place_on_side(&pond, 111, 0, side, &[edge]));
        assert_eq!(
            use_item(&pond, 111, 0, side, 0.0, &[edge]),
            Outcome::Refused
        );
        assert!(can_place_on_side(&pond, 111, 0, side, &[FAR]));
        let at = flat(&[]);
        assert!(!placed_with(&at, 78, [0.1, 3.5, 0.1, 0.3, 5.3, 0.3]));
        assert!(placed_with(&at, 78, [0.1, 4.0, 0.1, 0.3, 5.8, 0.3]));
    }

    #[test]
    fn double_plants_take_their_variant_from_the_lower_half() {
        // A sunflower top with facing 2 is not replaceable: the variant comes from below, so the
        // block goes above the plant.
        let at = flat(&[([0, 3, 1], 175 << 4), ([0, 4, 1], 175 << 4 | 10)]);
        let top = Click {
            pos: [0, 4, 1],
            face: UP,
            cursor: [0.5, 1.0, 0.5],
        };
        assert_eq!(
            placed(use_item(&at, 1, 0, top, 0.0, &[FAR])),
            vec![([0, 5, 1], 1 << 4)]
        );
        // A tall grass top with facing 0 is replaceable: the variant below is grass.
        let at = flat(&[([0, 3, 1], 175 << 4 | 2), ([0, 4, 1], 175 << 4 | 8)]);
        assert_eq!(
            placed(use_item(&at, 1, 0, top, 0.0, &[FAR])),
            vec![([0, 4, 1], 1 << 4)]
        );
    }

    #[test]
    fn plates_stand_on_fences() {
        let top = Click {
            pos: [0, 4, 0],
            face: UP,
            cursor: [0.5, 1.0, 0.5],
        };
        for fence in [85, 113, 188] {
            let at = flat(&[([0, 4, 0], fence << 4)]);
            assert_eq!(
                placed(use_item(&at, 70, 0, top, 0.0, &[FAR])),
                vec![([0, 5, 0], 70 << 4)]
            );
        }
        // A cobblestone wall is no fence.
        let wall = flat(&[([0, 4, 0], 139 << 4)]);
        assert_eq!(use_item(&wall, 70, 0, top, 0.0, &[FAR]), Outcome::Refused);
    }

    #[test]
    fn buttons_attach_to_a_normal_cube() {
        let side = Click {
            pos: [0, 5, 0],
            face: EAST,
            cursor: [1.0, 0.5, 0.5],
        };
        // Redstone ore cannot provide power, so a button sticks to its side (EAST meta 1).
        let ore = flat(&[([0, 5, 0], 73 << 4)]);
        assert_eq!(
            placed(use_item(&ore, 143, 0, side, 0.0, &[FAR])),
            vec![([1, 5, 0], 143 << 4 | 1)]
        );
        // A redstone block provides power, so nothing holds the button up.
        let block = flat(&[([0, 5, 0], 152 << 4)]);
        assert_eq!(
            use_item(&block, 143, 0, side, 0.0, &[FAR]),
            Outcome::Refused
        );
        // Only the clicked side counts: the solid block below the target cell does not hold
        // a button placed against the side of a fence.
        let fence = flat(&[([0, 5, 0], 85 << 4)]);
        assert_eq!(
            use_item(&fence, 143, 0, side, 0.0, &[FAR]),
            Outcome::Refused
        );
        assert!(!can_place_on_side(&fence, 143, 0, side, &[FAR]));
    }

    #[test]
    fn a_click_on_a_snow_layer_cell_stays_in_it() {
        // `canPlaceBlockOnSide` asks the snow cell whatever the layers; `onItemUse` offsets
        // unless the layers make the cell replaceable.
        let at = flat(&[([0, 4, 0], 78 << 4 | 2)]);
        let side = Click {
            pos: [0, 4, 0],
            face: EAST,
            cursor: [1.0, 0.5, 0.5],
        };
        assert!(can_place_on_side(&at, 1, 0, side, &[FAR]));
        assert_eq!(
            placed(use_item(&at, 1, 0, side, 0.0, &[FAR])),
            vec![([1, 4, 0], 1 << 4)]
        );
    }

    #[test]
    fn logs_torches_and_ladders_orient() {
        let wall = [([1, 4, 0], 1 << 4)];
        let at = flat(&wall);
        let east_face = Click {
            pos: [1, 4, 0],
            face: 4,
            cursor: [0.0, 0.5, 0.5],
        };
        assert_eq!(
            placed(use_item(&at, 17, 2, east_face, 0.0, &[FAR]))[0].1,
            17 << 4 | 2 | 4
        );
        assert_eq!(
            placed(use_item(&at, 50, 0, east_face, 0.0, &[FAR])),
            vec![([0, 4, 0], 50 << 4 | 2)]
        );
        assert_eq!(
            placed(use_item(&at, 65, 0, east_face, 0.0, &[FAR])),
            vec![([0, 4, 0], 65 << 4 | 4)]
        );
        // a ladder on the ground has no wall to hold it
        let open = Click {
            pos: [5, 3, 5],
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 65, 0, open, 0.0, &[FAR]), Outcome::Refused);
    }

    #[test]
    fn doors_need_the_top_face_and_pick_the_hinge() {
        let at = flat(&[]);
        let d = placed(use_item(&at, 324, 0, top_of_ground(), 0.0, &[FAR]));
        assert_eq!(d, vec![([0, 4, 0], 64 << 4 | 1), ([0, 5, 0], 64 << 4 | 8)]);
        let side = Click {
            face: 3,
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 324, 0, side, 0.0, &[FAR]), Outcome::Refused);
    }

    #[test]
    fn signs_are_not_predicted() {
        let at = flat(&[]);
        assert_eq!(
            use_item(&at, 323, 0, top_of_ground(), 0.0, &[FAR]),
            Outcome::Used
        );
        let down = Click {
            face: 0,
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 323, 0, down, 0.0, &[FAR]), Outcome::Refused);
    }
}
