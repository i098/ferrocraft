//! Block ambient effects, ported from 1.8.9 `ClientWorld.doRandomDisplayTicks` and the
//! `randomDisplayTick` of each block. Constants come from `javap -c` of the remapped 1.8.9 jar.

use crate::jrandom::JavaRandom;
use crate::particles::Kind;
use world::dig::opaque_cube;
use world::place::solid_top;
use world::shape::blocks_movement;

/// `doRandomDisplayTicks` samples cells within 16 blocks of the player's feet.
const RADIUS: i32 = 16;
const SAMPLES: usize = 1000;
/// `WorldRenderer.tryAddParticle` drops a particle more than 16 blocks from the camera entity.
const RANGE_SQ: f64 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    Particle {
        kind: Kind,
        pos: [f64; 3],
        /// Motion; a `RedDust` particle reads its colour from it.
        motion: [f64; 3],
    },
    Sound {
        name: &'static str,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    },
}

struct Cx<'a> {
    block: &'a dyn Fn(i32, i32, i32) -> u16,
    raining: &'a dyn Fn(i32, i32, i32) -> bool,
    out: Vec<Effect>,
}

impl Cx<'_> {
    fn at(&self, [x, y, z]: [i32; 3]) -> u16 {
        (self.block)(x, y, z)
    }

    fn particle(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) {
        self.out.push(Effect::Particle { kind, pos, motion });
    }

    fn sound(&mut self, name: &'static str, pos: [f64; 3], volume: f32, pitch: f32) {
        self.out.push(Effect::Sound {
            name,
            pos,
            volume,
            pitch,
        });
    }
}

/// One client tick of `doRandomDisplayTicks` around `feet`. `view` is the camera entity position and
/// `option` is `gameSettings.particles` (0 all, 1 decreased, 2 minimal): they cull particles as
/// `tryAddParticle` does. `block(x, y, z)` is a block state id, `raining(x, y, z)` is `World.isRaining(pos)`.
/// shortcut: the sampling and the blocks share one generator; vanilla uses `World.random` and a fresh `Random`.
pub fn random_display_ticks(
    feet: [i32; 3],
    view: [f64; 3],
    option: i32,
    rng: &mut JavaRandom,
    block: &dyn Fn(i32, i32, i32) -> u16,
    raining: &dyn Fn(i32, i32, i32) -> bool,
) -> Vec<Effect> {
    let mut cx = Cx {
        block,
        raining,
        out: Vec::new(),
    };
    for _ in 0..SAMPLES {
        let mut cell = feet;
        for c in &mut cell {
            *c += rng.next_int(RADIUS) - rng.next_int(RADIUS);
        }
        let state = cx.at(cell);
        display_tick(&mut cx, state, cell, rng);
    }
    cx.out.retain(|e| match *e {
        Effect::Sound { .. } => true,
        Effect::Particle { pos, .. } => {
            let mut option = option;
            if option == 1 && rng.next_int(3) == 0 {
                option = 2;
            }
            let d: f64 = (0..3).map(|i| (view[i] - pos[i]).powi(2)).sum();
            d <= RANGE_SQ && option <= 1
        }
    });
    cx.out
}

/// `Block.randomDisplayTick` of the block `state` at `pos`.
fn display_tick(cx: &mut Cx, state: u16, pos: [i32; 3], rng: &mut JavaRandom) {
    let (id, meta) = (state >> 4, state & 15);
    match id {
        50 => torch(cx, pos, meta),
        76 => redstone_torch(cx, pos, meta, rng),
        51 => fire(cx, pos, rng),
        8..=11 => liquid(cx, id, meta, pos, rng),
        55 => wire(cx, pos, meta, rng),
        74 => lit_ore(cx, pos, rng),
        _ => decor_tick(cx, id, meta, pos, rng),
    }
}

