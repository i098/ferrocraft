//! Player movement physics of Minecraft 1.8.9, ported method by method from vanilla.
//!
//! Ported: `PlayerEntity.mobTick` (the movement part of `onLivingUpdate`), `moveEntityWithHeading`
//! with its water, lava and ladder branches, `moveFlying`, `jump`, `jumpInWater`, `handleWaterMovement`
//! (water flow push) and `Entity.moveEntity` (axis-wise collision with the real block shapes,
//! step-up, cobweb, soul sand and slime), all with the exact `f32`/`f64` arithmetic of the Java
//! code, and the flying branch of `moveEntityWithHeading` with the vertical keys of
//! `EntityPlayerSP.onLivingUpdate`. Not ported yet: potion effects other than jump boost, depth
//! strider, riding.
//! The differential tests check every tick against the real 1.8.9 jar through `oracle`.

pub mod chunk;
pub mod dig;
pub mod effects;
pub mod entities;
pub mod entity_sound;
pub mod fluid;
pub mod inventory;
pub mod motion_sound;
pub mod place;
pub mod potion_brew;
pub mod shape;

use std::sync::LazyLock;

/// Block lookup for the simulation: the 1.8 block state, `id << 4 | metadata`.
pub trait World {
    fn block(&self, x: i32, y: i32, z: i32) -> u16;
}

/// Player input for one tick, as the client sets it before `onLivingUpdate`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub forward: f32,
    pub strafe: f32,
    pub jump: bool,
    pub sprint: bool,
    /// The sneak key: flying down; walking at 0.3 speed and held at block edges.
    pub sneak: bool,
    /// `isUsingItem()` while not riding: blocking, eating, drawing a bow. Walks at 0.2 speed.
    pub using_item: bool,
    /// `PlayerAbilities.flySpeed` while `isFlying`, else `None`.
    pub fly_speed: Option<f32>,
}

/// `MathHelper.sin` and `cos`: the vanilla lookup table, not `f32::sin`.
static SIN_TABLE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    (0..65536)
        .map(|i| (f64::from(i) * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
        .collect()
});

fn sin(v: f32) -> f32 {
    SIN_TABLE[((v * 10430.378_f32) as i32 & 65535) as usize]
}

fn cos(v: f32) -> f32 {
    SIN_TABLE[((v * 10430.378_f32 + 16384.0_f32) as i32 & 65535) as usize]
}

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

/// `AxisAlignedBB`; axis 0 is x, 1 is y, 2 is z.
#[derive(Clone, Copy, Debug)]
struct Aabb {
    min: [f64; 3],
    max: [f64; 3],
}

impl Aabb {
    fn add_coord(self, d: [f64; 3]) -> Aabb {
        let mut b = self;
        for (a, d) in d.into_iter().enumerate() {
            if d < 0.0 {
                b.min[a] += d;
            } else if d > 0.0 {
                b.max[a] += d;
            }
        }
        b
    }

    fn offset(self, axis: usize, d: f64) -> Aabb {
        let mut b = self;
        b.min[axis] += d;
        b.max[axis] += d;
        b
    }

    /// `calculateXOffset`, `calculateYOffset` and `calculateZOffset` in one.
    fn calculate_offset(&self, other: &Aabb, axis: usize, mut offset: f64) -> f64 {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        let overlaps = |i: usize| other.max[i] > self.min[i] && other.min[i] < self.max[i];
        if overlaps(a) && overlaps(b) {
            if offset > 0.0 && other.max[axis] <= self.min[axis] {
                let d = self.min[axis] - other.max[axis];
                if d < offset {
                    offset = d;
                }
            } else if offset < 0.0 && other.min[axis] >= self.max[axis] {
                let d = self.max[axis] - other.min[axis];
                if d > offset {
                    offset = d;
                }
            }
        }
        offset
    }
}

/// A player entity: the state that `mobTick` reads and writes.
#[derive(Clone, Debug)]
pub struct Player {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub on_ground: bool,
    pub sprinting: bool,
    /// `isSneaking()`, set from the input each tick.
    pub sneaking: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    /// `generic.movementSpeed` value for this tick, sprint modifier included; `None` before the
    /// engine sets it.
    pub speed_attr: Option<f64>,
    /// Jump Boost level (`amplifier + 1`), 0 without the effect.
    pub jump_boost: f32,
    jump_movement_factor: f32,
    jump_ticks: i32,
    /// `Entity.inWater`, set by `handleWaterMovement` at the start of each tick.
    in_water: bool,
    /// `isPushedByWater()` of this tick: false while flying.
    pushed_by_water: bool,
    /// `Entity.isInWeb`, set by a cobweb the last move touched; the next move is slowed.
    in_web: bool,
    bb: Aabb,
}

