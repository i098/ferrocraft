//! Client-side flight of the entities the server sends rarely: arrows, thrown objects, dropped items, primed TNT,
//! falling blocks and fishing bobbers. Each `step` is one `onUpdate` of the 1.8.9 class, run once per tick.
//!
//! Left out, as the server owns them: hits on entities, `setDead` timers, the first-tick block removal of a
//! falling block, cobweb and soul sand slowing, and `pushOutOfBlocks` of items.

use crate::{dig, floor, fluid, shape, Aabb, Player, World};

type At<'a> = &'a dyn Fn([i32; 3]) -> u16;

/// `Entity.motionY -= 0.04` of items, TNT and falling blocks.
const GRAVITY: f64 = 0.039_999_999_105_930_33;

/// Per-entity state that the packets do not carry.
#[derive(Clone, Debug, Default)]
pub struct State {
    in_ground: bool,
    /// `inTile` and `inData` as one block state; `None` until an arrow has stuck in a block.
    in_state: Option<u16>,
    in_water: bool,
    /// Seed of the xorshift stand-in for `Entity.rand`.
    rng: u32,
}

impl State {
    /// `Random.nextFloat`.
    fn rand(&mut self, id: i32) -> f64 {
        let mut x = if self.rng == 0 {
            id as u32 | 1
        } else {
            self.rng
        };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        f64::from(x >> 8) / f64::from(1u32 << 24)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Arrow,
    /// `ThrownEntity` with its `getGravity`.
    Thrown(f32),
    Item,
    Tnt,
    Falling,
    Bobber,
}

fn class(object: i8) -> Option<Class> {
    Some(match object {
        60 => Class::Arrow,
        61 | 62 | 65 => Class::Thrown(0.03),
        73 => Class::Thrown(0.05),
        75 => Class::Thrown(0.07),
        2 => Class::Item,
        50 => Class::Tnt,
        70 => Class::Falling,
        90 => Class::Bobber,
        _ => return None,
    })
}

/// Entities that the client moves between packets.
pub(crate) fn simulated(object: i8) -> bool {
    class(object).is_some()
}

/// `Entity.lerpPositionAndAngles` snaps; only the bobber (and living entities) ease over 3 ticks.
pub(crate) fn snaps(object: i8) -> bool {
    simulated(object) && object != 90
}

struct Blocks<'a>(At<'a>);

impl World for Blocks<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        (self.0)([x, y, z])
    }
}

/// The fields of an entity that the flight reads and writes.
pub(crate) struct Body {
    pub id: i32,
    pub age: u32,
    pub size: (f32, f32),
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub prev_yaw: f32,
    pub prev_pitch: f32,
    pub on_ground: bool,
    pub proj: State,
}

/// One `onUpdate` of the object type `object`.
pub(crate) fn step(object: i8, e: &mut Body, at: At) {
    let Some(class) = class(object) else { return };
    match class {
        Class::Arrow => arrow(e, at),
        Class::Thrown(gravity) => thrown(e, at, gravity),
        Class::Item => item(e, at),
        Class::Tnt => tnt(e, at),
        Class::Falling => falling(e, at),
        Class::Bobber => bobber(e, at),
    }
}

fn body(pos: [f64; 3], (w, h): (f32, f32)) -> Aabb {
    let hw = f64::from(w) / 2.0;
    Aabb {
        min: [pos[0] - hw, pos[1], pos[2] - hw],
        max: [pos[0] + hw, pos[1] + f64::from(h), pos[2] + hw],
    }
}