/// The blocks that decorate a build rather than burn or spark.
fn decor_tick(cx: &mut Cx, id: u16, meta: u16, pos: [i32; 3], rng: &mut JavaRandom) {
    match id {
        62 => furnace(cx, pos, meta, rng),
        116 => enchanting_table(cx, pos, rng),
        90 => portal(cx, pos, rng),
        119 => end_portal(cx, pos, rng),
        110 => mycelium(cx, pos, rng),
        18 | 161 => leaves(cx, pos, rng),
        _ => props_tick(cx, id, meta, pos, rng),
    }
}

fn props_tick(cx: &mut Cx, id: u16, meta: u16, pos: [i32; 3], rng: &mut JavaRandom) {
    match id {
        117 => brewing_stand(cx, pos, rng),
        130 => ender_chest(cx, pos, rng),
        94 => repeater(cx, pos, meta, rng),
        19 if meta == 1 => wet_sponge(cx, pos, rng),
        _ => {}
    }
}

fn f(v: i32, r: f32) -> f64 {
    f64::from(v as f32 + r)
}

fn d(v: i32) -> f64 {
    f64::from(v)
}

/// Horizontal offset of the side a wall torch leans away from (`FACING.getOpposite()`), or `None` for a floor torch.
fn torch_lean(meta: u16) -> Option<[f64; 2]> {
    match meta {
        1 => Some([-1.0, 0.0]),
        2 => Some([1.0, 0.0]),
        3 => Some([0.0, -1.0]),
        4 => Some([0.0, 1.0]),
        _ => None,
    }
}

/// Flame spot of a `TorchBlock`, offset by `jitter` and leaning away from its wall.
fn torch_spot([x, y, z]: [i32; 3], meta: u16, jitter: [f64; 3]) -> [f64; 3] {
    let at = [
        d(x) + 0.5 + jitter[0],
        d(y) + 0.7 + jitter[1],
        d(z) + 0.5 + jitter[2],
    ];
    match torch_lean(meta) {
        Some([ox, oz]) => [at[0] + 0.27 * ox, at[1] + 0.22, at[2] + 0.27 * oz],
        None => at,
    }
}

/// `TorchBlock`.
fn torch(cx: &mut Cx, pos: [i32; 3], meta: u16) {
    let at = torch_spot(pos, meta, [0.0; 3]);
    cx.particle(Kind::Smoke, at, [0.0; 3]);
    cx.particle(Kind::Flame, at, [0.0; 3]);
}

/// `RedstoneTorchBlock` dust (lit only).
fn redstone_torch(cx: &mut Cx, pos: [i32; 3], meta: u16, rng: &mut JavaRandom) {
    let jitter = [0; 3].map(|_| (rng.next_double() - 0.5) * 0.2);
    cx.particle(Kind::RedDust, torch_spot(pos, meta, jitter), [0.0; 3]);
}

/// `FireBlock.isFlammable`: the blocks `FireBlock.init` registers with a burn chance.
fn flammable(id: u16) -> bool {
    matches!(
        id,
        5 | 125 | 126 | 107 | 183..=187 | 85 | 188..=192 | 53 | 134..=136 | 17 | 162 | 18 | 161 | 47
            | 46 | 31 | 175 | 37 | 38 | 32 | 35 | 106 | 173 | 170 | 171
    )
}

