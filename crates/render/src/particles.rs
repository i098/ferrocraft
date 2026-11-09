//! Particle system ported from 1.8.9 `EntityFX`/`EntityDiggingFX`/`EffectRenderer`: simulation plus billboard quads.
//!
//! Integration (owner of `lib.rs`/`gpu.rs`): keep a `Particles`, call `spawn_packet` for each Particle 0x2A,
//! `break_block`/`dig_hit` for local events, `tick` at 20 Hz, then `vertices` each frame. Block-crack quads
//! sample the block atlas and draw with the world pipeline; sheet quads sample `textures/particle/particles.png`
//! (16x16 tiles of 8 px) and need that image bound with the same pipeline.

use crate::assets::Atlas;
use crate::mesh::Vertex;

const MAX_PARTICLES: usize = 4000;

/// Particle types of the 1.8.9 `EnumParticleTypes` ids carried by Particle 0x2A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Explode,
    LargeExplode,
    HugeExplode,
    Bubble,
    Splash,
    Crit,
    MagicCrit,
    Smoke,
    LargeSmoke,
    Spell,
    Flame,
    Lava,
    Cloud,
    RedDust,
    Heart,
    Note,
    Portal,
    Droplet,
    AngryVillager,
    HappyVillager,
    /// `blockcrack`/`blockdust`: `id | meta << 12` in the packet data.
    BlockCrack,
}

impl Kind {
    pub fn from_id(id: i32) -> Option<Kind> {
        const IDS: [(i32, i32, Kind); 21] = [
            (0, 0, Kind::Explode),
            (2, 2, Kind::HugeExplode),
            (1, 1, Kind::LargeExplode),
            (4, 4, Kind::Bubble),
            (5, 6, Kind::Splash),
            (9, 9, Kind::Crit),
            (10, 10, Kind::MagicCrit),
            (11, 11, Kind::Smoke),
            (12, 12, Kind::LargeSmoke),
            (13, 17, Kind::Spell),
            (20, 20, Kind::AngryVillager),
            (21, 21, Kind::HappyVillager),
            (23, 23, Kind::Note),
            (24, 24, Kind::Portal),
            (26, 26, Kind::Flame),
            (27, 27, Kind::Lava),
            (29, 29, Kind::Cloud),
            (30, 30, Kind::RedDust),
            (34, 34, Kind::Heart),
            (37, 38, Kind::BlockCrack),
            (39, 39, Kind::Droplet),
        ];
        IDS.iter()
            .find(|(lo, hi, _)| (*lo..=*hi).contains(&id))
            .map(|e| e.2)
    }
}

#[derive(Clone, Copy)]
enum Sprite {
    /// Block-atlas texture with the 4x4 sub-square picked by the jitter (vanilla `EntityDiggingFX`).
    Block { state: u16, jitter: [f32; 2] },
    /// Sheet tile `base`; `anim` picks `base + 7 - age*8/max_age` like vanilla's generic particles.
    Sheet { base: u32, anim: bool },
    /// 4x4 frames of `textures/entity/explosion.png`, picked by age (`LargeExplosionParticle`).
    Explosion,
}

/// Which vanilla `onUpdate` and size rule a particle follows; `Generic` is `EntityFX.onUpdate`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Generic,
    Smoke,
    Explosion,
    RedDust,
    Crit,
    Portal,
    Rain,
    HugeEmitter,
    LargeExplosion,
}

#[derive(Clone, Copy)]
struct Particle {
    pos: [f64; 3],
    prev: [f64; 3],
    vel: [f64; 3],
    color: [f32; 3],
    alpha: f32,
    scale: f32,
    gravity: f64,
    drag: f64,
    age: u32,
    max_age: u32,
    on_ground: bool,
    sprite: Sprite,
    behavior: Behavior,
    /// Size before the grow-in or fade rule of `behavior` applies.
    init_scale: f32,
    /// Spawn point; portal particles move along a curve from it.
    origin: [f64; 3],
    /// Half the width of the collision box (`setSize(0.2)` is 0.1).
    half: f64,
}

#[derive(Default)]
pub struct Particles {
    list: Vec<Particle>,
    rng: u64,
}

/// Quads split by texture: `block` samples the block atlas, `sheet` samples `particles.png`,
/// `explosion` samples `textures/entity/explosion.png`.
#[derive(Default)]
pub struct Quads {
    pub block: Vec<Vertex>,
    pub sheet: Vec<Vertex>,
    pub explosion: Vec<Vertex>,
}