const WIDTH: f32 = 0.6;
const HEIGHT: f32 = 1.8;
// vanilla `stepHeight` is a float (0.6F)
const STEP_HEIGHT: f64 = 0.6_f32 as f64;
/// `PlayerAbilities.walkSpeed` and the base of `generic.movementSpeed`.
const WALK_SPEED: f32 = 0.1;
/// `PlayerAbilities.flyingSpeed`; the air acceleration of a walking player.
const FLYING_SPEED: f32 = 0.02;
const SPRINT_BOOST: f64 = 0.300_000_011_920_928_96;

impl Player {
    /// A player standing at `pos`.
    pub fn new(pos: [f64; 3], yaw: f32) -> Player {
        let mut p = Player {
            pos,
            vel: [0.0; 3],
            yaw,
            on_ground: false,
            sprinting: false,
            sneaking: false,
            collided_horizontally: false,
            collided_vertically: false,
            speed_attr: None,
            jump_boost: 0.0,
            jump_movement_factor: 0.02,
            jump_ticks: 0,
            in_water: false,
            pushed_by_water: true,
            in_web: false,
            bb: Aabb {
                min: [0.0; 3],
                max: [0.0; 3],
            },
        };
        p.set_position(pos);
        p
    }

    /// `Entity.setPosition`.
    fn set_position(&mut self, pos: [f64; 3]) {
        self.pos = pos;
        let half = f64::from(WIDTH / 2.0);
        self.bb = Aabb {
            min: [pos[0] - half, pos[1], pos[2] - half],
            max: [pos[0] + half, pos[1] + f64::from(HEIGHT), pos[2] + half],
        };
    }

    /// One game tick: `Entity.handleWaterMovement` of `onEntityUpdate`, then `PlayerEntity.mobTick`.
    pub fn tick(&mut self, world: &dyn World, input: Input) {
        self.sprinting = input.sprint;
        self.sneaking = input.sneak;
        self.pushed_by_water = input.fly_speed.is_none();
        self.handle_water_movement(world);
        self.apply_jump_and_flight_keys(world, input);
        // `MovementInputFromOptions` (0.3 while sneaking), then `onLivingUpdate` (0.2 while using an item).
        let (mut strafe, mut forward) = (input.strafe, input.forward);
        if input.sneak {
            strafe *= 0.3_f32;
            forward *= 0.3_f32;
        }
        if input.using_item {
            strafe *= 0.2_f32;
            forward *= 0.2_f32;
        }
        self.move_with_heading(world, strafe * 0.98, forward * 0.98, input.fly_speed);

        self.jump_movement_factor = FLYING_SPEED;
        if self.sprinting {
            self.jump_movement_factor =
                (f64::from(self.jump_movement_factor) + f64::from(FLYING_SPEED) * 0.3) as f32;
        }
    }

    /// The vertical flight keys and the jump key of `EntityPlayerSP.onLivingUpdate`, before `mobTick`.
    fn apply_jump_and_flight_keys(&mut self, world: &dyn World, input: Input) {
        if let Some(fly) = input.fly_speed {
            let lift = f64::from(fly * 3.0_f32);
            if input.sneak {
                self.vel[1] -= lift;
            }
            if input.jump {
                self.vel[1] += lift;
            }
        }
        if self.jump_ticks > 0 {
            self.jump_ticks -= 1;
        }
        for v in &mut self.vel {
            if v.abs() < 0.005 {
                *v = 0.0;
            }
        }
        if !input.jump {
            self.jump_ticks = 0;
        } else if self.in_water || self.in_lava(world) {
            // `jumpInWater` and `jumpInLava` add 0.04F.
            self.vel[1] += f64::from(0.04_f32);
        } else if self.on_ground && self.jump_ticks == 0 {
            self.jump();
            self.jump_ticks = 10;
        }
    }