/// `Entity.handleWaterMovement` (the box shrunk by 0.4 above and below and by 0.001), or with `full`
/// `ItemEntity.checkWaterCollisions` (the whole box): sets `in_water` and adds the flow.
fn handle_water(e: &mut Body, at: At, full: bool) {
    let b = body(e.pos, e.size);
    let (min, max) = if full {
        (b.min, b.max)
    } else {
        (
            [b.min[0] + 0.001, b.min[1] + 0.4 + 0.001, b.min[2] + 0.001],
            [b.max[0] - 0.001, b.max[1] - 0.4 - 0.001, b.max[2] - 0.001],
        )
    };
    let (inside, push) = fluid::water_drag(at, min, max, true);
    for (v, p) in e.vel.iter_mut().zip(push) {
        *v += p;
    }
    e.proj.in_water = inside;
}

/// `World.rayTraceBlocks` along `motion`: the point where it first meets a block, and that block's cell.
/// With `solid_only` blocks without a collision box are skipped (`ignoreBlockWithoutBoundingBox`).
fn trace(
    at: At,
    from: [f64; 3],
    motion: [f64; 3],
    solid_only: bool,
) -> Option<([f64; 3], [i32; 3])> {
    let len = motion.iter().map(|m| m * m).sum::<f64>().sqrt();
    if len == 0.0 {
        return None;
    }
    let dir = motion.map(|m| m / len);
    let (mut origin, mut left) = (from, len);
    loop {
        let (hit, t) = dig::raycast_dir(at, origin, dir, left)?;
        let point = std::array::from_fn(|i| origin[i] + dir[i] * t);
        if !solid_only || !collision_boxes(at, hit.pos).is_empty() {
            return Some((point, hit.pos));
        }
        left -= t + 1e-4;
        if left <= 0.0 {
            return None;
        }
        origin = std::array::from_fn(|i| origin[i] + dir[i] * (t + 1e-4));
    }
}

fn collision_boxes(at: At, cell: [i32; 3]) -> Vec<[f64; 6]> {
    let mut boxes = Vec::new();
    shape::push_boxes(at, cell, &mut boxes);
    boxes
}

/// `Entity.moveEntity` for an entity with no step height: y, then x, then z against the block shapes.
fn move_entity(e: &mut Body, at: At, motion: [f64; 3]) -> bool {
    let world = Blocks(at);
    let original = body(e.pos, e.size);
    let boxes = Player::colliding_boxes(&world, &original.add_coord(motion));
    let (mut d, mut bb) = (motion, original);
    for axis in [1, 0, 2] {
        for b in &boxes {
            d[axis] = b.calculate_offset(&bb, axis, d[axis]);
        }
        bb = bb.offset(axis, d[axis]);
    }
    e.pos = [
        (bb.min[0] + bb.max[0]) / 2.0,
        bb.min[1],
        (bb.min[2] + bb.max[2]) / 2.0,
    ];
    let horizontal = motion[0] != d[0] || motion[2] != d[2];
    e.on_ground = motion[1] != d[1] && motion[1] < 0.0;
    stop_blocked(e, &world, motion, d);
    horizontal
}

/// Zero the velocity on each blocked axis; a landing on slime bounces instead.
fn stop_blocked(e: &mut Body, world: &Blocks, motion: [f64; 3], d: [f64; 3]) {
    if motion[0] != d[0] {
        e.vel[0] = 0.0;
    }
    if motion[1] != d[1] {
        // `BlockSlime.onLanded` bounces a non-living entity at 0.8; every other block stops it.
        if Player::block_below(world, e.pos) != 165 {
            e.vel[1] = 0.0;
        } else if e.vel[1] < 0.0 {
            e.vel[1] = -e.vel[1] * 0.8;
        }
    }
    if motion[2] != d[2] {
        e.vel[2] = 0.0;
    }
}