impl Particles {
    pub fn new() -> Particles {
        Particles {
            list: Vec::new(),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn gaussian(&mut self) -> f64 {
        let (a, b) = (self.rand().max(1e-12), self.rand());
        (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
    }

    /// `EntityFX` constructor: randomized, normalized start motion and a random lifetime.
    fn base(&mut self, pos: [f64; 3], motion: [f64; 3], sprite: Sprite) -> Particle {
        let mut v = [0.0; 3];
        for (o, m) in v.iter_mut().zip(motion) {
            *o = m + (self.rand() * 2.0 - 1.0) * 0.4;
        }
        let speed = (self.rand() + self.rand() + 1.0) * 0.15;
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
        let vel = [
            v[0] / len * speed * 0.4,
            v[1] / len * speed * 0.4 + 0.1,
            v[2] / len * speed * 0.4,
        ];
        let scale = ((self.rand() * 0.5 + 0.5) * 2.0) as f32;
        Particle {
            pos,
            prev: pos,
            vel,
            color: [1.0; 3],
            alpha: 1.0,
            scale,
            gravity: 0.0,
            drag: 0.98,
            age: 0,
            max_age: (4.0 / (self.rand() * 0.9 + 0.1)) as u32,
            on_ground: false,
            sprite,
            behavior: Behavior::Generic,
            init_scale: scale,
            origin: pos,
            half: 0.1,
        }
    }

    fn push(&mut self, p: Particle) {
        if self.list.len() >= MAX_PARTICLES {
            self.list.remove(0);
        }
        self.list.push(p);
    }

    /// One particle of `kind` with `motion` (already scaled by the packet speed).
    pub fn spawn(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3], data: i32) {
        let made = self
            .spawn_motion(kind, pos, motion, data)
            .or_else(|| self.spawn_timed(kind, pos, motion))
            .or_else(|| self.spawn_icon(kind, pos, motion));
        if let Some(p) = made {
            self.push(p);
        }
    }

    /// `Particle(world, x, y, z, 0, 0, 0)` with its motion scaled by 0.1 and `motion * factor` added: the
    /// shared constructor start of `SmokeParticle`, `RedstoneParticle` and `CriticalHitParticle`.
    fn damped(&mut self, pos: [f64; 3], sprite: Sprite, factor: f64, motion: [f64; 3]) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sprite);
        p.vel = [0, 1, 2].map(|i| p.vel[i] * 0.1 + motion[i] * factor);
        p.scale *= 0.75;
        p.init_scale = p.scale;
        p
    }

    /// `RainSplashParticle(world, x, y, z)`: also the base of `WaterSplashParticle`.
    fn rain(&mut self, pos: [f64; 3], tile: u32) -> Particle {
        let mut p = self.base(
            pos,
            [0.0; 3],
            Sprite::Sheet {
                base: tile,
                anim: false,
            },
        );
        p.vel[0] *= 0.3;
        p.vel[1] = self.rand() * 0.2 + 0.1;
        p.vel[2] *= 0.3;
        p.half = 0.005;
        p.gravity = 0.06;
        p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = Behavior::Rain;
        p
    }