fn fire(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    if rng.next_int(24) == 0 {
        let pos = [f(x, 0.5), f(y, 0.5), f(z, 0.5)];
        let volume = 1.0 + rng.next_float();
        cx.sound("fire.fire", pos, volume, rng.next_float() * 0.7 + 0.3);
    }
    let below = cx.at([x, y - 1, z]);
    if solid_top(below) || flammable(below >> 4) {
        for _ in 0..3 {
            let at = [
                d(x) + rng.next_double(),
                d(y) + rng.next_double() * 0.5 + 0.5,
                d(z) + rng.next_double(),
            ];
            cx.particle(Kind::LargeSmoke, at, [0.0; 3]);
        }
        return;
    }
    // West, east, north, south, up: two puffs along a side that has a flammable neighbour.
    const SIDES: [([i32; 3], usize, bool); 5] = [
        ([-1, 0, 0], 0, false),
        ([1, 0, 0], 0, true),
        ([0, 0, -1], 2, false),
        ([0, 0, 1], 2, true),
        ([0, 1, 0], 1, true),
    ];
    for ([dx, dy, dz], axis, far) in SIDES {
        if !flammable(cx.at([x + dx, y + dy, z + dz]) >> 4) {
            continue;
        }
        for _ in 0..2 {
            let mut at = [d(x), d(y), d(z)];
            for (a, v) in at.iter_mut().enumerate() {
                let r = rng.next_double();
                *v += match (a == axis, far) {
                    (false, _) => r,
                    (true, false) => r * 0.100_000_001_490_116_12,
                    (true, true) => 1.0 - r * 0.100_000_001_490_116_12,
                };
            }
            cx.particle(Kind::LargeSmoke, at, [0.0; 3]);
        }
    }
}

fn liquid(cx: &mut Cx, id: u16, level: u16, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    let water = id < 10;
    let centre = [d(x) + 0.5, d(y) + 0.5, d(z) + 0.5];
    if water {
        if level > 0 && level < 8 {
            if rng.next_int(64) == 0 {
                let volume = rng.next_float() * 0.25 + 0.75;
                cx.sound("liquid.water", centre, volume, rng.next_float() + 0.5);
            }
        } else if rng.next_int(10) == 0 {
            let at = [d(x), d(y), d(z)].map(|v| v + f64::from(rng.next_float()));
            cx.particle(Kind::Suspended, at, [0.0; 3]);
        }
    } else if cx.at([x, y + 1, z]) >> 4 == 0 {
        lava_pops(cx, [x, y, z], rng);
    }
    if rng.next_int(10) == 0 && solid_top(cx.at([x, y - 1, z])) {
        let under = cx.at([x, y - 2, z]) >> 4;
        if !blocks_movement(under) && !(8..=11).contains(&under) {
            let at = [
                d(x) + f64::from(rng.next_float()),
                d(y) - 1.05,
                d(z) + f64::from(rng.next_float()),
            ];
            cx.particle(
                if water {
                    Kind::DripWater
                } else {
                    Kind::DripLava
                },
                at,
                [0.0; 3],
            );
        }
    }
}

/// The `liquid.lavapop` pop with its lava spark and the `liquid.lava` crackle of a lava cell under air.
fn lava_pops(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    if rng.next_int(100) == 0 {
        let at = [
            d(x) + f64::from(rng.next_float()),
            d(y) + 1.0,
            d(z) + f64::from(rng.next_float()),
        ];
        cx.particle(Kind::Lava, at, [0.0; 3]);
        let volume = 0.2 + rng.next_float() * 0.2;
        cx.sound("liquid.lavapop", at, volume, 0.9 + rng.next_float() * 0.15);
    }
    if rng.next_int(200) == 0 {
        let volume = 0.2 + rng.next_float() * 0.2;
        let pitch = 0.9 + rng.next_float() * 0.15;
        cx.sound("liquid.lava", [d(x), d(y), d(z)], volume, pitch);
    }
}

/// `RedstoneWireBlock`: dust whose colour follows the power.
fn wire(cx: &mut Cx, [x, y, z]: [i32; 3], power: u16, rng: &mut JavaRandom) {
    if power == 0 {
        return;
    }
    let at = [
        d(x) + 0.5 + (f64::from(rng.next_float()) - 0.5) * 0.2,
        f(y, 0.0625),
        d(z) + 0.5 + (f64::from(rng.next_float()) - 0.5) * 0.2,
    ];
    let p = f32::from(power) / 15.0;
    let colour = [
        p * 0.6 + 0.4,
        (p * p * 0.7 - 0.5).max(0.0),
        (p * p * 0.6 - 0.7).max(0.0),
    ];
    cx.particle(Kind::RedDust, at, colour.map(f64::from));
}

