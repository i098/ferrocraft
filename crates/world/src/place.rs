//! Block placing of Minecraft 1.8.9 on the client: `PlayerControllerMP.onPlayerRightClick` asks
//! `ItemBlock.canPlaceBlockOnSide` before it sends C08, then runs `ItemBlock.onItemUse` (and the
//! `ItemSlab`, `ItemDoor` and `ItemSign` overrides) on the client world. The server's
//! `BlockChange` stays the truth and overwrites any wrong guess.
//!
//! Orientation (`onBlockPlaced`) is exact for stairs, slabs, logs, torches, ladders and doors.
//! Every other block takes its item damage as metadata and the replaceable-target rule; the
//! support rules of rails, plates, carpet, lily pads and plants are included, those of buttons,
//! levers, trapdoors, vines, cactus, mushrooms and snow layers are not. Blocks that face the
//! placer in `onBlockPlacedBy` (chests, furnaces, pistons, pumpkins, anvils) keep metadata 0
//! until the server answers. Only the local player blocks a placement; vanilla also counts
//! mobs and vehicles.

use crate::dig::{ladder_box, opaque_cube};

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

fn is_stairs(id: u16) -> bool {
    matches!(
        id,
        53 | 67 | 108 | 109 | 114 | 128 | 134 | 135 | 136 | 156 | 163 | 164 | 180
    )
}

fn is_slab(id: u16) -> bool {
    matches!(id, 44 | 126 | 182)
}

