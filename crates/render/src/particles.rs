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
    /// `spell` (13), `instantSpell` (14), `mobSpell` (15), `mobSpellAmbient` (16), `witchMagic` (17).
    Spell,
    InstantSpell,
    MobSpell,
    MobSpellAmbient,
    WitchMagic,
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
    /// `fireworksSpark` (3).
    FireworksSpark,
    /// `suspended` (7).
    Suspended,
    /// `depthsuspend` (8) and `townaura` (22).
    DepthSuspend,
    DripWater,
    DripLava,
    Enchant,
    SnowShovel,
    /// `blockcrack`/`blockdust`: `id | meta << 12` in the packet data.
    BlockCrack,
}

impl Kind {
    pub fn from_id(id: i32) -> Option<Kind> {
        const IDS: [(i32, i32, Kind); 33] = [
            (0, 0, Kind::Explode),
            (2, 2, Kind::HugeExplode),
            (1, 1, Kind::LargeExplode),
            (4, 4, Kind::Bubble),
            (5, 6, Kind::Splash),
            (9, 9, Kind::Crit),
            (10, 10, Kind::MagicCrit),
            (11, 11, Kind::Smoke),
            (12, 12, Kind::LargeSmoke),
            (13, 13, Kind::Spell),
            (14, 14, Kind::InstantSpell),
            (15, 15, Kind::MobSpell),
            (16, 16, Kind::MobSpellAmbient),
            (17, 17, Kind::WitchMagic),
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
            (3, 3, Kind::FireworksSpark),
            (7, 7, Kind::Suspended),
            (8, 8, Kind::DepthSuspend),
            (18, 18, Kind::DripWater),
            (19, 19, Kind::DripLava),
            (22, 22, Kind::DepthSuspend),
            (25, 25, Kind::Enchant),
            (32, 32, Kind::SnowShovel),
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
    Spell,
    HugeEmitter,
    LargeExplosion,
    Suspend,
    Enchant,
    DripWater,
    DripLava,
    Snow,
    Spark,
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
            .or_else(|| self.spawn_misc(kind, pos, motion))
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
            Kind::Spell
            | Kind::InstantSpell
            | Kind::MobSpell
            | Kind::MobSpellAmbient
            | Kind::WitchMagic => {
                p = self.spell(kind, pos, motion);
            }
            _ => return None,
        }
        Some(p)
    }

    /// `SpellParticle` and its colored variants.
    fn spell(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        // The horizontal start motion is `0.5 - random`, not the packet's.
        let start = [0.5 - self.rand(), motion[1], 0.5 - self.rand()];
        let tile = if matches!(kind, Kind::Spell | Kind::MobSpell | Kind::MobSpellAmbient) {
            128
        } else {
            144
        };
        let mut p = self.base(
            pos,
            start,
            Sprite::Sheet {
                base: tile,
                anim: true,
            },
        );
        p.vel[1] *= f64::from(0.2_f32);
        if motion[0] == 0.0 && motion[2] == 0.0 {
            p.vel[0] *= f64::from(0.1_f32);
            p.vel[2] *= f64::from(0.1_f32);
        }
        p.scale *= 0.75;
        p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = Behavior::Spell;
        match kind {
            Kind::MobSpell | Kind::MobSpellAmbient => {
                p.color = motion.map(|m| m as f32);
                if kind == Kind::MobSpellAmbient {
                    p.alpha = 0.15;
                }
            }
            Kind::WitchMagic => {
                let f = (self.rand() * 0.5 + 0.35) as f32;
                p.color = [f, 0.0, f];
            }
            _ => {}
        }
        p
    }

