use super::*;
use oracle::{Oracle, TickState};
use std::collections::HashMap;
use std::fmt::Write;

const STONE: u16 = 1 << 4;
const SLIME: u16 = 165 << 4;
const ICE: u16 = 79 << 4;
const WATER: u16 = 9 << 4;

struct TestWorld {
    floor: u16,
    blocks: HashMap<(i32, i32, i32), u16>,
}

impl World for TestWorld {
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        match self.blocks.get(&(x, y, z)) {
            Some(b) => *b,
            None if y <= 63 => self.floor,
            None => 0,
        }
    }
}

fn name(b: u16) -> &'static str {
    match b {
        0 => "air",
        STONE => "stone",
        ICE => "ice",
        _ => panic!("floor must be air, stone or ice"),
    }
}

/// `(ticks, input)` runs after a spawn at `start` facing `yaw`.
struct Scenario {
    floor: u16,
    blocks: Vec<((i32, i32, i32), u16)>,
    start: [f64; 3],
    yaw: f32,
    steps: Vec<(u32, Input)>,
    /// Depth Strider level of the worn boots.
    boots: i32,
    /// Spectator body: `Entity.noClip`, never on the ground.
    no_clip: bool,
}

impl Scenario {
    fn new(steps: Vec<(u32, Input)>) -> Scenario {
        Scenario {
            floor: STONE,
            blocks: vec![],
            start: [0.5, 64.0, 0.5],
            yaw: 0.0,
            steps,
            boots: 0,
            no_clip: false,
        }
    }