    /// `PlayerEntity.getSpeed`: the movement speed attribute. Without a server value, the walk speed
    /// with the sprint modifier (operation 2).
    fn speed(&self) -> f32 {
        if let Some(attr) = self.speed_attr {
            return attr as f32;
        }
        let mut speed = f64::from(WALK_SPEED);
        if self.sprinting {
            speed *= 1.0 + SPRINT_BOOST;
        }
        speed as f32
    }

    /// `LivingEntity.jump`.
    fn jump(&mut self) {
        self.vel[1] = f64::from(0.42_f32);
        self.vel[1] += f64::from(self.jump_boost * 0.1_f32);
        if self.sprinting {
            let f = self.yaw * 0.017_453_292_f32;
            self.vel[0] -= f64::from(sin(f) * 0.2_f32);
            self.vel[2] += f64::from(cos(f) * 0.2_f32);
        }
    }

    /// `EntityPlayer.moveEntityWithHeading`. A flying player (`fly` is the fly speed) accelerates in the
    /// air by that speed, doubled while sprinting, and keeps 0.6 of its vertical speed instead of gravity.
    fn move_with_heading(
        &mut self,
        world: &dyn World,
        strafe: f32,
        forward: f32,
        fly: Option<f32>,
    ) {
        let (kept_y, kept_factor) = (self.vel[1], self.jump_movement_factor);
        if let Some(fly) = fly {
            self.jump_movement_factor = fly * if self.sprinting { 2.0 } else { 1.0 };
        }
        if fly.is_some() {
            self.walk(world, strafe, forward);
            self.vel[1] = kept_y * 0.6;
            self.jump_movement_factor = kept_factor;
        } else if self.in_water {
            self.swim(world, strafe, forward);
        } else if self.in_lava(world) {
            self.wade_lava(world, strafe, forward);
        } else {
            self.walk(world, strafe, forward);
        }
    }

    /// The land and air branch of `EntityLivingBase.moveEntityWithHeading`, with the ladder.
    fn walk(&mut self, world: &dyn World, strafe: f32, forward: f32) {
        let mut friction = 0.91_f32;
        if self.on_ground {
            friction = self.ground_slipperiness(world) * 0.91;
        }
        let f = 0.162_771_36_f32 / (friction * friction * friction);
        let accel = if self.on_ground {
            self.speed() * f
        } else {
            self.jump_movement_factor
        };
        self.move_flying(strafe, forward, accel);

        friction = 0.91_f32;
        if self.on_ground {
            friction = self.ground_slipperiness(world) * 0.91;
        }
        if self.is_climbing(world) {
            let limit = f64::from(0.15_f32);
            self.vel[0] = self.vel[0].clamp(-limit, limit);
            self.vel[2] = self.vel[2].clamp(-limit, limit);
            if self.vel[1] < -0.15 {
                self.vel[1] = -0.15;
            }
            if self.sneaking && self.vel[1] < 0.0 {
                self.vel[1] = 0.0;
            }
        }
        self.move_entity(world, self.vel);
        if self.collided_horizontally && self.is_climbing(world) {
            self.vel[1] = 0.2;
        }
        self.vel[1] -= 0.08;
        self.vel[1] *= f64::from(0.98_f32);
        self.vel[0] *= f64::from(friction);
        self.vel[2] *= f64::from(friction);
    }

    /// The water branch of `moveEntityWithHeading` without depth strider.
    fn swim(&mut self, world: &dyn World, strafe: f32, forward: f32) {
        let y = self.pos[1];
        self.move_flying(strafe, forward, 0.02);
        self.move_entity(world, self.vel);
        let drag = f64::from(0.8_f32);
        self.vel[0] *= drag;
        self.vel[1] *= drag;
        self.vel[2] *= drag;
        self.vel[1] -= 0.02;
        self.climb_out_of_liquid(world, y);
    }

    /// The lava branch of `moveEntityWithHeading`.
    fn wade_lava(&mut self, world: &dyn World, strafe: f32, forward: f32) {
        let y = self.pos[1];
        self.move_flying(strafe, forward, 0.02);
        self.move_entity(world, self.vel);
        for v in &mut self.vel {
            *v *= 0.5;
        }
        self.vel[1] -= 0.02;
        self.climb_out_of_liquid(world, y);
    }

    /// A wall in the way and a free spot 0.6 above the surface: kick out of the liquid.
    fn climb_out_of_liquid(&mut self, world: &dyn World, start_y: f64) {
        if self.collided_horizontally {
            let up = self.vel[1] + 0.600_000_023_841_857_9 - self.pos[1] + start_y;
            if self.can_move(world, [self.vel[0], up, self.vel[2]]) {
                self.vel[1] = 0.300_000_011_920_928_96;
            }
        }
    }

