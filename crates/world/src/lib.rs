//! Player movement physics of Minecraft 1.8.9, ported method by method from vanilla.
//!
//! Ported: `PlayerEntity.mobTick` (the movement part of `onLivingUpdate`), `moveEntityWithHeading`,
//! `moveFlying`, `jump` and `Entity.moveEntity` (axis-wise collision and step-up), all with
//! the exact `f32`/`f64` arithmetic of the Java code, and the flying branch of `moveEntityWithHeading`
//! with the vertical keys of `EntityPlayerSP.onLivingUpdate`. Not ported yet: water, lava, ladders,
//! webs, sneaking, potion effects, soul sand and slime, entity pushing.
//! The differential tests check every tick against the real 1.8.9 jar through `oracle`.

pub mod chunk;
pub mod dig;
pub mod effects;
pub mod entities;
pub mod entity_sound;
pub mod inventory;
pub mod motion_sound;
pub mod place;
pub mod potion_brew;

use std::sync::LazyLock;

/// Block kinds that the differential harness can place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Air,
    Stone,
    Ice,
}

impl Block {
    /// `Block.slipperiness`: 0.98 for ice, 0.6 for every other block.
    fn slipperiness(self) -> f32 {
        match self {
            Block::Ice => 0.98,
            _ => 0.6,
        }
    }

    /// Every block that is not air is a full cube here.
    fn is_solid(self) -> bool {
        self != Block::Air
    }
}

/// Block lookup for the simulation.
pub trait World {
    fn block(&self, x: i32, y: i32, z: i32) -> Block;
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