/// `RedstoneOreBlock.addParticles`: dust on the faces that touch a non-opaque block.
fn lit_ore(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    const FACES: [[i32; 3]; 6] = [
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, 1],
        [0, 0, -1],
        [1, 0, 0],
        [-1, 0, 0],
    ];
    let base = [x, y, z];
    for face in FACES {
        let mut at = [0, 1, 2].map(|a| f(base[a], rng.next_float()));
        let neighbour = [x + face[0], y + face[1], z + face[2]];
        if !opaque_cube(cx.at(neighbour) >> 4) {
            let axis = if face[1] != 0 {
                1
            } else if face[2] != 0 {
                2
            } else {
                0
            };
            at[axis] = d(base[axis]) + if face[axis] > 0 { 1.0625 } else { -0.0625 };
        }
        if (0..3).all(|a| at[a] >= d(base[a]) && at[a] <= d(base[a]) + 1.0) {
            continue;
        }
        cx.particle(Kind::RedDust, at, [0.0; 3]);
    }
}

/// `FurnaceBlock` (lit): smoke and flame at the face the furnace looks out of.
fn furnace(cx: &mut Cx, [x, y, z]: [i32; 3], meta: u16, rng: &mut JavaRandom) {
    let (cx0, cz0) = (d(x) + 0.5, d(z) + 0.5);
    let cy = d(y) + rng.next_double() * 6.0 / 16.0;
    let (out, along) = (0.52, rng.next_double() * 0.6 - 0.3);
    let at = match meta {
        4 => [cx0 - out, cy, cz0 + along],
        5 => [cx0 + out, cy, cz0 + along],
        3 => [cx0 + along, cy, cz0 + out],
        _ => [cx0 + along, cy, cz0 - out],
    };
    cx.particle(Kind::Smoke, at, [0.0; 3]);
    cx.particle(Kind::Flame, at, [0.0; 3]);
}

/// `EnchantingTableBlock`: glyphs fly from each bookshelf two blocks away that has a clear line.
fn enchanting_table(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    for i in -2..=2 {
        let mut j = -2;
        while j <= 2 {
            if i > -2 && i < 2 && j == -1 {
                j = 2;
            }
            if rng.next_int(16) == 0 {
                glyphs(cx, [x, y, z], i, j, rng);
            }
            j += 1;
        }
    }
}

fn glyphs(cx: &mut Cx, [x, y, z]: [i32; 3], i: i32, j: i32, rng: &mut JavaRandom) {
    for k in 0..=1 {
        if cx.at([x + i, y + k, z + j]) >> 4 != 47 {
            continue;
        }
        if cx.at([x + i / 2, y, z + j / 2]) >> 4 != 0 {
            break;
        }
        let at = [d(x) + 0.5, d(y) + 2.0, d(z) + 0.5];
        let motion = [
            f(i, rng.next_float()) - 0.5,
            f64::from((k as f32 - rng.next_float()) - 1.0),
            f(j, rng.next_float()) - 0.5,
        ];
        cx.particle(Kind::Enchant, at, motion);
    }
}

/// `PortalBlock`: a rare hum and four specks that drift across the frame.
fn portal(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    if rng.next_int(100) == 0 {
        let pos = [f(x, 0.5), f(y, 0.5), f(z, 0.5)];
        cx.sound("portal.portal", pos, 0.5, rng.next_float() * 0.4 + 0.8);
    }
    let across_z = cx.at([x - 1, y, z]) >> 4 == 90 || cx.at([x + 1, y, z]) >> 4 == 90;
    for _ in 0..4 {
        let mut at = [
            f(x, rng.next_float()),
            f(y, rng.next_float()),
            f(z, rng.next_float()),
        ];
        let mut motion = [0.0; 3];
        for m in &mut motion {
            *m = (f64::from(rng.next_float()) - 0.5) * 0.5;
        }
        let sign = rng.next_int(2) * 2 - 1;
        let (pos_axis, motion_axis, base) = if across_z { (2, 2, z) } else { (0, 0, x) };
        at[pos_axis] = d(base) + 0.5 + 0.25 * d(sign);
        motion[motion_axis] = f64::from(rng.next_float() * 2.0 * sign as f32);
        cx.particle(Kind::Portal, at, motion);
    }
}

