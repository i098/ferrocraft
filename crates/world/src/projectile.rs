//! Client-side flight of the entities the server sends rarely: arrows, thrown objects, dropped items, primed TNT,
//! falling blocks, fishing bobbers and experience orbs. Each `step` is one `onUpdate` of the 1.8.9 class, run once per tick.
//!
//! Entity hits of arrows and bobbers run here as the client runs them (`takeDamage` answers on a client world).
//!
//! Left out, as the server owns them: damage, `setDead` timers, the first-tick block removal of a
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
    /// `inAirTicks`: ticks since the spawn, the last bounce or the last release from a block.
    air_ticks: u32,
    /// Seed of the xorshift stand-in for `Entity.rand`.
    rng: u32,
    /// A server move just snapped the entity: `settle` runs at the next tick.
    pub(crate) snapped: bool,
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
    Orb,
}

fn class(object: i8) -> Option<Class> {
    Some(match object {
        60 => Class::Arrow,
        61 | 62 | 65 => Class::Thrown(0.03),
        73 => Class::Thrown(0.05),
        75 => Class::Thrown(0.07),
        ORB => Class::Orb,
        2 => Class::Item,
        50 => Class::Tnt,
        70 => Class::Falling,
        90 => Class::Bobber,
        _ => return None,
    })
}

/// The id the projectile code gives an experience orb, which has no object id.
pub(crate) const ORB: i8 = -1;

/// Entities that the client moves between packets.
pub(crate) fn simulated(object: i8) -> bool {
    class(object).is_some()
}

/// `Entity.lerpPositionAndAngles` snaps; only the bobber (and living entities) ease over 3 ticks.
pub(crate) fn snaps(object: i8) -> bool {
    simulated(object) && object != 90
}

/// Entities whose server move runs `Entity.setPositionAndRotation2`, which lifts them out of blocks. `ArrowEntity`
/// overrides it with a plain `setPosition`.
pub(crate) fn lifts(object: i8) -> bool {
    snaps(object) && object != 60
}

/// The rest of `Entity.setPositionAndRotation2` after a server move: if the box, shrunk by 1/32 on x and z, meets a
/// block, the entity moves up so its feet stand on the highest top among the blocks it meets. A position rounded
/// down to 1/32 would otherwise leave a resting entity inside its floor, and gravity would sink it through.
// shortcut: block boxes only, no boxes of other entities (boats, minecarts); add them if a stack of those shows.
pub(crate) fn settle(pos: &mut [f64; 3], size: (f32, f32), at: At) {
    let b = body(*pos, size);
    let area = Aabb {
        min: [b.min[0] + 0.03125, b.min[1], b.min[2] + 0.03125],
        max: [b.max[0] - 0.03125, b.max[1], b.max[2] - 0.03125],
    };
    // `Block.addCollisionBoxesToList` keeps only the boxes that intersect the area.
    let mut met = Player::colliding_boxes(&Blocks(at), &area)
        .into_iter()
        .filter(|c| (0..3).all(|a| c.max[a] > area.min[a] && c.min[a] < area.max[a]))
        .peekable();
    if met.peek().is_some() {
        pos[1] += met.map(|c| c.max[1]).fold(0.0, f64::max) - b.min[1];
    }
}

struct Blocks<'a>(At<'a>);

impl World for Blocks<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        (self.0)([x, y, z])
    }
}

/// What `takeDamage` answers on a client world for the entity an arrow hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hurt {
    /// `false`: living entities (the local player included) and plain entities.
    No,
    /// `true`: other players, hanging entities, boats, minecarts and end crystals.
    Yes,
    /// `ProjectileEntity.takeDamage` (fireballs): `true` when the damage source has an attacker.
    IfAttacker,
}

/// An entity that passes `hasCollision` and `EntityFilter.NOT_SPECTATOR`, as an arrow or bobber sees it.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub id: i32,
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub hurt: Hurt,
    pub player: bool,
    /// `abilities.invulnerable` of a player.
    pub invulnerable: bool,
    pub enderman: bool,
}

/// The fields of an entity that the flight reads and writes.
pub(crate) struct Body<'a> {
    pub id: i32,
    /// `ArrowEntity.shooter` or `FishingBobberEntity.thrower`: the id from the spawn packet, 0 for none.
    pub owner: i32,
    pub targets: &'a [Target],
    /// Set when an arrow ends (`remove`) on a hit.
    pub removed: bool,
    /// `random.bowhit` at a position with a pitch, played by an arrow whose hit `takeDamage` accepted.
    pub hit_sound: Option<([f64; 3], f32)>,
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
        Class::Orb => orb(e, at),
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