    /// One game tick: `PlayerEntity.mobTick` for a player that is on land and not flying.
    pub fn tick(&mut self, world: &dyn World, input: Input) {
        self.sprinting = input.sprint;
        self.sneaking = input.sneak;
        // `EntityPlayerSP.onLivingUpdate`: the vertical flight keys, before `mobTick`.
        if let Some(fly) = input.fly_speed {
            if input.sneak {
                self.vel[1] -= f64::from(fly * 3.0_f32);
            }
            if input.jump {
                self.vel[1] += f64::from(fly * 3.0_f32);
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
        if input.jump {
            if self.on_ground && self.jump_ticks == 0 {
                self.jump();
                self.jump_ticks = 10;
            }
        } else {
            self.jump_ticks = 0;
        }
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

    /// `EntityPlayer.moveEntityWithHeading` without the water, lava and ladder branches. A flying player
    /// (`fly` is the fly speed) accelerates in the air by that speed, doubled while sprinting, and keeps
    /// 0.6 of its vertical speed instead of gravity.
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
        let mut friction = 0.91_f32;
        if self.on_ground {
            let below = world.block(
                floor(self.pos[0]),
                floor(self.bb.min[1]) - 1,
                floor(self.pos[2]),
            );
            friction = below.slipperiness() * 0.91;
        }
        let f = 0.162_771_36_f32 / (friction * friction * friction);
        let accel = if self.on_ground {
            self.speed() * f
        } else {
            self.jump_movement_factor
        };
        self.move_flying(strafe, forward, accel);

        self.move_entity(world, self.vel);
        self.vel[1] -= 0.08;
        self.vel[1] *= f64::from(0.98_f32);
        self.vel[0] *= f64::from(friction);
        self.vel[2] *= f64::from(friction);
        if fly.is_some() {
            self.vel[1] = kept_y * 0.6;
            self.jump_movement_factor = kept_factor;
        }
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

    /// `World.getCollidingBoundingBoxes` for full-cube blocks.
    fn colliding_boxes(world: &dyn World, area: &Aabb) -> Vec<Aabb> {
        let (x0, x1) = (floor(area.min[0]), floor(area.max[0] + 1.0));
        let (y0, y1) = (floor(area.min[1]), floor(area.max[1] + 1.0));
        let (z0, z1) = (floor(area.min[2]), floor(area.max[2] + 1.0));
        let mut boxes = Vec::new();
        for x in x0..x1 {
            for z in z0..z1 {
                for y in (y0 - 1)..y1 {
                    if world.block(x, y, z).is_solid() {
                        let min = [f64::from(x), f64::from(y), f64::from(z)];
                        boxes.push(Aabb {
                            min,
                            max: [min[0] + 1.0, min[1] + 1.0, min[2] + 1.0],
                        });
                    }
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
        self.set_collisions(motion, d);
    }

    /// The sneak part of `Entity.moveEntity`: step back by 0.05 until a block is under the new spot.
    fn edge_guard(&self, world: &dyn World, mut m: [f64; 3]) -> [f64; 3] {
        const STEP: f64 = 0.05;
        let empty = |x: f64, z: f64| {
            let b = self.bb.offset(0, x);
            let b = b.offset(1, -1.0).offset(2, z);
            // `getCollidingBoundingBoxes` keeps only boxes that overlap `b`; `colliding_boxes` also lists touching ones.
            !Self::colliding_boxes(world, &b)
                .iter()
                .any(|o| (0..3).all(|a| b.max[a] > o.min[a] && b.min[a] < o.max[a]))
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

    /// Collision flags and stopped velocity after moving `d` of the wanted `motion`.
    fn set_collisions(&mut self, motion: [f64; 3], d: [f64; 3]) {
        self.collided_horizontally = motion[0] != d[0] || motion[2] != d[2];
        self.collided_vertically = motion[1] != d[1];
        self.on_ground = self.collided_vertically && motion[1] < 0.0;
        // Block.onLanded on y; no bouncy blocks yet
        for a in 0..3 {
            if motion[a] != d[a] {
                self.vel[a] = 0.0;
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
mod tests {
    use super::*;
    use oracle::{Oracle, TickState};
    use std::collections::HashMap;
    use std::fmt::Write;

    struct TestWorld {
        floor: Block,
        blocks: HashMap<(i32, i32, i32), Block>,
    }

    impl World for TestWorld {
        fn block(&self, x: i32, y: i32, z: i32) -> Block {
            match self.blocks.get(&(x, y, z)) {
                Some(b) => *b,
                None if y <= 63 => self.floor,
                None => Block::Air,
            }
        }
    }

    fn name(b: Block) -> &'static str {
        match b {
            Block::Air => "air",
            Block::Stone => "stone",
            Block::Ice => "ice",
        }
    }

    /// `(ticks, input)` runs after a spawn at `start` facing `yaw`.
    struct Scenario {
        floor: Block,
        blocks: Vec<((i32, i32, i32), Block)>,
        start: [f64; 3],
        yaw: f32,
        steps: Vec<(u32, Input)>,
    }

    impl Scenario {
        fn new(steps: Vec<(u32, Input)>) -> Scenario {
            Scenario {
                floor: Block::Stone,
                blocks: vec![],
                start: [0.5, 64.0, 0.5],
                yaw: 0.0,
                steps,
            }
        }

        fn script(&self) -> String {
            let mut s = format!("floor {}\n", name(self.floor));
            for ((x, y, z), b) in &self.blocks {
                writeln!(s, "block {x} {y} {z} {}", name(*b)).unwrap();
            }
            let [x, y, z] = self.start;
            writeln!(s, "start {x:?} {y:?} {z:?} {:?}", self.yaw).unwrap();
            for (n, i) in &self.steps {
                writeln!(
                    s,
                    "tick {n} {:?} {:?} {} {} {:?} {} {}",
                    i.forward,
                    i.strafe,
                    i.jump as u8,
                    i.sprint as u8,
                    i.fly_speed.unwrap_or(0.0),
                    i.sneak as u8,
                    i.using_item as u8
                )
                .unwrap();
            }
            s
        }

        fn simulate(&self) -> Vec<TickState> {
            let world = TestWorld {
                floor: self.floor,
                blocks: self.blocks.iter().copied().collect(),
            };
            let mut p = Player::new(self.start, self.yaw);
            let mut out = Vec::new();
            for (n, input) in &self.steps {
                for _ in 0..*n {
                    p.tick(&world, *input);
                    out.push(TickState {
                        pos: p.pos.map(f64::to_bits),
                        vel: p.vel.map(f64::to_bits),
                        on_ground: p.on_ground,
                    });
                }
            }
            out
        }

        /// Run the real jar and the port; fail on the first tick that differs in any bit.
        fn check(&self, label: &str) {
            let lines = Oracle::get().run(&self.script()).unwrap();
            let truth: Vec<TickState> =
                lines.iter().map(|l| TickState::parse(l).unwrap()).collect();
            let ours = self.simulate();
            assert_eq!(truth.len(), ours.len(), "{label}: tick count");
            let show = |s: &TickState| {
                format!(
                    "pos {:?} vel {:?} ground {}",
                    s.pos.map(f64::from_bits),
                    s.vel.map(f64::from_bits),
                    s.on_ground
                )
            };
            for (i, (t, o)) in truth.iter().zip(&ours).enumerate() {
                assert_eq!(
                    t,
                    o,
                    "{label}: tick {i}\n  jar : {}\n  port: {}",
                    show(t),
                    show(o)
                );
            }
        }
    }

    fn input(forward: f32, strafe: f32, jump: bool, sprint: bool) -> Input {
        Input {
            forward,
            strafe,
            jump,
            sprint,
            sneak: false,
            using_item: false,
            fly_speed: None,
        }
    }

    const IDLE: Input = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
        sneak: false,
        using_item: false,
        fly_speed: None,
    };

    #[test]
    fn walk() {
        Scenario::new(vec![
            (5, IDLE),
            (40, input(1.0, 0.0, false, false)),
            (20, IDLE),
        ])
        .check("walk");
    }

    #[test]
    fn sprint_then_stop() {
        Scenario::new(vec![
            (3, IDLE),
            (40, input(1.0, 0.0, false, true)),
            (30, IDLE),
        ])
        .check("sprint");
    }

    #[test]
    fn standing_jump_held() {
        Scenario::new(vec![
            (3, IDLE),
            (60, input(0.0, 0.0, true, false)),
            (20, IDLE),
        ])
        .check("jump");
    }

    #[test]
    fn sprint_jump_with_yaw() {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (60, input(1.0, 0.0, true, true)),
            (30, IDLE),
        ]);
        s.yaw = 33.0;
        s.check("sprint jump");
    }

    #[test]
    fn flying_climbs_hovers_sprints_and_descends() {
        let fly = |forward, jump, sprint, sneak| Input {
            forward,
            strafe: 0.0,
            jump,
            sprint,
            sneak,
            using_item: false,
            fly_speed: Some(0.05),
        };
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (15, fly(0.0, true, false, false)),
            (10, fly(0.0, false, false, false)),
            (20, fly(1.0, false, false, false)),
            (20, fly(1.0, false, true, false)),
            (15, fly(0.0, false, false, true)),
            (10, fly(0.0, false, false, false)),
        ]);
        s.yaw = 33.0;
        s.check("flying");
    }

    #[test]
    fn walk_strafe_in_air_and_on_ground() {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (10, input(1.0, 1.0, false, false)),
            (1, input(1.0, -1.0, true, false)),
            (20, input(0.5, -1.0, false, false)),
            (25, input(-1.0, 0.3, false, false)),
        ]);
        s.yaw = -141.5;
        s.check("strafe");
    }

    #[test]
    fn ice_keeps_momentum() {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (30, input(1.0, 0.0, false, true)),
            (60, IDLE),
        ]);
        s.floor = Block::Ice;
        s.check("ice");
    }

    #[test]
    fn wall_blocks_and_slides() {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (40, input(1.0, 0.4, false, true)),
            (10, input(1.0, 0.0, true, true)),
        ]);
        s.blocks = (0..=2)
            .flat_map(|x| (64..=65).map(move |y| ((x, y, 4), Block::Stone)))
            .collect();
        s.yaw = 12.0;
        s.check("wall");
    }