/// The look that `ArrowEntity`, `ThrownEntity` and the bobber take from their motion: eased by 0.2 from the last tick.
fn face_motion(e: &mut Body) {
    let [vx, vy, vz] = e.vel;
    let flat = ((vx * vx + vz * vz).sqrt() as f32) as f64;
    let yaw = (vx.atan2(vz) * 180.0 / std::f64::consts::PI) as f32;
    let pitch = (vy.atan2(flat) * 180.0 / std::f64::consts::PI) as f32;
    while pitch - e.prev_pitch < -180.0 {
        e.prev_pitch -= 360.0;
    }
    while pitch - e.prev_pitch >= 180.0 {
        e.prev_pitch += 360.0;
    }
    while yaw - e.prev_yaw < -180.0 {
        e.prev_yaw -= 360.0;
    }
    while yaw - e.prev_yaw >= 180.0 {
        e.prev_yaw += 360.0;
    }
    e.pitch = e.prev_pitch + (pitch - e.prev_pitch) * 0.2;
    e.yaw = e.prev_yaw + (yaw - e.prev_yaw) * 0.2;
}

/// `ArrowEntity.tick`.
fn arrow(e: &mut Body, at: At) {
    handle_water(e, at, false);
    if e.prev_yaw == 0.0 && e.prev_pitch == 0.0 {
        let [vx, vy, vz] = e.vel;
        e.yaw = (vx.atan2(vz) * 180.0 / std::f64::consts::PI) as f32;
        let flat = (vx * vx + vz * vz).sqrt();
        e.pitch = (vy.atan2(flat) * 180.0 / std::f64::consts::PI) as f32;
    }
    let state = at(e.pos.map(floor));
    if stuck(e, at, state) {
        return;
    }
    if let Some((point, hit)) = trace(at, e.pos, e.vel, true) {
        e.vel = std::array::from_fn(|i| f64::from((point[i] - e.pos[i]) as f32));
        let f3 = e.vel.iter().map(|v| v * v).sum::<f64>().sqrt();
        if f3 > 0.0 {
            for i in 0..3 {
                e.pos[i] -= e.vel[i] / f3 * 0.05;
            }
        }
        e.proj.in_ground = true;
        e.proj.in_state = Some(at(hit));
    }
    for i in 0..3 {
        e.pos[i] += e.vel[i];
    }
    face_motion(e);
    let drag = if e.proj.in_water { 0.6 } else { 0.99 };
    e.vel = e.vel.map(|v| v * drag);
    e.vel[1] -= 0.05;
}

/// The in-ground part of `ArrowEntity.tick`: whether the arrow sits in a block and keeps still. A block that is not
/// the one it stuck in frees it with a random slowdown.
fn stuck(e: &mut Body, at: At, state: u16) -> bool {
    let cell = e.pos.map(floor);
    if state >> 4 != 0 {
        let inside = |b: &[f64; 6]| {
            (0..3).all(|a| {
                let p = e.pos[a] - f64::from(cell[a]);
                p > b[a] && p < b[a + 3]
            })
        };
        if collision_boxes(at, cell).iter().any(inside) {
            e.proj.in_ground = true;
        }
    }
    if e.proj.in_ground && e.proj.in_state != Some(state) {
        e.proj.in_ground = false;
        for i in 0..3 {
            e.vel[i] *= e.proj.rand(e.id) * 0.2;
        }
        return true;
    }
    e.proj.in_ground
}

/// `ThrownEntity.tick`. On the client a block hit only calls `onImpact`, which spawns particles: the flight goes on
/// until the server destroys the entity.
fn thrown(e: &mut Body, at: At, gravity: f32) {
    handle_water(e, at, false);
    for i in 0..3 {
        e.pos[i] += e.vel[i];
    }
    face_motion(e);
    let drag = if e.proj.in_water { 0.8 } else { 0.99 };
    e.vel = e.vel.map(|v| v * drag);
    e.vel[1] -= f64::from(gravity);
}

/// `ItemEntity.tick`.
fn item(e: &mut Body, at: At) {
    handle_water(e, at, true);
    e.vel[1] -= GRAVITY;
    let before = e.pos.map(floor);
    move_entity(e, at, e.vel);
    if before != e.pos.map(floor) || e.age.is_multiple_of(25) {
        lava_kick(e, at);
    }
    let mut f = 0.98_f32;
    if e.on_ground {
        let id = at([floor(e.pos[0]), floor(e.pos[1]) - 1, floor(e.pos[2])]) >> 4;
        f = shape::slipperiness(id) * 0.98;
    }
    e.vel[0] *= f64::from(f);
    e.vel[1] *= 0.98;
    e.vel[2] *= f64::from(f);
    if e.on_ground {
        e.vel[1] *= -0.5;
    }
}