    /// `Block.slipperiness` of the block under the feet.
    fn ground_slipperiness(&self, world: &dyn World) -> f32 {
        let below = world.block(
            floor(self.pos[0]),
            floor(self.bb.min[1]) - 1,
            floor(self.pos[2]),
        );
        shape::slipperiness(below >> 4)
    }

    /// `LivingEntity.isClimbing`: the feet are in a ladder or a vine.
    fn is_climbing(&self, world: &dyn World) -> bool {
        let id = world.block(
            floor(self.pos[0]),
            floor(self.bb.min[1]),
            floor(self.pos[2]),
        ) >> 4;
        matches!(id, 65 | 106)
    }

    /// `Entity.handleWaterMovement`: set `in_water` and let flowing water push the player.
    fn handle_water_movement(&mut self, world: &dyn World) {
        let (min, max) = (self.bb.min, self.bb.max);
        let area = Aabb {
            min: [
                min[0] + 0.001,
                min[1] + 0.400_000_005_960_464_5 + 0.001,
                min[2] + 0.001,
            ],
            max: [
                max[0] - 0.001,
                max[1] - 0.400_000_005_960_464_5 - 0.001,
                max[2] - 0.001,
            ],
        };
        let (in_water, push) = fluid::water_drag(
            &|p| world.block(p[0], p[1], p[2]),
            area.min,
            area.max,
            self.pushed_by_water,
        );
        for (v, p) in self.vel.iter_mut().zip(push) {
            *v += p;
        }
        self.in_water = in_water;
    }

    /// `Entity.isInLava`.
    fn in_lava(&self, world: &dyn World) -> bool {
        let (min, max) = (self.bb.min, self.bb.max);
        let (grow, drop) = (0.100_000_001_490_116_12, 0.400_000_005_960_464_5);
        let lo = [min[0] + grow, min[1] + drop, min[2] + grow];
        let hi = [max[0] - grow, max[1] - drop, max[2] - grow];
        Self::any_block(world, lo, hi, |id| matches!(id, 10 | 11))
    }

    /// A block of the cells touched by the box `lo` to `hi` satisfies `test` (`World.isMaterialInBB`).
    fn any_block(
        world: &dyn World,
        lo: [f64; 3],
        hi: [f64; 3],
        test: impl Fn(u16) -> bool,
    ) -> bool {
        let cells = |a: usize| floor(lo[a])..floor(hi[a] + 1.0);
        cells(0).any(|x| cells(1).any(|y| cells(2).any(|z| test(world.block(x, y, z) >> 4))))
    }

    /// `Entity.canMove`: the box moved by `d` touches no block shape and no liquid.
    fn can_move(&self, world: &dyn World, d: [f64; 3]) -> bool {
        let moved = Aabb {
            min: [
                self.bb.min[0] + d[0],
                self.bb.min[1] + d[1],
                self.bb.min[2] + d[2],
            ],
            max: [
                self.bb.max[0] + d[0],
                self.bb.max[1] + d[1],
                self.bb.max[2] + d[2],
            ],
        };
        !Self::collides(world, &moved)
            && !Self::any_block(world, moved.min, moved.max, shape::is_liquid)
    }

    /// `World.getCollidingBoundingBoxes` is not empty: a block shape overlaps `area`.
    fn collides(world: &dyn World, area: &Aabb) -> bool {
        Self::colliding_boxes(world, area)
            .iter()
            .any(|o| (0..3).all(|a| area.max[a] > o.min[a] && area.min[a] < o.max[a]))
    }

    /// `moveFlying`.
    fn move_flying(&mut self, mut strafe: f32, mut forward: f32, friction: f32) {
        let mut f = strafe * strafe + forward * forward;
        if f >= 1.0e-4_f32 {
            f = f64::from(f).sqrt() as f32;
            if f < 1.0 {
                f = 1.0;
            }
            f = friction / f;
            strafe *= f;
            forward *= f;
            let (s, c) = (
                sin(self.yaw * std::f32::consts::PI / 180.0),
                cos(self.yaw * std::f32::consts::PI / 180.0),
            );
            self.vel[0] += f64::from(strafe * c - forward * s);
            self.vel[2] += f64::from(forward * c + strafe * s);
        }
    }