    fn spawn_motion(
        &mut self,
        kind: Kind,
        pos: [f64; 3],
        motion: [f64; 3],
        data: i32,
    ) -> Option<Particle> {
        let sheet = |base, anim| Sprite::Sheet { base, anim };
        let mut p;
        match kind {
            Kind::BlockCrack => {
                let jitter = [(self.rand() * 3.0) as f32, (self.rand() * 3.0) as f32];
                p = self.base(
                    pos,
                    motion,
                    Sprite::Block {
                        state: (data & 0xFFFF) as u16,
                        jitter,
                    },
                );
                p.gravity = 1.0;
                p.color = [0.6; 3];
                p.scale /= 2.0;
            }
            Kind::Smoke | Kind::LargeSmoke => {
                // `SmokeParticle`: the lifetime is cut to an int before the scale multiplies it.
                let s = if kind == Kind::LargeSmoke { 2.5 } else { 1.0 };
                p = self.damped(pos, sheet(0, true), 1.0, motion);
                let g = (self.rand() * 0.3) as f32;
                p.color = [g; 3];
                p.scale *= s;
                p.init_scale = p.scale;
                p.max_age = ((8.0 / (self.rand() * 0.8 + 0.2)) as u32 as f32 * s) as u32;
                p.behavior = Behavior::Smoke;
            }
            Kind::Explode => {
                p = self.base(pos, motion, sheet(0, true));
                p.vel = [0, 1, 2].map(|i| motion[i] + (self.rand() * 2.0 - 1.0) * 0.05);
                let g = (self.rand() * 0.3 + 0.7) as f32;
                p.color = [g; 3];
                p.scale = (self.rand() * self.rand() * 6.0 + 1.0) as f32;
                p.max_age = (16.0 / (self.rand() * 0.8 + 0.2)) as u32 + 2;
                p.behavior = Behavior::Explosion;
            }
            Kind::LargeExplode => {
                p = self.base(pos, [0.0; 3], Sprite::Explosion);
                p.vel = [0.0; 3];
                p.max_age = 6 + (self.rand() * 4.0) as u32;
                let g = (self.rand() * 0.6 + 0.4) as f32;
                p.color = [g; 3];
                // The x motion is the emitter's `age / maxAge`; the quad is `2 * scaleFactor` wide each side.
                p.scale = 20.0 * (1.0 - motion[0] as f32 * 0.5);
                p.behavior = Behavior::LargeExplosion;
            }
            Kind::HugeExplode => {
                p = self.base(pos, [0.0; 3], sheet(0, false));
                p.max_age = 8;
                p.behavior = Behavior::HugeEmitter;
            }
            _ => return None,
        }
        Some(p)
    }