    /// Drifting, dripping, enchanting, snow and firework particles.
    fn spawn_misc(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Option<Particle> {
        let sheet = |base, anim| Sprite::Sheet { base, anim };
        let p = match kind {
            Kind::DepthSuspend | Kind::HappyVillager | Kind::Suspended => {
                self.suspended(kind, pos, motion)
            }
            Kind::Enchant => self.enchant(pos, motion),
            Kind::DripWater | Kind::DripLava => self.drip(kind, pos),
            Kind::SnowShovel => {
                let mut p = self.damped(pos, sheet(0, true), 1.0, motion);
                p.color = [1.0 - (self.rand() * f64::from(0.3_f32)) as f32; 3];
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
                p.behavior = Behavior::Snow;
                p
            }
            Kind::FireworksSpark => {
                let mut p = self.base(pos, [0.0; 3], sheet(160, true));
                p.vel = motion;
                p.scale *= 0.75;
                p.init_scale = p.scale;
                p.alpha = 0.99;
                p.max_age = 48 + (self.rand() * 12.0) as u32;
                p.behavior = Behavior::Spark;
                p
            }
            _ => return None,
        };
        Some(p)
    }

    /// `DepthSuspendParticle` (the happy villager variant is tile 82, white) and `SuspendedParticle`.
    fn suspended(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        let sheet = |base, anim| Sprite::Sheet { base, anim };
        let mut p;
        if kind == Kind::Suspended {
            // shortcut: vanilla also removes it outside water; no liquid lookup reaches `tick`.
            p = self.base([pos[0], pos[1] - 0.125, pos[2]], [0.0; 3], sheet(0, false));
            p.vel = [0.0; 3];
            p.color = [0.4, 0.4, 0.7];
            p.half = 0.005;
            p.scale *= (self.rand() * 0.6 + 0.2) as f32;
            p.max_age = (16.0 / (self.rand() * 0.8 + 0.2)) as u32;
        } else {
            p = self.base(pos, motion, sheet(0, false));
            p.color = [(self.rand() * 0.1 + 0.2) as f32; 3];
            p.half = 0.01;
            p.scale *= (self.rand() * 0.6 + 0.5) as f32;
            p.vel = p.vel.map(|v| v * f64::from(0.02_f32));
            p.max_age = (20.0 / (self.rand() * 0.8 + 0.2)) as u32;
            if kind == Kind::HappyVillager {
                p.sprite = sheet(82, false);
                p.color = [1.0; 3];
            }
        }
        p.behavior = Behavior::Suspend;
        p
    }

    /// `EnchantingParticle`: a fixed curve from `pos + motion` back to `pos`.
    fn enchant(&mut self, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        let tile = 225 + (self.rand() * 26.0) as u32;
        let mut p = self.base(
            pos,
            [0.0; 3],
            Sprite::Sheet {
                base: tile,
                anim: false,
            },
        );
        p.vel = motion;
        p.origin = pos;
        p.pos = [0, 1, 2].map(|i| pos[i] + motion[i]);
        p.prev = p.pos;
        let f = (self.rand() * 0.6 + 0.4) as f32;
        p.color = [f * 0.9, f * 0.9, f];
        p.scale = (self.rand() * 0.5 + 0.2) as f32;
        p.max_age = (self.rand() * 10.0) as u32 + 30;
        p.behavior = Behavior::Enchant;
        p
    }

    /// `LiquidDripParticle` constructor.
    fn drip(&mut self, kind: Kind, pos: [f64; 3]) -> Particle {
        let mut p = self.base(
            pos,
            [0.0; 3],
            Sprite::Sheet {
                base: 113,
                anim: false,
            },
        );
        p.vel = [0.0; 3];
        let water = kind == Kind::DripWater;
        p.color = if water {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        p.half = 0.005;
        p.gravity = f64::from(0.06_f32);
        p.max_age = (64.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = if water {
            Behavior::DripWater
        } else {
            Behavior::DripLava
        };
        p
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
            Kind::Heart | Kind::AngryVillager => {
                let tile = if kind == Kind::Heart { 80 } else { 81 };
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

    /// `EntityLivingBase.onEntityUpdate`, the potion part: about every other tick an entity with
    /// active effects (`color` from metadata index 7, `ambient` from index 8) sheds one swirl.
    pub fn potion_swirl(
        &mut self,
        pos: [f64; 3],
        (width, height): (f32, f32),
        color: i32,
        ambient: bool,
        invisible: bool,
    ) {
        if color <= 0 {
            return;
        }
        let mut shed = if invisible {
            (self.rand() * 15.0) as u32 == 0
        } else {
            self.rand() < 0.5
        };
        if ambient {
            shed &= (self.rand() * 5.0) as u32 == 0;
        }
        if !shed {
            return;
        }
        let at = [
            pos[0] + (self.rand() - 0.5) * f64::from(width),
            pos[1] + self.rand() * f64::from(height),
            pos[2] + (self.rand() - 0.5) * f64::from(width),
        ];
        let rgb = [16, 8, 0].map(|shift| f64::from(color >> shift & 255) / 255.0);
        let kind = if ambient {
            Kind::MobSpellAmbient
        } else {
            Kind::MobSpell
        };
        self.spawn(kind, at, rgb, 0);
    }

    /// `Explosion.doExplosionB`: the burst at `pos`, then smoke and puffs from every destroyed block
    /// in `records` (block positions). A radius of 2 or more uses the huge burst.
    pub fn explosion(&mut self, pos: [f64; 3], radius: f32, records: &[[i32; 3]]) {
        let burst = if radius >= 2.0 {
            Kind::HugeExplode
        } else {
            Kind::LargeExplode
        };
        self.spawn(burst, pos, [1.0, 0.0, 0.0], 0);
        for b in records {
            let c = [0, 1, 2].map(|i| f64::from(b[i]) + self.rand());
            let d = [0, 1, 2].map(|i| c[i] - pos[i]);
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
            let k = 0.5 / (len / f64::from(radius) + 0.1) * (self.rand() * self.rand() + 0.3);
            let m = d.map(|v| v / len * k);
            self.spawn(
                Kind::Explode,
                [0, 1, 2].map(|i| (c[i] + pos[i]) / 2.0),
                m,
                0,
            );
            self.spawn(Kind::Smoke, c, m, 0);
        }
    }

    /// The particle cases of `WorldRenderer.doEvent` that need only the particle pass: 2000 dispenser
    /// smoke (`data` is the facing), 2003 ender eye portal ring and 2004 spawner smoke and flames.
    pub fn world_effect(&mut self, id: i32, pos: [i32; 3], data: i32) {
        let b = pos.map(f64::from);
        match id {
            2000 => self.dispenser_smoke(b, data),
            2003 => self.eye_portal_ring(b),
            2004 => self.spawner_burst(b),
            _ => {}
        }
    }

    fn dispenser_smoke(&mut self, b: [f64; 3], data: i32) {
        let (dx, dz) = (f64::from(data % 3 - 1), f64::from(data / 3 % 3 - 1));
        let c = [b[0] + dx * 0.6 + 0.5, b[1] + 0.5, b[2] + dz * 0.6 + 0.5];
        for _ in 0..10 {
            let d = self.rand() * 0.2 + 0.01;
            let at = [
                c[0] + dx * 0.01 + (self.rand() - 0.5) * dz * 0.5,
                c[1] + (self.rand() - 0.5) * 0.5,
                c[2] + dz * 0.01 + (self.rand() - 0.5) * dx * 0.5,
            ];
            let m = [
                dx * d + self.gaussian() * 0.01,
                -0.03 + self.gaussian() * 0.01,
                dz * d + self.gaussian() * 0.01,
            ];
            self.spawn(Kind::Smoke, at, m, 0);
        }
    }

    fn eye_portal_ring(&mut self, b: [f64; 3]) {
        let (x, z) = (b[0] + 0.5, b[2] + 0.5);
        let mut a = 0.0_f64;
        while a < std::f64::consts::TAU {
            for k in [-5.0, -7.0] {
                let at = [x + a.cos() * 5.0, b[1] - 0.4, z + a.sin() * 5.0];
                self.spawn(Kind::Portal, at, [a.cos() * k, 0.0, a.sin() * k], 0);
            }
            a += 0.157_079_632_679_489_66;
        }
    }

    fn spawner_burst(&mut self, b: [f64; 3]) {
        for _ in 0..20 {
            let at = [0, 1, 2].map(|i| b[i] + 0.5 + (self.rand() - 0.5) * 2.0);
            self.spawn(Kind::Smoke, at, [0.0; 3], 0);
            self.spawn(Kind::Flame, at, [0.0; 3], 0);
        }
    }

    /// World effect 2002: a splash potion shatters into 100 swirls tinted by the potion `rgb`.
    pub fn splash_potion(&mut self, pos: [i32; 3], rgb: i32, instant: bool) {
        let b = pos.map(f64::from);
        let kind = if instant {
            Kind::InstantSpell
        } else {
            Kind::Spell
        };
        let tint = [16, 8, 0].map(|shift| (rgb >> shift & 255) as f32 / 255.0);
        for _ in 0..100 {
            let d = self.rand() * 4.0;
            let a = self.rand() * std::f64::consts::TAU;
            let m = [a.cos() * d, 0.01 + self.rand() * 0.5, a.sin() * d];
            self.spawn(
                kind,
                [b[0] + m[0] * 0.1, b[1] + 0.3, b[2] + m[2] * 0.1],
                m,
                0,
            );
            let f = 0.75 + self.rand() as f32 * 0.25;
            if let Some(p) = self.list.last_mut() {
                p.color = tint.map(|t| t * f);
                p.vel = p.vel.map(|v| v * f64::from(d as f32));
            }
        }
    }

    /// World effect 2005, `ItemDye.spawnBonemealParticles`: `count` (15 when 0) green sparkles over a block
    /// whose top is `max_y` above its base.
    pub fn bone_meal(&mut self, pos: [i32; 3], count: i32, max_y: f64) {
        let b = pos.map(f64::from);
        for _ in 0..if count == 0 { 15 } else { count } {
            let m = [0, 1, 2].map(|_| self.gaussian() * 0.02);
            let at = [
                b[0] + self.rand(),
                b[1] + self.rand() * max_y,
                b[2] + self.rand(),
            ];
            self.spawn(Kind::HappyVillager, at, m, 0);
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
            Behavior::Suspend => {
                p.pos = [0, 1, 2].map(|i| p.pos[i] + p.vel[i]);
                p.vel = p.vel.map(|v| v * 0.99);
                let alive = p.age < p.max_age;
                p.age += 1;
                alive
            }
            Behavior::Enchant => {
                let f = 1.0 - p.age as f32 / p.max_age as f32;
                let g = (1.0 - f) * (1.0 - f);
                let g = g * g;
                p.pos = [
                    p.origin[0] + p.vel[0] * f64::from(f),
                    p.origin[1] + p.vel[1] * f64::from(f) - f64::from(g * 1.2),
                    p.origin[2] + p.vel[2] * f64::from(f),
                ];
                let alive = p.age < p.max_age;
                p.age += 1;
                alive
            }
            Behavior::DripWater | Behavior::DripLava => tick_drip(p, solid, spawned),
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
        Behavior::Smoke | Behavior::RedDust | Behavior::Crit | Behavior::Snow => {
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
        Behavior::Spell => {
            p.vel[1] += 0.004;
            step(p, solid);
            creep_on_floor(p);
            p.vel = p.vel.map(|v| v * f64::from(0.96_f32));
        }
        Behavior::Snow => {
            p.vel[1] -= 0.03;
            step(p, solid);
            p.vel = p.vel.map(|v| v * f64::from(0.99_f32));
        }
        Behavior::Spark => {
            if p.age > p.max_age / 2 {
                p.alpha = 1.0 - (p.age - p.max_age / 2) as f32 / p.max_age as f32;
            }
            p.vel[1] -= 0.004;
            step(p, solid);
            p.vel = p.vel.map(|v| v * f64::from(0.91_f32));
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

/// `LiquidDripParticle.onUpdate`: it hangs for 40 ticks, then falls; water splashes on the ground,
/// lava stays as a puddle tile. `leakTime` is `40 - age`.
fn tick_drip(
    p: &mut Particle,
    solid: &impl Fn(i32, i32, i32) -> bool,
    spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
) -> bool {
    let water = p.behavior == Behavior::DripWater;
    let age = p.age as i32;
    p.color = if water {
        [0.2, 0.3, 1.0]
    } else {
        [1.0, 16.0 / (age + 16) as f32, 4.0 / (age + 8) as f32]
    };
    p.vel[1] -= p.gravity;
    let tile = if age < 40 {
        p.vel = p.vel.map(|v| v * 0.02);
        113
    } else {
        112
    };
    p.sprite = Sprite::Sheet {
        base: tile,
        anim: false,
    };
    step(p, solid);
    p.vel = p.vel.map(|v| v * f64::from(0.98_f32));
    let mut dead = p.age >= p.max_age;
    p.age += 1;
    if p.on_ground {
        if water {
            dead = true;
            spawned.push((Kind::Splash, p.pos, [0.0; 3]));
        } else {
            p.sprite = Sprite::Sheet {
                base: 114,
                anim: false,
            };
        }
        p.vel[0] *= f64::from(0.7_f32);
        p.vel[2] *= f64::from(0.7_f32);
    }
    // shortcut: only solid full cubes are known here, not the liquid level or the real block top.
    let cell = p.pos.map(f64::floor);
    dead |= solid(cell[0] as i32, cell[1] as i32, cell[2] as i32) && p.pos[1] < cell[1] + 1.0;
    !dead
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
    fn mob_spell_takes_its_colour_from_the_motion_and_ambient_ones_are_faint() {
        let mut ps = Particles::new();
        ps.spawn_packet(15, [0.0; 3], [0.25, 0.5, 0.75], 1.0, 0, &[]);
        ps.spawn_packet(16, [0.0; 3], [0.25, 0.5, 0.75], 1.0, 0, &[]);
        assert_eq!(ps.list[0].color, [0.25, 0.5, 0.75]);
        assert_eq!(ps.list[0].alpha, 1.0);
        assert_eq!(ps.list[1].alpha, 0.15);
    }

    #[test]
    fn swirl_sheds_about_half_the_ticks_and_only_with_a_colour() {
        let mut ps = Particles::new();
        for _ in 0..1000 {
            ps.potion_swirl([0.0; 3], (0.6, 1.8), 0, false, false);
        }
        assert!(ps.is_empty());
        for _ in 0..1000 {
            ps.potion_swirl([0.0; 3], (0.6, 1.8), 0x00FF_8000, false, false);
        }
        assert!((400..600).contains(&ps.len()), "{}", ps.len());
        // Invisible entities shed 1 tick in 15.
        let mut inv = Particles::new();
        for _ in 0..3000 {
            inv.potion_swirl([0.0; 3], (0.6, 1.8), 1, false, true);
        }
        assert!((120..300).contains(&inv.len()), "{}", inv.len());
        let first = &ps.list[0];
        assert!((first.color[1] - 128.0 / 255.0).abs() < 1e-6 && first.color[0] == 1.0);
    }

    #[test]
    fn spell_particle_drifts_up_and_ends_with_its_lifetime() {
        let mut ps = Particles::new();
        ps.spawn(Kind::MobSpell, [0.0, 10.0, 0.0], [1.0, 1.0, 1.0], 0);
        let life = ps.list[0].max_age;
        let y = ps.list[0].pos[1];
        ps.tick(|_, _, _| false);
        assert!(ps.list[0].pos[1] > y);
        for _ in 0..=life {
            ps.tick(|_, _, _| false);
        }
        assert!(ps.is_empty());
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

    #[test]
    fn explosion_bursts_then_puffs_and_smokes_per_block() {
        let mut ps = Particles::new();
        ps.explosion([0.0; 3], 4.0, &[[1, 0, 0], [0, 1, 0], [0, 0, 2]]);
        // One emitter plus an explosion puff and a smoke per block.
        assert_eq!(ps.len(), 1 + 3 * 2);
        assert!(ps.list.iter().any(|p| p.behavior == Behavior::HugeEmitter));
        let mut small = Particles::new();
        small.explosion([0.0; 3], 1.0, &[]);
        assert!(small
            .list
            .iter()
            .all(|p| p.behavior == Behavior::LargeExplosion));
    }

    #[test]
    fn spawner_effect_makes_20_smoke_and_20_flames() {
        let mut ps = Particles::new();
        ps.world_effect(2004, [0, 64, 0], 0);
        assert_eq!(ps.len(), 40);
    }

    #[test]
    fn splash_potion_tints_100_swirls_and_ender_eye_ring_is_portal() {
        let mut ps = Particles::new();
        ps.splash_potion([0, 0, 0], 0xF8_2423, true);
        assert_eq!(ps.len(), 100);
        assert!(ps
            .list
            .iter()
            .all(|p| p.color[0] > p.color[1] && p.color[0] <= 1.0));
        let mut ring = Particles::new();
        ring.world_effect(2003, [0, 0, 0], 0);
        assert_eq!(ring.len(), 2 * 40);
        assert!(ring.list.iter().all(|p| p.behavior == Behavior::Portal));
    }

    #[test]
    fn new_packet_ids_are_mapped_and_spawn() {
        for id in [3, 7, 8, 18, 19, 22, 25, 32] {
            let mut ps = Particles::new();
            ps.spawn_packet(id, [0.0; 3], [0.0; 3], 0.0, 0, &[]);
            assert_eq!(ps.len(), 1, "id {id}");
        }
    }

    #[test]
    fn enchant_glyph_flies_from_offset_back_to_its_origin() {
        let mut ps = Particles::new();
        ps.spawn(Kind::Enchant, [0.0, 0.0, 0.0], [1.0, 2.0, 0.0], 0);
        assert_eq!(ps.list[0].pos, [1.0, 2.0, 0.0]);
        for _ in 0..ps.list[0].max_age {
            ps.tick(|_, _, _| false);
        }
        assert!(ps.list[0].pos[0].abs() < 0.05, "{:?}", ps.list[0].pos);
    }

    #[test]
    fn water_drip_splashes_on_the_ground_and_lava_stays() {
        let floor = |_: i32, y: i32, _: i32| y < 0;
        let mut ps = Particles::new();
        ps.spawn(Kind::DripWater, [0.5, 0.2, 0.5], [0.0; 3], 0);
        for _ in 0..200 {
            ps.tick(floor);
        }
        assert!(ps.list.iter().all(|p| p.behavior != Behavior::DripWater));
        let mut lava = Particles::new();
        lava.spawn(Kind::DripLava, [0.5, 0.2, 0.5], [0.0; 3], 0);
        for _ in 0..60 {
            lava.tick(floor);
        }
        assert!(lava.list.iter().all(|p| p.behavior == Behavior::DripLava));
    }
}