    fn script(&self) -> String {
        let mut s = format!("floor {}\n", name(self.floor));
        for ((x, y, z), b) in &self.blocks {
            writeln!(s, "state {x} {y} {z} {} {}", b >> 4, b & 15).unwrap();
        }
        let [x, y, z] = self.start;
        writeln!(s, "start {x:?} {y:?} {z:?} {:?}", self.yaw).unwrap();
        writeln!(s, "boots {}", self.boots).unwrap();
        // The real `pushAwayFrom` runs at every tick, as `LocalClientPlayerEntity` does.
        s.push_str("pushout\n");
        if self.no_clip {
            s.push_str("noclip\n");
        }
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
        p.depth_strider = self.boots;
        p.no_clip = self.no_clip;
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
        let truth: Vec<TickState> = lines
            .iter()
            .filter(|l| *l != "ok")
            .map(|l| TickState::parse(l).unwrap())
            .collect();
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
    s.floor = ICE;
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
        .flat_map(|x| (64..=65).map(move |y| ((x, y, 4), STONE)))
        .collect();
    s.yaw = 12.0;
    s.check("wall");
}

#[test]
fn ceiling_stops_jump() {
    let mut s = Scenario::new(vec![(3, IDLE), (30, input(0.0, 0.0, true, false))]);
    s.blocks = vec![((0, 66, 0), STONE)];
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
fn sprint_then_block_walk() {
    let sprint = input(1.0, 0.0, false, true);
    let block = Input {
        forward: 1.0,
        using_item: true,
        ..IDLE
    };
    Scenario::new(vec![(3, IDLE), (12, sprint), (20, block)]).check("sprint then block");
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
        .flat_map(|x| (-3..=3).map(move |z| ((x, 63, z), if z >= 3 { 0 } else { STONE })))
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
        .flat_map(|x| (-3..=3).map(move |z| ((x, 63, z), if z >= 3 { 0 } else { STONE })))
        .collect();
    s.floor = STONE;
    s.check("edge");
}

fn peak_height(boost: f32) -> f64 {
    let world = TestWorld {
        floor: STONE,
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
        floor: STONE,
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

/// A pool of still water (level 0) two blocks deep, with a flowing block beside it.
fn pool() -> Vec<((i32, i32, i32), u16)> {
    let mut b = vec![];
    for x in -3..=3 {
        for z in -3..=3 {
            b.push(((x, 63, z), WATER));
            b.push(((x, 64, z), WATER));
        }
    }
    b.push(((1, 64, 0), (8 << 4) | 3));
    b.push(((2, 64, 0), (8 << 4) | 5));
    b
}

#[test]
fn swim_sink_and_jump_in_water() {
    let mut s = Scenario::new(vec![
        (10, IDLE),
        (20, input(1.0, 0.0, false, false)),
        (20, input(0.0, 0.0, true, false)),
        (10, IDLE),
    ]);
    s.start = [0.5, 66.0, 0.5];
    s.blocks = pool();
    s.floor = STONE;
    s.check("water");
}

#[test]
fn lava_and_ladder_and_web_and_soul_sand() {
    let mut s = Scenario::new(vec![
        (10, IDLE),
        (30, input(1.0, 0.0, false, false)),
        (10, IDLE),
    ]);
    s.blocks = (-2..=2).map(|x| ((x, 64, 0), 11 << 4)).collect();
    s.check("lava");
    let mut s = Scenario::new(vec![(30, input(0.0, 0.0, true, false)), (20, IDLE)]);
    s.blocks = vec![
        ((0, 64, 1), 65 << 4 | 2),
        ((0, 65, 1), 65 << 4 | 2),
        ((0, 66, 1), 65 << 4 | 2),
    ];
    s.start = [0.5, 64.0, 0.5];
    s.check("ladder");
    let mut s = Scenario::new(vec![(30, input(0.0, 0.0, true, false)), (20, IDLE)]);
    s.blocks = (64..=66).map(|y| ((0, y, 1), 106 << 4 | 4)).collect();
    s.start = [0.5, 64.0, 0.5];
    s.check("vine");
    let mut s = Scenario::new(vec![
        (10, IDLE),
        (20, input(1.0, 0.0, false, false)),
        (5, IDLE),
    ]);
    s.blocks = vec![((0, 64, 1), 30 << 4), ((0, 65, 1), 30 << 4)];
    s.start = [0.5, 66.0, 0.5];
    s.check("web");
    let mut s = Scenario::new(vec![(10, input(1.0, 0.0, false, false)), (5, IDLE)]);
    s.blocks = (-1..=4).map(|z| ((0, 63, z), 88 << 4)).collect();
    s.check("soul sand");
}

#[test]
fn slabs_stairs_and_fences_collide_with_their_shape() {
    let mut s = Scenario::new(vec![(30, input(1.0, 0.0, false, false)), (20, IDLE)]);
    s.floor = STONE;
    s.blocks = vec![
        ((0, 64, 2), 44 << 4),
        ((0, 64, 4), (53 << 4) | 2),
        ((0, 64, 6), 85 << 4),
    ];
    s.check("shapes");
}

/// Depth Strider 0 to 4 (4 is capped to 3): a sprint along the pool floor (halved level), a jump
/// off it and a swim in open water, each tick bit-exact against the jar.
#[test]
fn depth_strider_in_water() {
    for level in 0..=4 {
        let mut s = Scenario::new(vec![
            (3, IDLE),
            (25, input(1.0, 0.3, false, true)),
            (6, input(1.0, 0.0, true, true)),
            (20, input(1.0, -0.5, false, false)),
            (10, IDLE),
        ]);
        s.blocks = (-12..=12)
            .flat_map(|x| (-3..=40).flat_map(move |z| (64..=66).map(move |y| ((x, y, z), WATER))))
            .collect();
        s.yaw = 20.0;
        s.boots = level;
        s.check(&format!("depth strider {level}"));
    }
}

#[test]
fn teleport_keeps_state_and_zeroes_absolute_velocity() {
    let mut p = Player::new([0.5, 64.0, 0.5], 0.0);
    p.on_ground = true;
    p.in_web = true;
    p.jump_ticks = 7;
    p.jump_movement_factor = 0.026;
    p.collided_horizontally = true;
    p.vel = [0.1, 0.2, 0.3];
    p.teleport([5.0, 70.0, 6.0], 725.0, [true, false, false]);
    assert_eq!(p.vel, [0.1, 0.0, 0.0]);
    assert_eq!(p.yaw, 5.0);
    assert_eq!(p.pos, [5.0, 70.0, 6.0]);
    assert!(p.on_ground && p.in_web && p.collided_horizontally);
    assert_eq!((p.jump_ticks, p.jump_movement_factor), (7, 0.026));
    p.teleport([5.0, 70.0, 6.0], 0.0, [false; 3]);
    assert_eq!(p.vel, [0.0; 3]);
}

/// A slime floor at y 63 (`SlimeBlock.onSteppedOn` slows a walker standing on it).
fn slime_floor(steps: Vec<(u32, Input)>) -> Scenario {
    let mut s = Scenario::new(steps);
    s.blocks = (-3..=3)
        .flat_map(|x| (-3..=40).map(move |z| ((x, 63, z), SLIME)))
        .collect();
    s
}

#[test]
fn walk_on_slime() {
    slime_floor(vec![
        (5, IDLE),
        (40, input(1.0, 0.0, false, false)),
        (20, IDLE),
    ])
    .check("slime walk");
}

#[test]
fn sprint_on_slime() {
    slime_floor(vec![
        (3, IDLE),
        (40, input(1.0, 0.0, false, true)),
        (30, IDLE),
    ])
    .check("slime sprint");
}

#[test]
fn sneak_walk_on_slime_is_not_slowed() {
    let sneak = Input {
        forward: 1.0,
        sneak: true,
        ..IDLE
    };
    slime_floor(vec![(3, IDLE), (40, sneak), (20, IDLE)]).check("slime sneak");
}

#[test]
fn sprint_jumping_on_slime() {
    slime_floor(vec![(80, input(1.0, 0.0, true, true))]).check("slime jump");
}

/// A player at (0.5, 64.5, 0.5) in a two-high stone column; only the cell at x = -1 is open.
fn walled_in() -> Scenario {
    let mut s = Scenario::new(vec![(8, IDLE)]);
    s.start = [0.5, 64.5, 0.5];
    for (x, z) in [(0, 0), (1, 0), (0, -1), (0, 1)] {
        for y in [64, 65] {
            s.blocks.push(((x, y, z), STONE));
        }
    }
    s
}

#[test]
fn player_in_stone_is_pushed_to_the_open_side() {
    let s = walled_in();
    let world = TestWorld {
        floor: s.floor,
        blocks: s.blocks.iter().copied().collect(),
    };
    let mut p = Player::new(s.start, 0.0);
    p.push_out_of_blocks_at_corners(&world);
    assert_eq!(p.vel, [f64::from(-0.1_f32), 0.0, 0.0]);
    s.check("pushed out of stone");
}

#[test]
fn player_in_stone_is_pushed_north_when_west_is_closed() {
    // West and east are stone; north and south are open, and north is the nearer (0.29) from most corners.
    let mut s = walled_in();
    s.blocks
        .retain(|((x, _, z), _)| (*x, *z) != (0, -1) && (*x, *z) != (0, 1));
    s.blocks
        .extend([((-1, 64, 0), STONE), ((-1, 65, 0), STONE)]);
    s.check("pushed out of stone to the north");
}

/// A spectator in flight through a wall (z = 3, high) and down through the stone floor.
#[test]
fn spectator_flies_through_walls_and_floor_like_the_real_player() {
    let fly = |forward, sneak| Input {
        forward,
        sneak,
        fly_speed: Some(0.05),
        ..IDLE
    };
    let mut s = Scenario::new(vec![
        (3, fly(0.0, false)),
        (25, fly(1.0, false)),
        (40, fly(0.0, true)),
        (5, fly(0.0, false)),
    ]);
    s.start = [0.5, 70.0, 0.5];
    s.no_clip = true;
    for y in 64..80 {
        for x in -1..=1 {
            s.blocks.push(((x, y, 3), STONE));
        }
    }
    s.check("spectator");
}

#[test]
fn no_clip_moves_the_box_by_the_full_delta() {
    let wall = TestWorld {
        floor: STONE,
        blocks: (-5..5)
            .flat_map(|y| (0..3).map(move |z| ((1, 60 + y, z), STONE)))
            .collect(),
    };
    let start = [0.5, 64.0, 0.5];
    let mut p = Player::new(start, 0.0);
    p.no_clip = true;
    let motion = [0.25, -0.5, 0.125];
    for _ in 0..40 {
        p.on_ground = false;
        p.move_entity(&wall, motion);
    }
    assert_eq!(p.pos, [10.5, 44.0, 5.5]);
    assert!(!p.on_ground && !p.collided_horizontally && !p.collided_vertically);
}

/// An empty world with boat boxes in it.
struct Boats(Vec<[f64; 6]>);

impl World for Boats {
    fn block(&self, _: i32, _: i32, _: i32) -> u16 {
        0
    }

    fn entity_boxes(&self) -> &[[f64; 6]] {
        &self.0
    }
}

#[test]
fn player_above_a_boat_stands_on_it() {
    let boat = [-0.75, 62.0, -0.75, 0.75, 62.6, 0.75];
    let world = Boats(vec![boat]);
    let mut p = Player::new([0.0, 66.0, 0.0], 0.0);
    for _ in 0..40 {
        p.tick(&world, IDLE);
    }
    assert!(p.on_ground);
    assert_eq!(p.pos[1], 62.6);
}