fn end_portal(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    let at = [f(x, rng.next_float()), f(y, 0.8), f(z, rng.next_float())];
    cx.particle(Kind::Smoke, at, [0.0; 3]);
}

fn mycelium(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    if rng.next_int(10) == 0 {
        let at = [f(x, rng.next_float()), f(y, 1.1), f(z, rng.next_float())];
        cx.particle(Kind::DepthSuspend, at, [0.0; 3]);
    }
}

/// `AbstractLeavesBlock`: rain drips off leaves that have no solid block under them.
fn leaves(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    if (cx.raining)(x, y + 1, z) && !solid_top(cx.at([x, y - 1, z])) && rng.next_int(15) == 1 {
        let at = [f(x, rng.next_float()), d(y) - 0.05, f(z, rng.next_float())];
        cx.particle(Kind::DripWater, at, [0.0; 3]);
    }
}

fn brewing_stand(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    let at = [
        f(x, 0.4 + rng.next_float() * 0.2),
        f(y, 0.7 + rng.next_float() * 0.3),
        f(z, 0.4 + rng.next_float() * 0.2),
    ];
    cx.particle(Kind::Smoke, at, [0.0; 3]);
}

fn ender_chest(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    for _ in 0..3 {
        let j = rng.next_int(2) * 2 - 1;
        let k = rng.next_int(2) * 2 - 1;
        let at = [
            d(x) + 0.5 + 0.25 * d(j),
            f(y, rng.next_float()),
            d(z) + 0.5 + 0.25 * d(k),
        ];
        let motion = [
            f64::from(rng.next_float() * j as f32),
            (f64::from(rng.next_float()) - 0.5) * 0.125,
            f64::from(rng.next_float() * k as f32),
        ];
        cx.particle(Kind::Portal, at, motion);
    }
}

/// `RepeaterBlock` (powered): dust that sits on the delay side or the other side of the repeater.
fn repeater(cx: &mut Cx, [x, y, z]: [i32; 3], meta: u16, rng: &mut JavaRandom) {
    let jitter = |rng: &mut JavaRandom| f64::from(rng.next_float() - 0.5) * 0.2;
    let at = [
        f(x, 0.5) + jitter(rng),
        f(y, 0.4) + jitter(rng),
        f(z, 0.5) + jitter(rng),
    ];
    let mut shift = -5.0f32;
    if rng.next_boolean() {
        shift = (1 + i32::from(meta >> 2)) as f32 * 2.0 - 1.0;
    }
    shift /= 16.0;
    // `Direction.getHorizontal(meta & 3)`: south, west, north, east.
    let (ox, oz) = [(0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)][usize::from(meta & 3)];
    let at = [
        at[0] + f64::from(shift * ox),
        at[1],
        at[2] + f64::from(shift * oz),
    ];
    cx.particle(Kind::RedDust, at, [0.0; 3]);
}