/// An item in lava jumps with a random sideways push.
fn lava_kick(e: &mut Body, at: At) {
    if matches!(at(e.pos.map(floor)) >> 4, 10 | 11) {
        e.vel[1] = 0.2;
        e.vel[0] = (e.proj.rand(e.id) - e.proj.rand(e.id)) * 0.2;
        e.vel[2] = (e.proj.rand(e.id) - e.proj.rand(e.id)) * 0.2;
    }
}

/// `PrimedTntEntity.tick`.
fn tnt(e: &mut Body, at: At) {
    e.vel[1] -= GRAVITY;
    move_entity(e, at, e.vel);
    e.vel = e.vel.map(|v| v * 0.98);
    if e.on_ground {
        e.vel[0] *= 0.7;
        e.vel[2] *= 0.7;
        e.vel[1] *= -0.5;
    }
    handle_water(e, at, false);
}

/// `FallingBlockEntity.tick`; landing is the server's.
fn falling(e: &mut Body, at: At) {
    e.vel[1] -= GRAVITY;
    move_entity(e, at, e.vel);
    e.vel = e.vel.map(|v| v * 0.98);
}

/// `FishingBobberEntity.tick` without the angler checks and the bite, which run on the server only.
fn bobber(e: &mut Body, at: At) {
    handle_water(e, at, false);
    if e.proj.in_ground {
        // `inBlock` is never set on the client, so the block never matches: the bobber drops out of the ground.
        e.proj.in_ground = false;
        for i in 0..3 {
            e.vel[i] *= e.proj.rand(e.id) * 0.2;
        }
    }
    if trace(at, e.pos, e.vel, false).is_some() {
        e.proj.in_ground = true;
    }
    if e.proj.in_ground {
        return;
    }
    let horizontal = move_entity(e, at, e.vel);
    face_motion(e);
    let mut drag = if e.on_ground || horizontal {
        0.5_f32
    } else {
        0.92
    };
    let depth = submerged(e, at);
    e.vel[1] += 0.04 * (depth * 2.0 - 1.0);
    if depth > 0.0 {
        drag *= 0.9;
        e.vel[1] *= 0.8;
    }
    e.vel = e.vel.map(|v| v * f64::from(drag));
}

/// The share of the bobber's box (five slices) that is in water.
fn submerged(e: &Body, at: At) -> f64 {
    let b = body(e.pos, e.size);
    let h = b.max[1] - b.min[1];
    let wet = (0..5)
        .filter(|&i| {
            let lo = b.min[1] + h * f64::from(i) / 5.0;
            let hi = b.min[1] + h * f64::from(i + 1) / 5.0;
            in_water_material(at, [b.min[0], lo, b.min[2]], [b.max[0], hi, b.max[2]])
        })
        .count();
    wet as f64 * (1.0 / 5.0)
}

/// `World.isAABBInMaterial` for water: a water cell whose surface reaches the box top.
fn in_water_material(at: At, min: [f64; 3], max: [f64; 3]) -> bool {
    (floor(min[0])..floor(max[0] + 1.0)).any(|x| {
        (floor(min[1])..floor(max[1] + 1.0)).any(|y| {
            (floor(min[2])..floor(max[2] + 1.0)).any(|z| {
                let state = at([x, y, z]);
                let meta = if state & 15 >= 8 { 0 } else { state & 15 };
                let surface = f64::from(y + 1) - f64::from(f32::from(meta + 1) / 9.0);
                fluid::is_water(state >> 4) && max[1] >= surface
            })
        })
    })
}