    fn spawn_timed(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Option<Particle> {
        let sheet = |base, anim| Sprite::Sheet { base, anim };
        let mut p;
        match kind {
            Kind::Lava => {
                p = self.base(pos, [0.0; 3], sheet(49, false));
                p.vel = [0.0, 0.0, 0.0];
                p.vel[0] = (self.rand() * 2.0 - 1.0) * 0.4 * 0.4;
                p.vel[2] = (self.rand() * 2.0 - 1.0) * 0.4 * 0.4;
                p.vel[1] = self.rand() * 0.4 * 0.4 + 0.1;
                p.gravity = 0.75;
                p.max_age = (16.0 / (self.rand() * 0.8 + 0.2)) as u32;
            }
            Kind::Droplet => {
                let tile = 19 + (self.rand() * 4.0) as u32;
                p = self.rain(pos, tile);
            }
            Kind::Splash => {
                // `WaterSplashParticle`: the tile right of the rain tile, a slower fall.
                let tile = 20 + (self.rand() * 4.0) as u32;
                p = self.rain(pos, tile);
                p.gravity = 0.04;
                if motion[1] == 0.0 && (motion[0] != 0.0 || motion[2] != 0.0) {
                    p.vel = [motion[0], motion[1] + 0.1, motion[2]];
                }
            }
            Kind::Bubble => {
                p = self.base(pos, motion, sheet(32, false));
                p.gravity = -0.02;
                p.drag = 0.85;
                p.scale *= 0.5;
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
            }
            Kind::Cloud => {
                p = self.base(pos, motion, sheet(0, true));
                p.color = [1.0; 3];
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.3)) as u32;
            }
            Kind::RedDust => {
                // `RedstoneParticle`: the packet motion carries the colour; a zero red channel means 1.
                p = self.damped(pos, sheet(0, true), 0.0, [0.0; 3]);
                let r = if motion[0] == 0.0 {
                    1.0
                } else {
                    motion[0] as f32
                };
                let f = (self.rand() * 0.4 + 0.6) as f32;
                let mut tint = |c: f32| ((self.rand() * 0.2) as f32 + 0.8) * c * f;
                p.color = [tint(r), tint(motion[1] as f32), tint(motion[2] as f32)];
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
                p.behavior = Behavior::RedDust;
            }
            Kind::Spell => {
                p = self.base(pos, motion, sheet(128, true));
                p.max_age = (40.0 / (self.rand() * 0.8 + 0.2)) as u32;
                p.gravity = -0.2;
            }
            _ => return None,
        }
        Some(p)
    }

    fn spawn_icon(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Option<Particle> {
        let sheet = |base, anim| Sprite::Sheet { base, anim };
        let mut p;
        match kind {
            Kind::Crit | Kind::MagicCrit => {
                p = self.damped(pos, sheet(65, false), 0.4, motion);
                let g = (self.rand() * 0.3 + 0.6) as f32;
                p.color = [g; 3];
                p.max_age = (6.0 / (self.rand() * 0.8 + 0.6)) as u32;
                p.behavior = Behavior::Crit;
                // The vanilla constructor ends with one `tick()`; the world is not at hand here.
                self.tick_one(&mut p, &|_, _, _| false, &mut Vec::new());
                if kind == Kind::MagicCrit {
                    p.color = [p.color[0] * 0.3, p.color[1] * 0.8, p.color[2]];
                    p.sprite = sheet(66, false);
                }
            }
            Kind::Flame => {
                p = self.base(pos, [0.0; 3], sheet(48, false));
                p.vel = motion;
                p.max_age = 8 + (self.rand() * 8.0) as u32;
                p.drag = 0.96;
                p.scale *= 0.5;
            }
            Kind::Heart | Kind::HappyVillager | Kind::AngryVillager => {
                let tile = match kind {
                    Kind::Heart => 80,
                    Kind::HappyVillager => 82,
                    _ => 81,
                };
                p = self.base(pos, [0.0; 3], sheet(tile, false));
                p.vel = [motion[0] * 0.01, motion[1] * 0.01 + 0.1, motion[2] * 0.01];
                p.scale *= 1.5;
                p.max_age = 16;
                p.drag = 0.86;
            }
            Kind::Note => {
                p = self.base(pos, [0.0; 3], sheet(64, false));
                p.vel = [motion[0] * 0.01, motion[1] * 0.01 + 0.2, motion[2] * 0.01];
                p.scale *= 2.0;
                p.max_age = 6;
                p.drag = 0.66;
            }
            Kind::Portal => {
                // `PortalParticle`: a fixed curve from the spawn point along `motion`; tiles 0..8 tinted purple.
                p = self.base(pos, motion, sheet(0, false));
                p.vel = motion;
                let f = (self.rand() * 0.6 + 0.4) as f32;
                p.scale = (self.rand() * 0.2 + 0.5) as f32;
                p.init_scale = p.scale;
                p.color = [f * 0.9, f * 0.3, f];
                p.max_age = (self.rand() * 10.0) as u32 + 40;
                p.sprite = sheet((self.rand() * 8.0) as u32, false);
                p.behavior = Behavior::Portal;
            }
            _ => return None,
        }
        Some(p)
    }

    /// Particle 0x2A: `count == 0` spawns one particle moving by `offset * speed`; otherwise `count`
    /// particles with gaussian position spread `offset` and gaussian velocity `speed`.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_packet(
        &mut self,
        id: i32,
        pos: [f32; 3],
        offset: [f32; 3],
        speed: f32,
        count: i32,
        data: &[i32],
    ) {
        let Some(kind) = Kind::from_id(id) else {
            return;
        };
        let data0 = if kind == Kind::BlockCrack {
            let d = data.first().copied().unwrap_or(0);
            ((d & 0xfff) << 4) | ((d >> 12) & 0xf)
        } else {
            0
        };
        let pos = pos.map(f64::from);
        if count == 0 {
            let m = offset.map(|o| f64::from(o * speed));
            self.spawn(kind, pos, m, data0);
            return;
        }
        for _ in 0..count.min(200) {
            let p = [
                pos[0] + self.gaussian() * f64::from(offset[0]),
                pos[1] + self.gaussian() * f64::from(offset[1]),
                pos[2] + self.gaussian() * f64::from(offset[2]),
            ];
            let m = [
                self.gaussian() * f64::from(speed),
                self.gaussian() * f64::from(speed),
                self.gaussian() * f64::from(speed),
            ];
            self.spawn(kind, p, m, data0);
        }
    }

    /// `EffectRenderer.addBlockDestroyEffects`: 4x4x4 shards flying out of the broken block.
    pub fn break_block(&mut self, [x, y, z]: [i32; 3], state: u16) {
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    let c = [
                        f64::from(x) + (f64::from(i) + 0.5) / 4.0,
                        f64::from(y) + (f64::from(j) + 0.5) / 4.0,
                        f64::from(z) + (f64::from(k) + 0.5) / 4.0,
                    ];
                    let m = [
                        c[0] - f64::from(x) - 0.5,
                        c[1] - f64::from(y) - 0.5,
                        c[2] - f64::from(z) - 0.5,
                    ];
                    self.spawn(Kind::BlockCrack, c, m, i32::from(state));
                }
            }
        }
    }

    /// `EffectRenderer.addBlockHitEffects`: one shard on the hit `face` (0 down .. 5 east) while mining.
    /// `bounds` is the block's `[min x, y, z, max x, y, z]` inside its cell, as vanilla `getMinX()`..`getMaxZ()`.
    pub fn dig_hit(&mut self, [x, y, z]: [i32; 3], face: u8, state: u16, bounds: [f64; 6]) {
        let o = [x, y, z].map(f64::from);
        // Vanilla `float f = 0.1F` widened to double.
        let e = f64::from(0.1_f32);
        let mut c = [0, 1, 2]
            .map(|i| o[i] + self.rand() * (bounds[i + 3] - bounds[i] - e * 2.0) + e + bounds[i]);
        match face {
            0 => c[1] = o[1] + bounds[1] - e,
            1 => c[1] = o[1] + bounds[4] + e,
            2 => c[2] = o[2] + bounds[2] - e,
            3 => c[2] = o[2] + bounds[5] + e,
            4 => c[0] = o[0] + bounds[0] - e,
            _ => c[0] = o[0] + bounds[3] + e,
        }
        self.spawn(Kind::BlockCrack, c, [0.0; 3], i32::from(state));
        if let Some(p) = self.list.last_mut() {
            // `multiplyVelocity(0.2)` keeps the 0.1 upward offset; `multiplySize(0.6)` also shrinks the box.
            p.vel = [p.vel[0] * 0.2, (p.vel[1] - 0.1) * 0.2 + 0.1, p.vel[2] * 0.2];
            p.scale *= 0.6;
            p.half = 0.06;
        }
    }

    /// One 20 Hz tick (`EntityFX.onUpdate`); `solid(x, y, z)` reports blocks that stop particles.
    pub fn tick(&mut self, solid: impl Fn(i32, i32, i32) -> bool) {
        let mut list = std::mem::take(&mut self.list);
        let mut spawned = Vec::new();
        list.retain_mut(|p| self.tick_one(p, &solid, &mut spawned));
        self.list = list;
        for (kind, pos, motion) in spawned {
            self.spawn(kind, pos, motion, 0);
        }
    }

    /// Ticks one particle; false removes it. `spawned` collects what the particle emits.
    fn tick_one(
        &mut self,
        p: &mut Particle,
        solid: &impl Fn(i32, i32, i32) -> bool,
        spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
    ) -> bool {
        p.prev = p.pos;
        if let Some(alive) = self.tick_special(p, solid, spawned) {
            return alive;
        }
        if p.age >= p.max_age {
            return false;
        }
        p.age += 1;
        move_generic(p, solid);
        if p.on_ground {
            p.vel[0] *= f64::from(0.7_f32);
            p.vel[2] *= f64::from(0.7_f32);
        }
        true
    }

    /// Behaviors with their own lifetime rule; `None` for those that use the generic age check.
    fn tick_special(
        &mut self,
        p: &mut Particle,
        solid: &impl Fn(i32, i32, i32) -> bool,
        spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
    ) -> Option<bool> {
        Some(match p.behavior {
            Behavior::Portal => {
                // No collision: the position follows a curve of age/lifetime and `y` rises by `1 - t`.
                let t = p.age as f32 / p.max_age as f32;
                let f = f64::from(1.0 - (-t + t * t * 2.0));
                p.pos = [
                    p.origin[0] + p.vel[0] * f,
                    p.origin[1] + p.vel[1] * f + f64::from(1.0 - t),
                    p.origin[2] + p.vel[2] * f,
                ];
                let alive = p.age < p.max_age;
                p.age += 1;
                alive
            }
            Behavior::LargeExplosion => {
                p.age += 1;
                p.age < p.max_age
            }
            Behavior::HugeEmitter => {
                if p.age >= p.max_age {
                    return Some(false);
                }
                for _ in 0..6 {
                    let at = [0, 1, 2].map(|i| p.pos[i] + (self.rand() - self.rand()) * 4.0);
                    let t = f64::from(p.age as f32 / p.max_age as f32);
                    spawned.push((Kind::LargeExplode, at, [t, 0.0, 0.0]));
                }
                p.age += 1;
                true
            }
            Behavior::Rain => self.tick_rain(p, solid),
            _ => return None,
        })
    }

    /// `RainSplashParticle.onUpdate`: gravity is subtracted whole, and a landed drop dies half the time.
    fn tick_rain(&mut self, p: &mut Particle, solid: &impl Fn(i32, i32, i32) -> bool) -> bool {
        p.vel[1] -= p.gravity;
        step(p, solid);
        p.vel = p.vel.map(|v| v * f64::from(0.98_f32));
        let mut dead = p.age >= p.max_age;
        p.age += 1;
        if p.on_ground {
            dead |= self.rand() < 0.5;
            p.vel[0] *= f64::from(0.7_f32);
            p.vel[2] *= f64::from(0.7_f32);
        }
        // shortcut: only solid full cubes are known here, not the liquid level or the real block top.
        let cell = p.pos.map(f64::floor);
        dead |= solid(cell[0] as i32, cell[1] as i32, cell[2] as i32) && p.pos[1] < cell[1] + 1.0;
        !dead
    }

    /// Camera-facing quads for all live particles; `partial` is the tick fraction in `0..1`.
    pub fn vertices(
        &self,
        atlas: &Atlas,
        cam_yaw: f32,
        cam_pitch: f32,
        cam_pos: [f64; 3],
        partial: f32,
    ) -> Quads {
        let (right, up) = basis(cam_yaw, cam_pitch);
        let mut out = Quads::default();
        for p in &self.list {
            if p.behavior == Behavior::HugeEmitter {
                continue;
            }
            let t = f64::from(partial);
            let c = [0, 1, 2].map(|i| (p.prev[i] + (p.pos[i] - p.prev[i]) * t - cam_pos[i]) as f32);
            let Some((layer, uv)) = sprite_uv(p, atlas, partial) else {
                continue;
            };
            let target = match layer {
                Layer::Block => &mut out.block,
                Layer::Sheet => &mut out.sheet,
                Layer::Explosion => &mut out.explosion,
            };
            push_quad(target, p, render_scale(p, partial), c, uv, right, up);
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Layer {
    Block,
    Sheet,
    Explosion,
}

/// Particle size at draw time: smoke, redstone and crit grow in over the first 1/32 of their life,
/// portal particles follow `1 - (1 - t)^2`.
fn render_scale(p: &Particle, partial: f32) -> f32 {
    let t = (p.age as f32 + partial) / p.max_age.max(1) as f32;
    match p.behavior {
        Behavior::Smoke | Behavior::RedDust | Behavior::Crit => {
            p.init_scale * (t * 32.0).clamp(0.0, 1.0)
        }
        Behavior::Portal => p.init_scale * (1.0 - (1.0 - t) * (1.0 - t)),
        _ => p.scale,
    }
}

/// Which texture the sprite samples, and its two corner uvs; `None` when nothing is drawn.
fn sprite_uv(p: &Particle, atlas: &Atlas, partial: f32) -> Option<(Layer, [[f32; 2]; 2])> {
    Some(match p.sprite {
        Sprite::Block { state, jitter } => {
            let name = crate::blocks::look(state).tex[crate::blocks::NORTH];
            let a = atlas.uv(name, [jitter[0] / 4.0, jitter[1] / 4.0]);
            let b = atlas.uv(name, [(jitter[0] + 1.0) / 4.0, (jitter[1] + 1.0) / 4.0]);
            (Layer::Block, [a, b])
        }
        Sprite::Sheet { base, anim } => {
            let frame = 7u32.saturating_sub(p.age * 8 / p.max_age.max(1));
            let tile = base + if anim { frame } else { 0 };
            let (u, v) = ((tile % 16) as f32 / 16.0, (tile / 16) as f32 / 16.0);
            (Layer::Sheet, [[u, v], [u + 1.0 / 16.0, v + 1.0 / 16.0]])
        }
        Sprite::Explosion => {
            // `LargeExplosionParticle`: 4x4 frames over its life; the quad is mirrored in u.
            let n = ((p.age as f32 + partial) * 15.0 / p.max_age as f32) as u32;
            if n > 15 {
                return None;
            }
            let (u, v) = ((n % 4) as f32 / 4.0, (n / 4) as f32 / 4.0);
            (Layer::Explosion, [[u + 0.24975, v], [u, v + 0.24975]])
        }
    })
}

fn push_quad(
    target: &mut Vec<Vertex>,
    p: &Particle,
    scale: f32,
    c: [f32; 3],
    uv: [[f32; 2]; 2],
    right: [f32; 3],
    up: [f32; 3],
) {
    let half = 0.1 * scale;
    let color = [p.color[0], p.color[1], p.color[2], p.alpha];
    for (dx, dy, u, v) in [
        (-1.0, -1.0, 0, 1),
        (1.0, -1.0, 1, 1),
        (1.0, 1.0, 1, 0),
        (-1.0, 1.0, 0, 0),
    ] {
        let pos = [0, 1, 2].map(|i| c[i] + (right[i] * dx + up[i] * dy) * half);
        target.push(Vertex {
            pos,
            uv: [uv[u][0], uv[v][1]],
            color,
            light: [1.0, 1.0],
        });
    }
}

/// The per-behavior motion of one tick (`onUpdate` after the age check).
fn move_generic(p: &mut Particle, solid: &impl Fn(i32, i32, i32) -> bool) {
    match p.behavior {
        Behavior::Smoke | Behavior::Explosion => {
            p.vel[1] += 0.004;
            step(p, solid);
            let drag = if p.behavior == Behavior::Smoke {
                creep_on_floor(p);
                0.96_f32
            } else {
                0.9_f32
            };
            p.vel = p.vel.map(|v| v * f64::from(drag));
        }
        Behavior::RedDust => {
            step(p, solid);
            creep_on_floor(p);
            p.vel = p.vel.map(|v| v * f64::from(0.96_f32));
        }
        Behavior::Crit => {
            step(p, solid);
            p.color[1] = (f64::from(p.color[1]) * 0.96) as f32;
            p.color[2] = (f64::from(p.color[2]) * 0.9) as f32;
            p.vel = p.vel.map(|v| v * f64::from(0.7_f32));
            p.vel[1] -= f64::from(0.02_f32);
        }
        _ => {
            p.vel[1] -= 0.04 * p.gravity;
            step(p, solid);
            p.vel = p.vel.map(|v| v * p.drag);
        }
    }
}

/// A particle that did not move vertically (it hit a floor or ceiling) speeds up sideways by 1.1.
fn creep_on_floor(p: &mut Particle) {
    if p.pos[1] == p.prev[1] {
        p.vel[0] *= 1.1;
        p.vel[2] *= 1.1;
    }
}

/// Moves by `vel` one axis at a time (y, x, z); a blocked axis zeroes its velocity.
fn step(p: &mut Particle, solid: &impl Fn(i32, i32, i32) -> bool) {
    let want = p.vel;
    for axis in [1, 0, 2] {
        let mut next = p.pos;
        next[axis] += want[axis];
        if blocked(solid, next, p.half) {
            if axis == 1 {
                p.on_ground = want[1] < 0.0;
            }
            p.vel[axis] = 0.0;
        } else {
            p.pos = next;
            if axis == 1 {
                p.on_ground = false;
            }
        }
    }
}

/// Whether a box of half-width `h` centred on `pos` touches a solid block (checked by its eight corners).
fn blocked(solid: &impl Fn(i32, i32, i32) -> bool, pos: [f64; 3], h: f64) -> bool {
    [-h, h].iter().any(|dx| {
        [-h, h].iter().any(|dy| {
            [-h, h].iter().any(|dz| {
                solid(
                    (pos[0] + dx).floor() as i32,
                    (pos[1] + dy).floor() as i32,
                    (pos[2] + dz).floor() as i32,
                )
            })
        })
    })
}

/// Screen-right and screen-up world vectors for vanilla yaw/pitch degrees.
fn basis(yaw: f32, pitch: f32) -> ([f32; 3], [f32; 3]) {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let f = [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()];
    let r = {
        let r = [-f[2], 0.0, f[0]];
        let l = (r[0] * r[0] + r[2] * r[2]).sqrt().max(1e-6);
        [r[0] / l, 0.0, r[2] / l]
    };
    let u = [
        r[1] * f[2] - r[2] * f[1],
        r[2] * f[0] - r[0] * f[2],
        r[0] * f[1] - r[1] * f[0],
    ];
    (r, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_block_spawns_64_shards_that_fall_and_expire() {
        let mut ps = Particles::new();
        ps.break_block([0, 10, 0], 1 << 4);
        assert_eq!(ps.len(), 64);
        for _ in 0..200 {
            ps.tick(|_, y, _| y < 5);
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn particles_rest_on_floor() {
        let mut ps = Particles::new();
        ps.spawn(Kind::BlockCrack, [0.5, 6.5, 0.5], [0.0; 3], 16);
        for _ in 0..3 {
            ps.tick(|_, y, _| y < 5);
        }
        assert!(ps.list.iter().all(|p| p.pos[1] >= 5.0));
    }

    #[test]
    fn packet_count_zero_spawns_one_and_count_n_spawns_n() {
        let mut ps = Particles::new();
        ps.spawn_packet(9, [0.0; 3], [0.0, 1.0, 0.0], 0.1, 0, &[]);
        assert_eq!(ps.len(), 1);
        ps.spawn_packet(9, [0.0; 3], [0.5; 3], 0.1, 10, &[]);
        assert_eq!(ps.len(), 11);
        ps.spawn_packet(99, [0.0; 3], [0.0; 3], 0.0, 5, &[]);
        assert_eq!(ps.len(), 11);
    }

    #[test]
    fn mining_shard_on_a_lower_slab_top_sits_just_above_its_surface() {
        let mut ps = Particles::new();
        for _ in 0..50 {
            ps.dig_hit([3, 10, 4], 1, 44 << 4, [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]);
        }
        for p in &ps.list {
            // Vanilla `maxY + (double) 0.1F`: the float widens to 0.10000000149, not 0.1.
            assert!(
                (p.pos[1] - (10.5 + f64::from(0.1_f32))).abs() < 1e-12,
                "y {}",
                p.pos[1]
            );
            assert!((3.1..=3.9).contains(&p.pos[0]) && (4.1..=4.9).contains(&p.pos[2]));
        }
    }

    #[test]
    fn mining_shard_velocity_uses_0_2_around_the_upward_offset() {
        let mut ps = Particles::new();
        for _ in 0..200 {
            ps.dig_hit([0, 0, 0], 4, 1 << 4, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
        // Fresh `EntityFX` motion is at most 0.45 * 0.4 per axis, so x and z stay within 0.2 * that.
        for p in &ps.list {
            assert!(
                p.vel[0].abs() <= 0.036 && p.vel[2].abs() <= 0.036,
                "{:?}",
                p.vel
            );
            assert!((0.064..=0.136).contains(&p.vel[1]), "{:?}", p.vel);
            // Vanilla size is 1..2, halved by the shard and cut to 0.6 by `multiplySize`.
            assert!((0.3..=0.6).contains(&p.scale), "{}", p.scale);
        }
    }

    #[test]
    fn portal_particle_follows_its_curve_and_dies_after_its_lifetime() {
        let mut ps = Particles::new();
        ps.spawn(Kind::Portal, [0.0; 3], [1.0, 0.0, 0.0], 0);
        let life = ps.list[0].max_age;
        assert!((40..50).contains(&life));
        ps.tick(|_, _, _| false);
        // t = 0: f = 1, so the particle sits at origin + motion and 1 block above.
        assert_eq!(ps.list[0].pos, [1.0, 1.0, 0.0]);
        for _ in 0..life {
            ps.tick(|_, _, _| false);
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn huge_explosion_emits_six_large_explosions_for_eight_ticks() {
        let mut ps = Particles::new();
        ps.spawn(Kind::HugeExplode, [0.0; 3], [0.0; 3], 0);
        for _ in 0..8 {
            ps.tick(|_, _, _| false);
        }
        let large = ps
            .list
            .iter()
            .filter(|p| p.behavior == Behavior::LargeExplosion)
            .count();
        assert!(large > 0 && large <= 48);
        ps.tick(|_, _, _| false);
        assert!(ps.list.iter().all(|p| p.behavior != Behavior::HugeEmitter));
    }

    #[test]
    fn red_dust_without_red_motion_tints_red() {
        let mut ps = Particles::new();
        for _ in 0..20 {
            ps.spawn(Kind::RedDust, [0.0; 3], [0.0; 3], 0);
        }
        // red = (0.8..1.0) * 1 * (0.6..1.0); green and blue are 0.
        for p in &ps.list {
            assert!((0.48..=1.0).contains(&p.color[0]));
            assert_eq!(p.color[1..], [0.0, 0.0]);
        }
    }
}