/// `Block.isNormalCube`: opaque full cube that cannot power (redstone ore, redstone block).
fn normal_cube(state: u16) -> bool {
    let id = state >> 4;
    opaque_cube(id) && !matches!(id, 73 | 74 | 152)
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

/// `Block.isReplaceable`: a click on such a block places into it, not against it.
fn block_replaceable(state: u16) -> bool {
    let (id, meta) = (state >> 4, state & 15);
    match id {
        0 | 8..=11 | 31 | 32 | 51 | 106 => true,
        78 => meta & 7 == 0,
        175 => matches!(meta & 7, 2 | 3),
        _ => false,
    }
}

/// `Material.isReplaceable`: air, liquids, fire, snow layer and the plants of `Material.vine`.
fn material_replaceable(id: u16) -> bool {
    matches!(id, 0 | 8..=11 | 31 | 32 | 51 | 78 | 106 | 175)
}

/// Collision box of the block's default state, `None` when it has none.
const NO_COLLISION: [u16; 38] = [
    6, 27, 28, 31, 32, 37, 38, 39, 40, 50, 55, 59, 63, 66, 68, 69, 70, 72, 75, 76, 77, 78, 83, 90,
    93, 94, 104, 105, 106, 115, 131, 132, 141, 142, 143, 147, 148, 175,
];

fn collision(id: u16) -> Option<[f64; 6]> {
    if NO_COLLISION.contains(&id) {
        return None;
    }
    match id {
        44 | 126 | 182 => Some([0.0, 0.0, 0.0, 1.0, 0.5, 1.0]),
        65 => Some(ladder_box(2)),
        171 => Some([0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]),
        _ => Some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
    }
}

/// `World.checkNoEntityCollision` for the local player only.
fn blocked_by_player(b: [f64; 6], pos: [i32; 3], body: [f64; 6]) -> bool {
    (0..3).all(|a| {
        let o = f64::from(pos[a]);
        body[a + 3] > o + b[a] && body[a] < o + b[a + 3]
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
        face != 0 && normal_cube(support)
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

/// `Block.canPlaceBlockAt` of `id` into a cell whose material is replaceable.
fn supported(at: &dyn Fn([i32; 3]) -> u16, id: u16, pos: [i32; 3]) -> bool {
    let below = at(offset(pos, 0));
    match id {
        50 | 75 | 76 => [NORTH, EAST, SOUTH, WEST, UP]
            .into_iter()
            .any(|f| torch_can_place(at, pos, f)),
        65 => HORIZONTAL.into_iter().any(|f| ladder_can_stay(at, pos, f)),
        27 | 28 | 66 | 157 | 70 | 72 | 147 | 148 => solid_top(below),
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
            .find(|&f| normal_cube(at(offset(pos, opposite(f)))))
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
    body: [f64; 6],
) -> bool {
    let block = id as u16;
    if is_slab(block) && slab_target(at, block, damage, click).is_some() {
        return true;
    }
    let state = at(click.pos);
    let pos = if block_replaceable(state) {
        click.pos
    } else {
        offset(click.pos, click.face)
    };
    can_block_be_placed(at, block, pos, body)
}

/// `World.canBlockBePlaced` with no placing entity.
fn can_block_be_placed(
    at: &dyn Fn([i32; 3]) -> u16,
    block: u16,
    pos: [i32; 3],
    body: [f64; 6],
) -> bool {
    if collision(block).is_some_and(|b| blocked_by_player(b, pos, body)) {
        return false;
    }
    material_replaceable(at(pos) >> 4) && supported(at, block, pos)
}

/// `ItemStack.onItemUse` of an item block (id 1 to 255) and the items that place like one:
/// doors (324, 330, 427 to 431) and signs (323).
pub fn use_item(
    at: &dyn Fn([i32; 3]) -> u16,
    id: i16,
    damage: i16,
    click: Click,
    yaw: f32,
    body: [f64; 6],
) -> Outcome {
    match id {
        323 => use_sign(at, click),
        324 | 330 | 427..=431 => use_door(at, id, click, yaw),
        1..=255 => use_block(at, id as u16, damage, click, yaw, body),
        _ => Outcome::Refused,
    }
}

fn use_block(
    at: &dyn Fn([i32; 3]) -> u16,
    id: u16,
    damage: i16,
    click: Click,
    yaw: f32,
    body: [f64; 6],
) -> Outcome {
    if is_slab(id) {
        if let Some(out) = merge_slab(at, id, damage, click, body) {
            return out;
        }
    }
    let pos = if block_replaceable(at(click.pos)) {
        click.pos
    } else {
        offset(click.pos, click.face)
    };
    if !can_block_be_placed(at, id, pos, body) {
        return Outcome::Refused;
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
    body: [f64; 6],
) -> Option<Outcome> {
    let var = damage as u16 & 7;
    let pos = slab_target(at, id, damage, click)?;
    if blocked_by_player([0.0, 0.0, 0.0, 1.0, 1.0, 1.0], pos, body) {
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
    let pos = if block_replaceable(at(click.pos)) {
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
        let s = placed(use_item(&at, 53, 0, top_of_ground(), 0.0, FAR));
        assert_eq!(s, vec![([0, 4, 0], 53 << 4 | 2)]);
        // yaw 90 faces west: meta 1; click on the upper half of a side face: top half
        let side = Click {
            pos: [0, 4, 5],
            face: 5,
            cursor: [1.0, 0.8, 0.5],
        };
        let s = placed(use_item(&at, 53, 0, side, 90.0, FAR));
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
            use_item(&at, 44, 3, on, 0.0, FAR),
            Outcome::Placed(vec![([0, 4, 0], 43 << 4 | 3)])
        );
        // another variant does not merge: placed on top at y=5
        let s = placed(use_item(&at, 44, 1, on, 0.0, FAR));
        assert_eq!(s, vec![([0, 5, 0], 44 << 4 | 1)]);
        // side click in the upper half gives a top slab
        let side = Click {
            pos: [0, 4, 0],
            face: 3,
            cursor: [0.5, 0.75, 1.0],
        };
        let s = placed(use_item(&at, 44, 0, side, 0.0, FAR));
        assert_eq!(s, vec![([0, 4, 1], 44 << 4 | 8)]);
    }

    #[test]
    fn the_local_player_blocks_a_cube_but_not_a_torch() {
        let at = flat(&[]);
        let body = [-0.3, 4.0, -0.3, 0.3, 5.8, 0.3];
        assert_eq!(
            use_item(&at, 4, 0, top_of_ground(), 0.0, body),
            Outcome::Refused
        );
        assert!(!can_place_on_side(&at, 4, 0, top_of_ground(), body));
        assert!(!placed(use_item(&at, 50, 0, top_of_ground(), 0.0, body)).is_empty());
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
            placed(use_item(&at, 17, 2, east_face, 0.0, FAR))[0].1,
            17 << 4 | 2 | 4
        );
        assert_eq!(
            placed(use_item(&at, 50, 0, east_face, 0.0, FAR)),
            vec![([0, 4, 0], 50 << 4 | 2)]
        );
        assert_eq!(
            placed(use_item(&at, 65, 0, east_face, 0.0, FAR)),
            vec![([0, 4, 0], 65 << 4 | 4)]
        );
        // a ladder on the ground has no wall to hold it
        let open = Click {
            pos: [5, 3, 5],
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 65, 0, open, 0.0, FAR), Outcome::Refused);
    }

    #[test]
    fn doors_need_the_top_face_and_pick_the_hinge() {
        let at = flat(&[]);
        let d = placed(use_item(&at, 324, 0, top_of_ground(), 0.0, FAR));
        assert_eq!(d, vec![([0, 4, 0], 64 << 4 | 1), ([0, 5, 0], 64 << 4 | 8)]);
        let side = Click {
            face: 3,
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 324, 0, side, 0.0, FAR), Outcome::Refused);
    }

    #[test]
    fn signs_are_not_predicted() {
        let at = flat(&[]);
        assert_eq!(
            use_item(&at, 323, 0, top_of_ground(), 0.0, FAR),
            Outcome::Used
        );
        let down = Click {
            face: 0,
            ..top_of_ground()
        };
        assert_eq!(use_item(&at, 323, 0, down, 0.0, FAR), Outcome::Refused);
    }
}