    #[test]
    fn ceiling_stops_jump() {
        let mut s = Scenario::new(vec![(3, IDLE), (30, input(0.0, 0.0, true, false))]);
        s.blocks = vec![((0, 66, 0), Block::Stone)];
        s.check("ceiling");
    }

    #[test]
    fn sneak_walk() {
        let sneak = Input {
            forward: 1.0,
            sneak: true,
            ..IDLE
        };
        Scenario::new(vec![(3, IDLE), (40, sneak), (20, IDLE)]).check("sneak walk");
    }

    #[test]
    fn sword_block_walk() {
        let block = Input {
            forward: 1.0,
            strafe: 1.0,
            using_item: true,
            ..IDLE
        };
        Scenario::new(vec![(3, IDLE), (40, block), (20, IDLE)]).check("block walk");
    }

    #[test]
    fn sneak_stops_at_a_block_edge() {
        let sneak = Input {
            forward: 1.0,
            sneak: true,
            ..IDLE
        };
        let mut s = Scenario::new(vec![(3, IDLE), (120, sneak), (20, IDLE)]);
        s.blocks = (-3..=3)
            .flat_map(|x| {
                (-3..=3).map(move |z| ((x, 63, z), if z >= 3 { Block::Air } else { Block::Stone }))
            })
            .collect();
        s.check("sneak edge");
        let last = s.simulate().pop().unwrap();
        assert!(last.on_ground, "sneaking player fell off the ledge");
    }

    #[test]
    fn walks_off_an_edge() {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (30, input(1.0, 0.0, false, true)),
            (20, IDLE),
        ]);
        s.blocks = (-3..=3)
            .flat_map(|x| {
                (-3..=3).map(move |z| ((x, 63, z), if z >= 3 { Block::Air } else { Block::Stone }))
            })
            .collect();
        s.floor = Block::Stone;
        s.check("edge");
    }

    fn peak_height(boost: f32) -> f64 {
        let world = TestWorld {
            floor: Block::Stone,
            blocks: HashMap::new(),
        };
        let mut p = Player::new([0.5, 64.0, 0.5], 0.0);
        p.jump_boost = boost;
        let jump = input(0.0, 0.0, true, false);
        let mut top = 64.0_f64;
        for _ in 0..30 {
            p.tick(&world, jump);
            top = top.max(p.pos[1]);
        }
        top
    }

    #[test]
    fn jump_boost_raises_the_jump_by_a_tenth_of_a_block_speed_per_level() {
        let (plain, one, two) = (peak_height(0.0), peak_height(1.0), peak_height(2.0));
        assert!(plain < one && one < two);
        assert!(
            (plain - 65.25).abs() < 0.01,
            "plain jump is 1.25 blocks: {plain}"
        );
    }

    #[test]
    fn the_movement_attribute_drives_ground_speed() {
        let world = TestWorld {
            floor: Block::Stone,
            blocks: HashMap::new(),
        };
        let walked = |attr: Option<f64>| {
            let mut p = Player::new([0.5, 64.0, 0.5], 0.0);
            p.speed_attr = attr;
            for _ in 0..10 {
                p.tick(&world, input(1.0, 0.0, false, false));
            }
            p.pos[2].abs()
        };
        let base = walked(None);
        assert_eq!(walked(Some(0.100_000_001_490_116_12)), base);
        assert!(walked(Some(0.12)) > base);
        assert!(walked(Some(0.05)) < base);
    }
}