/// `SpongeBlock` (wet): a drip from a random side that has no solid top beside it.
fn wet_sponge(cx: &mut Cx, [x, y, z]: [i32; 3], rng: &mut JavaRandom) {
    // `Direction` order: down, up, north, south, west, east.
    let side = rng.next_int(6);
    let offset = [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ][side as usize];
    if side == 1 || solid_top(cx.at([x + offset[0], y + offset[1], z + offset[2]])) {
        return;
    }
    let mut at = [d(x), d(y), d(z)];
    if side == 0 {
        at = [
            at[0] + rng.next_double(),
            at[1] - 0.05,
            at[2] + rng.next_double(),
        ];
    } else {
        at[1] += rng.next_double() * 0.8;
        if side >= 4 {
            at[2] += rng.next_double();
            at[0] += if side == 5 { 1.1 } else { 0.05 };
        } else {
            at[0] += rng.next_double();
            at[2] += if side == 3 { 1.1 } else { 0.05 };
        }
    }
    cx.particle(Kind::DripWater, at, [0.0; 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        state: u16,
        tick: impl Fn(&mut Cx, [i32; 3], &mut JavaRandom),
        seed: i64,
        n: usize,
    ) -> Vec<Effect> {
        let block = move |_: i32, _: i32, _: i32| state;
        let raining = |_: i32, _: i32, _: i32| false;
        let mut cx = Cx {
            block: &block,
            raining: &raining,
            out: Vec::new(),
        };
        let mut rng = JavaRandom::new(seed);
        for _ in 0..n {
            tick(&mut cx, [0, 0, 0], &mut rng);
        }
        cx.out
    }

    #[test]
    fn floor_torch_spawns_smoke_and_flame_at_the_flame_spot() {
        let out = run(50 << 4 | 5, |cx, p, _| torch(cx, p, 5), 1, 1);
        let at = [0.5, 0.7, 0.5];
        assert_eq!(
            out,
            vec![
                Effect::Particle {
                    kind: Kind::Smoke,
                    pos: at,
                    motion: [0.0; 3]
                },
                Effect::Particle {
                    kind: Kind::Flame,
                    pos: at,
                    motion: [0.0; 3]
                },
            ]
        );
    }

    #[test]
    fn lit_redstone_torch_dust_jitters_each_axis_then_leans() {
        let mut oracle = JavaRandom::new(7);
        let j: Vec<f64> = (0..3).map(|_| (oracle.next_double() - 0.5) * 0.2).collect();
        let mut after = oracle;
        let seen = std::cell::Cell::new(None);
        let out = run(
            76 << 4 | 1,
            |cx, p, rng| {
                redstone_torch(cx, p, 1, rng);
                seen.set(Some(rng.next_double()));
            },
            7,
            1,
        );
        assert_eq!(seen.get(), Some(after.next_double()));
        assert_eq!(
            out,
            vec![Effect::Particle {
                kind: Kind::RedDust,
                pos: [0.5 + j[0] - 0.27, 0.7 + j[1] + 0.22, 0.5 + j[2]],
                motion: [0.0; 3]
            }]
        );
    }

    /// A cell is sampled with chance (1/16)^3 per sample: 1000 samples a tick hit one torch cell about 0.244 times.
    #[test]
    fn a_torch_is_sampled_at_the_vanilla_rate() {
        let torch_cell = |x: i32, y: i32, z: i32| {
            if [x, y, z] == [0, 0, 0] {
                50 << 4 | 5
            } else {
                0
            }
        };
        let never = |_: i32, _: i32, _: i32| false;
        let mut rng = JavaRandom::new(7);
        let ticks = 4000;
        let flames = (0..ticks)
            .flat_map(|_| {
                random_display_ticks([0, 0, 0], [0.0; 3], 0, &mut rng, &torch_cell, &never)
            })
            .filter(|e| {
                matches!(
                    e,
                    Effect::Particle {
                        kind: Kind::Flame,
                        ..
                    }
                )
            })
            .count();
        let rate = flames as f64 / f64::from(ticks);
        assert!((0.2..0.29).contains(&rate), "{rate}");
    }

    #[test]
    fn fire_sounds_once_in_24_and_lava_pops_once_in_100() {
        let count = |out: &[Effect], name| {
            out.iter()
                .filter(|e| matches!(e, Effect::Sound { name: n, .. } if *n == name))
                .count()
        };
        let fire = run(51 << 4, fire, 3, 24_000);
        assert!((850..1150).contains(&count(&fire, "fire.fire")));
        let lava = |cx: &mut Cx, p, r: &mut JavaRandom| lava_pops(cx, p, r);
        let out = run(0, lava, 5, 20_000);
        assert!((150..250).contains(&count(&out, "liquid.lavapop")));
        assert!((70..130).contains(&count(&out, "liquid.lava")));
    }
}