    /// `World.getCollidingBoundingBoxes`: the shapes of the blocks around `area`, touching ones included.
    fn colliding_boxes(world: &dyn World, area: &Aabb) -> Vec<Aabb> {
        let (x0, x1) = (floor(area.min[0]), floor(area.max[0] + 1.0));
        let (y0, y1) = (floor(area.min[1]), floor(area.max[1] + 1.0));
        let (z0, z1) = (floor(area.min[2]), floor(area.max[2] + 1.0));
        let at = |p: [i32; 3]| world.block(p[0], p[1], p[2]);
        let mut boxes = Vec::new();
        let mut cell = Vec::new();
        for x in x0..x1 {
            for z in z0..z1 {
                for y in (y0 - 1)..y1 {
                    cell.clear();
                    shape::push_boxes(&at, [x, y, z], &mut cell);
                    let o = [f64::from(x), f64::from(y), f64::from(z)];
                    boxes.extend(cell.iter().map(|b| Aabb {
                        min: [o[0] + b[0], o[1] + b[1], o[2] + b[2]],
                        max: [o[0] + b[3], o[1] + b[4], o[2] + b[5]],
                    }));
                }
            }
        }
        boxes
    }

    /// The step-up part of `Entity.moveEntity`: the stepped move when it goes farther than `d`.
    fn step(
        world: &dyn World,
        original: &Aabb,
        motion: [f64; 3],
        kept_d: [f64; 3],
        kept_bb: Aabb,
    ) -> ([f64; 3], Aabb) {
        let boxes = Self::colliding_boxes(
            world,
            &original.add_coord([motion[0], STEP_HEIGHT, motion[2]]),
        );

        let rise = |sweep: &Aabb| {
            boxes
                .iter()
                .fold(STEP_HEIGHT, |off, b| b.calculate_offset(sweep, 1, off))
        };
        // Candidate A: rise with the box stretched over the horizontal move, then slide.
        let rise_a = rise(&original.add_coord([motion[0], 0.0, motion[2]]));
        let (a_x, a_z, bb_a) = Self::slide(&boxes, original.offset(1, rise_a), motion);
        // Candidate B: rise with the plain box, then slide.
        let rise_b = rise(original);
        let (b_x, b_z, bb_b) = Self::slide(&boxes, original.offset(1, rise_b), motion);

        let (x, z, rise, cur) = if a_x * a_x + a_z * a_z > b_x * b_x + b_z * b_z {
            (a_x, a_z, rise_a, bb_a)
        } else {
            (b_x, b_z, rise_b, bb_b)
        };
        let mut y = -rise;
        for b in &boxes {
            y = b.calculate_offset(&cur, 1, y);
        }
        if kept_d[0] * kept_d[0] + kept_d[2] * kept_d[2] >= x * x + z * z {
            return (kept_d, kept_bb);
        }
        ([x, y, z], cur.offset(1, y))
    }

    /// `Entity.moveEntity`.
    fn move_entity(&mut self, world: &dyn World, mut motion: [f64; 3]) {
        if self.in_web {
            self.in_web = false;
            motion = [
                motion[0] * 0.25,
                motion[1] * 0.050_000_000_745_058_06,
                motion[2] * 0.25,
            ];
            self.vel = [0.0; 3];
        }
        let original = self.bb;
        if self.on_ground && self.sneaking {
            motion = self.edge_guard(world, motion);
        }
        let mut d = motion;
        let boxes = Self::colliding_boxes(world, &original.add_coord(d));
        let mut bb = original;
        for axis in [1, 0, 2] {
            for b in &boxes {
                d[axis] = b.calculate_offset(&bb, axis, d[axis]);
            }
            bb = bb.offset(axis, d[axis]);
        }

        let can_step = self.on_ground || (motion[1] != d[1] && motion[1] < 0.0);
        if STEP_HEIGHT > 0.0 && can_step && (motion[0] != d[0] || motion[2] != d[2]) {
            (d, bb) = Self::step(world, &original, motion, d, bb);
        }

        self.pos = [
            (bb.min[0] + bb.max[0]) / 2.0,
            bb.min[1],
            (bb.min[2] + bb.max[2]) / 2.0,
        ];
        self.bb = bb;
        self.set_collisions(world, motion, d);
        self.block_collisions(world);
    }