/// The end of this tick's step, `pos + motion`.
fn advanced(e: &Body) -> [f64; 3] {
    std::array::from_fn(|i| e.pos[i] + e.vel[i])
}

/// `Vec3.getIntermediateWithXValue` and its Y and Z twins: the point on `a`..`c` where `axis` is `value`.
fn intermediate(a: [f64; 3], c: [f64; 3], axis: usize, value: f64) -> Option<[f64; 3]> {
    let d: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
    if d[axis] * d[axis] < 1.000_000_011_686_097_4e-7 {
        return None;
    }
    let t = (value - a[axis]) / d[axis];
    (0.0..=1.0)
        .contains(&t)
        .then(|| std::array::from_fn(|i| a[i] + d[i] * t))
}

/// `AxisAlignedBB.calculateIntercept`: the point where `a`..`c` meets a face of the box, nearest to `a`. A segment that
/// starts and ends inside the box crosses no face and misses.
fn clip(min: [f64; 3], max: [f64; 3], a: [f64; 3], c: [f64; 3]) -> Option<[f64; 3]> {
    let dist = |p: [f64; 3]| (0..3).map(|i| (p[i] - a[i]) * (p[i] - a[i])).sum::<f64>();
    let mut best: Option<[f64; 3]> = None;
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for value in [min[axis], max[axis]] {
            let Some(p) = intermediate(a, c, axis, value) else {
                continue;
            };
            let inside = |i: usize| p[i] >= min[i] && p[i] <= max[i];
            if inside(u) && inside(v) && best.is_none_or(|q| dist(p) < dist(q)) {
                best = Some(p);
            }
        }
    }
    best
}

/// The entity scan of `ArrowEntity.tick` and `FishingBobberEntity.tick`: the targets whose box meets the swept box
/// grown by 1, minus the shooter during its first 5 air ticks, whose box grown by 0.3 the step `pos`..`end` crosses
/// nearest. A start on a face (distance 0) lets the next hit take over, as in the source.
fn nearest<'a>(e: &Body<'a>, end: [f64; 3]) -> Option<&'a Target> {
    let sweep = body(e.pos, e.size).add_coord(e.vel);
    let grow = f64::from(0.3_f32);
    let (mut hit, mut best) = (None, 0.0);
    for t in e.targets {
        let meets = (0..3).all(|i| sweep.max[i] + 1.0 > t.min[i] && sweep.min[i] - 1.0 < t.max[i]);
        if !meets || (e.owner != 0 && t.id == e.owner && e.proj.air_ticks < 5) {
            continue;
        }
        let (min, max) = (t.min.map(|v| v - grow), t.max.map(|v| v + grow));
        let Some(p) = clip(min, max, e.pos, end) else {
            continue;
        };
        let d = (0..3)
            .map(|i| (p[i] - e.pos[i]) * (p[i] - e.pos[i]))
            .sum::<f64>();
        if d < best || best == 0.0 {
            (hit, best) = (Some(t), d);
        }
    }
    hit
}