    /// The sneak part of `Entity.moveEntity`: step back by 0.05 until a block is under the new spot.
    fn edge_guard(&self, world: &dyn World, mut m: [f64; 3]) -> [f64; 3] {
        const STEP: f64 = 0.05;
        let empty = |x: f64, z: f64| {
            let b = self.bb.offset(0, x);
            !Self::collides(world, &b.offset(1, -1.0).offset(2, z))
        };
        let back = |v: f64| {
            if (-STEP..STEP).contains(&v) {
                0.0
            } else if v > 0.0 {
                v - STEP
            } else {
                v + STEP
            }
        };
        while m[0] != 0.0 && empty(m[0], 0.0) {
            m[0] = back(m[0]);
        }
        while m[2] != 0.0 && empty(0.0, m[2]) {
            m[2] = back(m[2]);
        }
        while m[0] != 0.0 && m[2] != 0.0 && empty(m[0], m[2]) {
            m[0] = back(m[0]);
            m[2] = back(m[2]);
        }
        m
    }

    /// Collision flags and stopped velocity after moving `d` of the wanted `motion`; the block
    /// under the feet decides what a landing does to the vertical speed (`Block.onLanded`).
    fn set_collisions(&mut self, world: &dyn World, motion: [f64; 3], d: [f64; 3]) {
        self.collided_horizontally = motion[0] != d[0] || motion[2] != d[2];
        self.collided_vertically = motion[1] != d[1];
        self.on_ground = self.collided_vertically && motion[1] < 0.0;
        // `updateFallState` runs `handleWaterMovement` at the new position when not yet in water.
        if !self.in_water {
            self.handle_water_movement(world);
        }
        let under = Self::block_below(world, self.pos);
        if motion[0] != d[0] {
            self.vel[0] = 0.0;
        }
        if motion[1] != d[1] {
            // `BlockSlime.onLanded` bounces a player that is not sneaking.
            if under != 165 || self.sneaking {
                self.vel[1] = 0.0;
            } else if self.vel[1] < 0.0 {
                self.vel[1] = -self.vel[1];
            }
        }
        if motion[2] != d[2] {
            self.vel[2] = 0.0;
        }
    }

    /// The block id that `moveEntity` passes to `updateFallState`: the cell 0.2 under the feet; for
    /// air, a fence post or a wall below counts, as their boxes reach 0.5 above the block.
    fn block_below(world: &dyn World, pos: [f64; 3]) -> u16 {
        let (x, z) = (floor(pos[0]), floor(pos[2]));
        let y = floor(pos[1] - 0.200_000_002_980_232_24);
        let under = world.block(x, y, z) >> 4;
        let below = world.block(x, y - 1, z) >> 4;
        if under == 0 && matches!(below, 85 | 107 | 113 | 139 | 183..=192) {
            below
        } else {
            under
        }
    }

    /// `Block.onEntityCollidedWithBlock` of cobweb, soul sand and slime for one cell.
    fn collide_with(&mut self, id: u16) {
        match id {
            30 => self.in_web = true,
            88 => {
                self.vel[0] *= 0.4;
                self.vel[2] *= 0.4;
            }
            165 if self.vel[1].abs() < 0.1 && !self.sneaking => {
                let slow = 0.4 + self.vel[1].abs() * 0.2;
                self.vel[0] *= slow;
                self.vel[2] *= slow;
            }
            _ => {}
        }
    }

    /// `Entity.doBlockCollisions`: cobweb, soul sand and slime act on a player inside their cell.
    fn block_collisions(&mut self, world: &dyn World) {
        let lo = [0, 1, 2].map(|a| floor(self.bb.min[a] + 0.001));
        let hi = [0, 1, 2].map(|a| floor(self.bb.max[a] - 0.001));
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                for x in lo[0]..=hi[0] {
                    self.collide_with(world.block(x, y, z) >> 4);
                }
            }
        }
    }

    /// The x then z slide of the step-up test; returns the moves and the final box.
    fn slide(boxes: &[Aabb], start: Aabb, motion: [f64; 3]) -> (f64, f64, Aabb) {
        let mut bb = start;
        let mut x = motion[0];
        for b in boxes {
            x = b.calculate_offset(&bb, 0, x);
        }
        bb = bb.offset(0, x);
        let mut z = motion[2];
        for b in boxes {
            z = b.calculate_offset(&bb, 2, z);
        }
        (x, z, bb.offset(2, z))
    }
}
#[cfg(test)]
mod tests;