/// The entity branch of `ArrowEntity.tick` as a client runs it: `takeDamage` answers by the target class. `true` plays
/// `random.bowhit` and ends the arrow (not on an enderman); `false` turns it back at a tenth of its speed.
fn hit_entity(e: &mut Body, t: &Target) {
    if t.player && t.invulnerable {
        return;
    }
    let hurt = match t.hurt {
        Hurt::Yes => true,
        Hurt::No => false,
        Hurt::IfAttacker => e.owner != 0,
    };
    if hurt {
        e.hit_sound = Some((e.pos, 1.0 / (e.proj.rand(e.id) as f32 * 0.2 + 0.9)));
        e.removed |= !t.enderman;
    } else {
        e.vel = e.vel.map(|v| v * f64::from(-0.1_f32));
        e.yaw += 180.0;
        e.prev_yaw += 180.0;
        e.proj.air_ticks = 0;
    }
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
    e.proj.air_ticks += 1;
    let block = trace(at, e.pos, e.vel, true);
    let end = block.map_or(advanced(e), |(point, _)| point);
    if let Some(target) = nearest(e, end) {
        hit_entity(e, target);
    } else if let Some((point, hit)) = block {
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
    // `0.99F`, `0.6F` and `0.05F` widen to double in the source.
    let drag = f64::from(if e.proj.in_water { 0.6_f32 } else { 0.99 });
    e.vel = e.vel.map(|v| v * drag);
    e.vel[1] -= f64::from(0.05_f32);
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
        e.proj.air_ticks = 0;
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

/// `EntityXPOrb.onUpdate`: gravity 0.03, a lava kick, drag 0.98 (the floor's slipperiness on the ground) and a
/// bounce of 0.9.
// shortcut: no pull toward the nearest player and no push out of a block; the server's packets move the orb.
fn orb(e: &mut Body, at: At) {
    e.vel[1] -= 0.029_999_999_329_447_746;
    lava_kick(e, at);
    move_entity(e, at, e.vel);
    let mut f = 0.98_f32;
    if e.on_ground {
        let id = at([floor(e.pos[0]), floor(e.pos[1]) - 1, floor(e.pos[2])]) >> 4;
        f = shape::slipperiness(id) * 0.98;
    }
    e.vel[0] *= f64::from(f);
    e.vel[1] *= 0.98;
    e.vel[2] *= f64::from(f);
    if e.on_ground {
        e.vel[1] *= -0.9;
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
        e.proj.air_ticks = 0;
        for i in 0..3 {
            e.vel[i] *= e.proj.rand(e.id) * 0.2;
        }
    } else {
        e.proj.air_ticks += 1;
    }
    let block = trace(at, e.pos, e.vel, false);
    let end = block.map_or(advanced(e), |(point, _)| point);
    // An entity in the way replaces the block as the hit; its `takeDamage` and `hookedEntity` run on the server.
    if block.is_some() && nearest(e, end).is_none() {
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

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: &dyn Fn([i32; 3]) -> u16 = &|_| 0;

    /// A player box (0.6 x 1.8) with its feet at (0.2, 69.5, 1.5).
    fn target(id: i32, hurt: Hurt) -> Target {
        Target {
            id,
            min: [-0.1, 69.5, 1.2],
            max: [0.5, 71.3, 1.8],
            hurt,
            player: false,
            invulnerable: false,
            enderman: false,
        }
    }

    fn body(object_size: (f32, f32), pos: [f64; 3], vel: [f64; 3], targets: &[Target]) -> Body<'_> {
        Body {
            id: 5,
            owner: 0,
            targets,
            removed: false,
            hit_sound: None,
            age: 0,
            size: object_size,
            pos,
            vel,
            yaw: 10.0,
            pitch: 5.0,
            prev_yaw: 10.0,
            prev_pitch: 5.0,
            on_ground: false,
            proj: State::default(),
        }
    }

    fn arrow_at<'a>(targets: &'a [Target]) -> Body<'a> {
        body((0.5, 0.5), [0.0, 70.0, 0.0], [0.3, -0.1, 2.5], targets)
    }

    // Expected values below come from running the remapped 1.8.9 jar: `Box.grown(0.3f).clip`, then the arithmetic
    // of `ArrowEntity.tick` after its entity branch (`* -0.10000000149011612`, `+= motion`, `* 0.99f`, `-= 0.05f`).
    const JAR_VEL_FLY: [f64; 3] = [0.29700000286102296, -0.1490000016987324, 2.475000023841858];
    const JAR_POS_BOUNCE: [f64; 3] = [-0.030000000447034835, 70.010000000149, -0.2500000037252903];
    const JAR_VEL_BOUNCE: [f64; 3] = [
        -0.029700000728666787,
        -0.04010000050216913,
        -0.24750000607222322,
    ];

    #[test]
    fn clip_matches_the_jar() {
        let grow = f64::from(0.3_f32);
        let (min, max) = (
            [-0.1 - grow, 69.5 - grow, 1.2 - grow],
            [0.5 + grow, 71.3 + grow, 1.8 + grow],
        );
        let hit = clip(min, max, [0.0, 70.0, 0.0], [0.3, 69.9, 2.5]);
        assert_eq!(
            hit,
            Some([0.10799999856948851, 69.96400000047684, 0.899999988079071])
        );
        // Both ends inside: no face is crossed, so no hit.
        assert_eq!(clip(min, max, [0.1, 70.0, 1.2], [0.2, 70.0, 1.3]), None);
        // Starting inside and leaving: the exit face.
        assert_eq!(
            clip(min, max, [0.1, 70.0, 1.2], [0.1, 70.0, 2.5]),
            Some([0.1, 70.0, 2.1000000119209288])
        );
    }

    #[test]
    fn arrow_turns_back_at_an_entity_the_client_cannot_hurt() {
        let targets = [target(9, Hurt::No)];
        let mut e = arrow_at(&targets);
        e.proj.air_ticks = 20;
        step(60, &mut e, AIR);
        assert_eq!((e.pos, e.vel), (JAR_POS_BOUNCE, JAR_VEL_BOUNCE));
        assert_eq!(e.proj.air_ticks, 0);
        assert!(!e.removed && e.hit_sound.is_none() && !e.proj.in_ground);
        // `prevRotationYaw += 180` (10 to 190), then the look easing wraps it back by 360.
        assert_eq!(e.prev_yaw, -170.0);
    }

    #[test]
    fn arrow_ends_at_an_entity_the_client_hurts() {
        let targets = [target(9, Hurt::Yes)];
        let mut e = arrow_at(&targets);
        step(60, &mut e, AIR);
        assert!(e.removed);
        let (pos, pitch) = e.hit_sound.expect("random.bowhit");
        assert_eq!(pos, [0.0, 70.0, 0.0]);
        assert!((1.0 / 1.1..=1.0 / 0.9).contains(&pitch), "{pitch}");
        // The motion is not turned back.
        assert_eq!(e.vel, JAR_VEL_FLY);

        let mut enderman = [target(9, Hurt::Yes)];
        enderman[0].enderman = true;
        let mut e = arrow_at(&enderman);
        step(60, &mut e, AIR);
        assert!(!e.removed && e.hit_sound.is_some());
    }

    #[test]
    fn arrow_passes_a_player_with_invulnerable_abilities() {
        let mut t = target(9, Hurt::No);
        (t.player, t.invulnerable) = (true, true);
        let targets = [t];
        let mut e = arrow_at(&targets);
        step(60, &mut e, AIR);
        assert_eq!(e.vel, JAR_VEL_FLY);
    }

    #[test]
    fn arrow_skips_its_shooter_for_the_first_four_scans() {
        let targets = [target(7, Hurt::No)];
        for (air_ticks, bounces) in [(0, false), (3, false), (4, true)] {
            let mut e = arrow_at(&targets);
            e.owner = 7;
            e.proj.air_ticks = air_ticks;
            step(60, &mut e, AIR);
            assert_eq!(e.vel == JAR_VEL_BOUNCE, bounces, "air ticks {air_ticks}");
            assert_eq!(e.vel == JAR_VEL_FLY, !bounces, "air ticks {air_ticks}");
        }
    }

    #[test]
    fn bobber_does_not_settle_in_a_block_behind_an_entity() {
        let floor: &dyn Fn([i32; 3]) -> u16 = &|p| if p[1] < 70 { 1 << 4 } else { 0 };
        let mut mob = target(9, Hurt::No);
        (mob.min, mob.max) = ([0.2, 69.0, 0.2], [0.8, 69.8, 0.8]);
        for (targets, settles) in [(vec![], true), (vec![mob], false)] {
            let mut e = body((0.25, 0.25), [0.5, 70.2, 0.5], [0.0, -0.4, 0.0], &targets);
            e.proj.air_ticks = 20;
            step(90, &mut e, floor);
            assert_eq!(e.proj.in_ground, settles);
        }
    }

    #[test]
    fn bobber_skips_its_thrower_for_the_first_four_scans() {
        let floor: &dyn Fn([i32; 3]) -> u16 = &|p| if p[1] < 70 { 1 << 4 } else { 0 };
        let mut me = target(7, Hurt::No);
        (me.min, me.max) = ([0.2, 69.0, 0.2], [0.8, 69.8, 0.8]);
        let targets = [me];
        for (air_ticks, settles) in [(0, true), (3, true), (4, false)] {
            let mut e = body((0.25, 0.25), [0.5, 70.2, 0.5], [0.0, -0.4, 0.0], &targets);
            e.owner = 7;
            e.proj.air_ticks = air_ticks;
            step(90, &mut e, floor);
            assert_eq!(e.proj.in_ground, settles, "air ticks {air_ticks}");
        }
    }
}
