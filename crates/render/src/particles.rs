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
const BARRIER: i16 = 166;
const SNOWBALL: i16 = 332;
const SLIME_BALL: i16 = 341;
const POTION: i16 = 373;
const ENDER_EYE: i16 = 381;
const FOOTPRINT_LIFE: u32 = 200;

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
    /// `footstep` (28).
    Footstep,
    /// `snowballpoof` (31), `slime` (33), `barrier` (35).
    Snowball,
    Slime,
    Barrier,
    /// `iconcrack_` (36): the packet data is the item id and its damage.
    ItemCrack,
}

impl Kind {
    pub fn from_id(id: i32) -> Option<Kind> {
        const IDS: [(i32, i32, Kind); 38] = [
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
            (28, 28, Kind::Footstep),
            (31, 31, Kind::Snowball),
            (33, 33, Kind::Slime),
            (35, 35, Kind::Barrier),
            (36, 36, Kind::ItemCrack),
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
    /// Texture of item `(id, damage)`; `quarter` picks a 4x4 sub-square like `ItemParticle`, `None` the
    /// whole icon (`BarrierParticle`).
    Item {
        id: i16,
        damage: i16,
        quarter: Option<[f32; 2]>,
    },
    /// `textures/particle/footprint.png`, a flat quad (`FootstepParticle`).
    Footprint,
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
    SuspendWater,
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
    /// Quads of `textures/particle/footprint.png`.
    pub footprint: Vec<Vertex>,
    /// Quads per item `(id, damage)` whose icon texture they sample.
    pub items: Vec<((i16, i16), Vec<Vertex>)>,
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
            .or_else(|| self.spawn_icon(kind, pos, motion))
            .or_else(|| self.spawn_item(kind, pos, motion, data));
        if let Some(p) = made {
            self.push(p);
        }
    }

    /// `ItemParticle`, `BarrierParticle` and `FootstepParticle`. `data` is `id | damage << 16` for item crack.
    fn spawn_item(
        &mut self,
        kind: Kind,
        pos: [f64; 3],
        motion: [f64; 3],
        data: i32,
    ) -> Option<Particle> {
        let item = |id, damage| Sprite::Item {
            id,
            damage,
            quarter: None,
        };
        let mut p = match kind {
            Kind::Footstep => {
                let mut p = self.base(pos, [0.0; 3], Sprite::Footprint);
                // `FootstepParticle.tick` removes it at the 200th tick; the generic rule one tick later.
                p.max_age = FOOTPRINT_LIFE - 1;
                p
            }
            Kind::Barrier => {
                let mut p = self.base(pos, [0.0; 3], item(BARRIER, 0));
                p.scale = 5.0;
                p.max_age = 80;
                p
            }
            Kind::Snowball => self.base(pos, [0.0; 3], item(SNOWBALL, 0)),
            Kind::Slime => self.base(pos, [0.0; 3], item(SLIME_BALL, 0)),
            Kind::ItemCrack => {
                let (id, damage) = ((data & 0xFFFF) as i16, (data >> 16) as i16);
                let mut p = self.base(pos, [0.0; 3], item(id, damage));
                p.vel = [0, 1, 2].map(|i| p.vel[i] * f64::from(0.1_f32) + motion[i]);
                p
            }
            _ => return None,
        };
        match kind {
            Kind::Footstep | Kind::Barrier => {
                p.vel = [0.0; 3];
                p.gravity = 0.0;
            }
            _ => {
                let quarter = [(self.rand() * 3.0) as f32, (self.rand() * 3.0) as f32];
                if let Sprite::Item { id, damage, .. } = p.sprite {
                    p.sprite = Sprite::Item {
                        id,
                        damage,
                        quarter: Some(quarter),
                    };
                }
                p.gravity = 1.0;
                p.scale /= 2.0;
            }
        }
        Some(p)
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
        p.behavior = if kind == Kind::Suspended {
            Behavior::SuspendWater
        } else {
            Behavior::Suspend
        };
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
                self.tick_one(&mut p, &|_, _, _| 0, &mut Vec::new());
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
        let d = data.first().copied().unwrap_or(0);
        let data0 = match kind {
            Kind::BlockCrack => ((d & 0xfff) << 4) | ((d >> 12) & 0xf),
            Kind::ItemCrack => (d & 0xFFFF) | (data.get(1).copied().unwrap_or(0) << 16),
            _ => 0,
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
            2003 => {
                self.item_shards([b[0] + 0.5, b[1], b[2] + 0.5], i32::from(ENDER_EYE));
                self.eye_portal_ring(b);
            }
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

    /// Eight item shards flying out of `at` (`data` is `id | damage << 16`).
    fn item_shards(&mut self, at: [f64; 3], data: i32) {
        for _ in 0..8 {
            let m = [
                self.gaussian() * 0.15,
                self.rand() * 0.2,
                self.gaussian() * 0.15,
            ];
            self.spawn(Kind::ItemCrack, at, m, data);
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
    pub fn splash_potion(&mut self, pos: [i32; 3], damage: i32, rgb: i32, instant: bool) {
        let b = pos.map(f64::from);
        let kind = if instant {
            Kind::InstantSpell
        } else {
            Kind::Spell
        };
        let tint = [16, 8, 0].map(|shift| (rgb >> shift & 255) as f32 / 255.0);
        self.item_shards(b, i32::from(POTION) | (damage << 16));
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

    /// `EmitterParticle.tick`: 16 tries inside a unit sphere around the middle of the entity, each
    /// flying out along its offset (`magic` spawns enchant crits, else crits).
    pub fn hit_burst(&mut self, magic: bool, pos: [f64; 3], (width, height): (f32, f32)) {
        let (w, h) = (f64::from(width), f64::from(height));
        let kind = if magic { Kind::MagicCrit } else { Kind::Crit };
        for _ in 0..16 {
            let d = [0, 1, 2].map(|_| (self.rand() as f32 * 2.0 - 1.0) as f64);
            if d.iter().map(|v| v * v).sum::<f64>() > 1.0 {
                continue;
            }
            let at = [
                pos[0] + d[0] * w / 4.0,
                pos[1] + h / 2.0 + d[1] * h / 4.0,
                pos[2] + d[2] * w / 4.0,
            ];
            self.spawn(kind, at, [d[0], d[1] + 0.2, d[2]], 0);
        }
    }

    /// `EntityLivingBase.onDeathUpdate` at tick 20: 20 explosion puffs inside the hitbox.
    pub fn death_puff(&mut self, pos: [f64; 3], (width, height): (f32, f32)) {
        let (w, h) = (f64::from(width), f64::from(height));
        for _ in 0..20 {
            let m = [0, 1, 2].map(|_| self.gaussian() * 0.02);
            let at = [
                pos[0] + self.rand() * w * 2.0 - w,
                pos[1] + self.rand() * h,
                pos[2] + self.rand() * w * 2.0 - w,
            ];
            self.spawn(Kind::Explode, at, m, 0);
        }
    }

    /// `Entity.doSprintingEffect`: one shard of the block `state` under a sprinting entity,
    /// thrown against its motion `vel`.
    pub fn sprint_dust(&mut self, pos: [f64; 3], vel: [f64; 3], width: f32, state: u16) {
        let w = f64::from(width);
        let at = [
            pos[0] + (self.rand() - 0.5) * w,
            pos[1] + 0.1,
            pos[2] + (self.rand() - 0.5) * w,
        ];
        let m = [-vel[0] * 4.0, 1.5, -vel[2] * 4.0];
        self.spawn(Kind::BlockCrack, at, m, i32::from(state));
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

    /// One 20 Hz tick (`EntityFX.onUpdate`); `block(x, y, z)` is the block state at that cell.
    pub fn tick(&mut self, block: impl Fn(i32, i32, i32) -> u16) {
        let mut list = std::mem::take(&mut self.list);
        let mut spawned = Vec::new();
        list.retain_mut(|p| self.tick_one(p, &block, &mut spawned));
        self.list = list;
        for (kind, pos, motion) in spawned {
            self.spawn(kind, pos, motion, 0);
        }
    }

    /// Ticks one particle; false removes it. `spawned` collects what the particle emits.
    fn tick_one(
        &mut self,
        p: &mut Particle,
        block: &impl Fn(i32, i32, i32) -> u16,
        spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
    ) -> bool {
        p.prev = p.pos;
        if let Some(alive) = self.tick_special(p, block, spawned) {
            return alive;
        }
        if p.age >= p.max_age {
            return false;
        }
        p.age += 1;
        move_generic(p, &|x, y, z| crate::is_solid(block(x, y, z)));
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
        block: &impl Fn(i32, i32, i32) -> u16,
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
            Behavior::Suspend | Behavior::SuspendWater => {
                p.pos = [0, 1, 2].map(|i| p.pos[i] + p.vel[i]);
                p.vel = p.vel.map(|v| v * 0.99);
                let c = p.pos.map(|v| v.floor() as i32);
                let wet = p.behavior == Behavior::Suspend || is_water(block(c[0], c[1], c[2]));
                let alive = wet && p.age < p.max_age;
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
            Behavior::DripWater | Behavior::DripLava => tick_drip(p, block, spawned),
            Behavior::Rain => self.tick_rain(p, block),
            _ => return None,
        })
    }

    /// `RainSplashParticle.onUpdate`: gravity is subtracted whole, and a landed drop dies half the time.
    fn tick_rain(&mut self, p: &mut Particle, block: &impl Fn(i32, i32, i32) -> u16) -> bool {
        let solid = |x, y, z| crate::is_solid(block(x, y, z));
        p.vel[1] -= p.gravity;
        step(p, &solid);
        p.vel = p.vel.map(|v| v * f64::from(0.98_f32));
        let mut dead = p.age >= p.max_age;
        p.age += 1;
        if p.on_ground {
            dead |= self.rand() < 0.5;
            p.vel[0] *= f64::from(0.7_f32);
            p.vel[2] *= f64::from(0.7_f32);
        }
        !(dead || buried(block, p.pos, true))
    }

    /// Camera-facing quads for all live particles; `partial` is the tick fraction in `0..1`. `light` is
    /// `[sky, block]` (`0..=1`) in the block at a position, 0 when its chunk is not loaded (`Renderer::light_at`).
    pub fn vertices(
        &self,
        atlas: &Atlas,
        cam_yaw: f32,
        cam_pitch: f32,
        cam_pos: [f64; 3],
        partial: f32,
        light: impl Fn([f64; 3]) -> [f32; 2],
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
            if let Layer::Footprint = layer {
                push_footprint(&mut out.footprint, p, partial, c, light(p.pos));
                continue;
            }
            let target = match layer {
                Layer::Block => &mut out.block,
                Layer::Sheet => &mut out.sheet,
                Layer::Explosion => &mut out.explosion,
                Layer::Item(key) => item_quads(&mut out.items, key),
                Layer::Footprint => continue,
            };
            push_quad(
                target,
                p,
                render_scale(p, partial),
                c,
                uv,
                right,
                up,
                particle_light(p, partial, &light),
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Layer {
    Block,
    Sheet,
    Explosion,
    Footprint,
    Item((i16, i16)),
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

/// `getLightLevel` of the particle classes as `[sky, block]` in `0..=1` (a packed lightmap coordinate / 240).
fn particle_light(p: &Particle, partial: f32, light: &impl Fn([f64; 3]) -> [f32; 2]) -> [f32; 2] {
    // `Entity.getLightLevel`: the block at eye height, 0.85 of the box height (particle boxes are square).
    let own = || light([p.pos[0], p.pos[1] + p.half * 1.7, p.pos[2]]);
    match (p.behavior, p.sprite) {
        // `FireworksParticles.Spark`: 0xF000F0.
        (Behavior::Spark, _) => [1.0, 1.0],
        // `LargeExplosionParticle` 0xF0F0 and lava `LiquidDripParticle` 257: full block light, no sky light.
        (Behavior::LargeExplosion | Behavior::DripLava, _) => [0.0, 1.0],
        // `PortalParticle`, `EnchantingParticle`: sky light rises by (age/lifetime)^4.
        (Behavior::Portal | Behavior::Enchant, _) => {
            let t = (p.age as f32 / p.max_age.max(1) as f32).powi(4);
            let [s, b] = own();
            [(s + t).min(1.0), b]
        }
        // `FlameParticle` (tile 48): block light rises over its life.
        (_, Sprite::Sheet { base: 48, .. }) => {
            let t = ((p.age as f32 + partial) / p.max_age.max(1) as f32).clamp(0.0, 1.0);
            let [s, b] = own();
            [s, (b + t).min(1.0)]
        }
        // `LavaParticle` (tile 49): full block light.
        (_, Sprite::Sheet { base: 49, .. }) => [own()[0], 1.0],
        // `BlockParticle`: a dark spot takes the light of the source block.
        // shortcut: the source is the spawn cell; vanilla uses the hit block for `dig_hit`, which only differs when it is lit.
        (_, Sprite::Block { .. }) => match own() {
            [0.0, 0.0] => light(p.origin),
            l => l,
        },
        _ => own(),
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
        Sprite::Item {
            id,
            damage,
            quarter,
        } => {
            let (a, b) = match quarter {
                Some([j, k]) => ([j / 4.0, k / 4.0], [(j + 1.0) / 4.0, (k + 1.0) / 4.0]),
                None => ([0.0, 0.0], [1.0, 1.0]),
            };
            (Layer::Item((id, damage)), [a, b])
        }
        Sprite::Footprint => (Layer::Footprint, [[0.0, 0.0], [1.0, 1.0]]),
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

/// The quads that sample item `key`'s icon, appended to its group.
fn item_quads(groups: &mut Vec<((i16, i16), Vec<Vertex>)>, key: (i16, i16)) -> &mut Vec<Vertex> {
    let i = groups.iter().position(|g| g.0 == key).unwrap_or_else(|| {
        groups.push((key, Vec::new()));
        groups.len() - 1
    });
    &mut groups[i].1
}

/// `FootstepParticle.render`: a flat 0.25 block square that fades out over its life, shaded by the light at it
/// (`World.getBrightness`).
fn push_footprint(
    target: &mut Vec<Vertex>,
    p: &Particle,
    partial: f32,
    c: [f32; 3],
    light: [f32; 2],
) {
    let t = (p.age as f32 + partial) / FOOTPRINT_LIFE as f32;
    let alpha = (2.0 - t * t * 2.0).min(1.0) * 0.2;
    let color = [1.0, 1.0, 1.0, alpha];
    for (dx, dz, u, v) in [
        (-1.0, 1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 1.0),
        (1.0, -1.0, 1.0, 0.0),
        (-1.0, -1.0, 0.0, 0.0),
    ] {
        target.push(Vertex {
            pos: [c[0] + dx * 0.125, c[1], c[2] + dz * 0.125],
            uv: [u, v],
            color,
            light,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    target: &mut Vec<Vertex>,
    p: &Particle,
    scale: f32,
    c: [f32; 3],
    uv: [[f32; 2]; 2],
    right: [f32; 3],
    up: [f32; 3],
    light: [f32; 2],
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
            light,
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
    block: &impl Fn(i32, i32, i32) -> u16,
    spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
) -> bool {
    let solid = |x, y, z| crate::is_solid(block(x, y, z));
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
    step(p, &solid);
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
    !(dead || buried(block, p.pos, false))
}

fn is_water(state: u16) -> bool {
    matches!(state >> 4, 8 | 9)
}

/// `Material.isSolid`: full cubes, and shaped blocks except the plant, circuit, snow-layer, carpet,
/// web, fire and portal materials.
fn material_solid(state: u16) -> bool {
    match crate::blocks::look(state).kind {
        crate::blocks::Kind::Opaque | crate::blocks::Kind::Cutout => true,
        crate::blocks::Kind::Shape => !matches!(
            state >> 4,
            6 | 27
                | 28
                | 30
                | 31
                | 32
                | 37
                | 38
                | 39
                | 40
                | 50
                | 51
                | 55
                | 59
                | 65
                | 66
                | 69
                | 75
                | 76
                | 77
                | 78
                | 83
                | 90
                | 104
                | 105
                | 106
                | 111
                | 115
                | 119
                | 127
                | 131
                | 132
                | 93
                | 94
                | 140
                | 141
                | 142
                | 143
                | 144
                | 149
                | 150
                | 157
                | 171
                | 175
                | 198
        ),
        _ => false,
    }
}

/// Whether the cell holding `pos` has a liquid surface or solid top above `pos` (`LiquidBlock.getHeightLoss`;
/// rain splashes use the block's real top, drips the full cell).
fn buried(block: &impl Fn(i32, i32, i32) -> u16, pos: [f64; 3], real_top: bool) -> bool {
    let c = pos.map(|v| v.floor() as i32);
    let state = block(c[0], c[1], c[2]);
    let top = if matches!(state >> 4, 8..=11) {
        let level = state & 15;
        1.0 - f64::from(if level >= 8 { 1 } else { level + 1 }) / 9.0
    } else if !material_solid(state) {
        return false;
    } else if real_top {
        let at = |p: [i32; 3]| block(p[0], p[1], p[2]);
        world::dig::selection_box(&at, c).map_or(1.0, |b| b[4])
    } else {
        1.0
    };
    pos[1] < f64::from(c[1]) + top
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
            ps.tick(|_, y, _| if y < 5 { 16 } else { 0 });
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn hit_burst_spawns_near_the_entity_and_death_puff_makes_20() {
        let mut ps = Particles::new();
        ps.hit_burst(false, [0.0; 3], (0.6, 1.8));
        assert!(!ps.is_empty() && ps.len() <= 16);
        assert!(ps
            .list
            .iter()
            .all(|p| p.pos[0].abs() <= 0.5 && (p.pos[1] - 0.9).abs() <= 1.2));
        let mut ps = Particles::new();
        ps.death_puff([0.0; 3], (0.6, 1.8));
        assert_eq!(ps.len(), 20);
    }

    #[test]
    fn sprint_dust_spawns_one_block_crack_shard_at_the_feet() {
        // Vanilla Entity.doSprintingEffect: one shard within the hitbox width, 0.1 above the feet,
        // thrown against the motion (4x outweighs the +-0.4 start jitter at vx = 1).
        let mut ps = Particles::new();
        ps.sprint_dust([10.0, 64.0, -5.0], [1.0, 0.0, 0.0], 0.6, 0x1234);
        assert_eq!(ps.len(), 1);
        let p = ps.list[0];
        assert!(matches!(p.sprite, Sprite::Block { state: 0x1234, .. }));
        assert!((p.pos[0] - 10.0).abs() <= 0.3 && (p.pos[2] + 5.0).abs() <= 0.3);
        assert!((p.pos[1] - 64.1).abs() < 1e-9);
        assert!(p.vel[0] < 0.0 && p.vel[1] > 0.0);
    }

    #[test]
    fn particles_rest_on_floor() {
        let mut ps = Particles::new();
        ps.spawn(Kind::BlockCrack, [0.5, 6.5, 0.5], [0.0; 3], 16);
        for _ in 0..3 {
            ps.tick(|_, y, _| if y < 5 { 16 } else { 0 });
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
        ps.tick(|_, _, _| 0);
        // t = 0: f = 1, so the particle sits at origin + motion and 1 block above.
        assert_eq!(ps.list[0].pos, [1.0, 1.0, 0.0]);
        for _ in 0..life {
            ps.tick(|_, _, _| 0);
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn huge_explosion_emits_six_large_explosions_for_eight_ticks() {
        let mut ps = Particles::new();
        ps.spawn(Kind::HugeExplode, [0.0; 3], [0.0; 3], 0);
        for _ in 0..8 {
            ps.tick(|_, _, _| 0);
        }
        let large = ps
            .list
            .iter()
            .filter(|p| p.behavior == Behavior::LargeExplosion)
            .count();
        assert!(large > 0 && large <= 48);
        ps.tick(|_, _, _| 0);
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
        ps.tick(|_, _, _| 0);
        assert!(ps.list[0].pos[1] > y);
        for _ in 0..=life {
            ps.tick(|_, _, _| 0);
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
        let smoke = ps
            .list
            .iter()
            .filter(|p| p.behavior == Behavior::Smoke)
            .count();
        let flames = ps
            .list
            .iter()
            .filter(|p| matches!(p.sprite, Sprite::Sheet { base: 48, .. }))
            .count();
        assert_eq!((smoke, flames), (20, 20));
    }

    #[test]
    fn splash_potion_uses_the_instant_or_plain_spell_sprite() {
        let tile = |instant| {
            let mut ps = Particles::new();
            ps.splash_potion([0, 0, 0], 16_421, 0xF8_2423, instant);
            ps.list
                .iter()
                .filter(|p| p.behavior == Behavior::Spell)
                .map(|p| match p.sprite {
                    Sprite::Sheet { base, .. } => base,
                    _ => 0,
                })
                .collect::<Vec<_>>()
        };
        assert!(tile(true).iter().all(|&b| b == 144));
        assert!(tile(false).iter().all(|&b| b == 128));
    }

    #[test]
    fn dispenser_effect_puffs_10_smoke_ahead_of_its_facing() {
        // `data` 5 is facing +x: x offset 1, z offset 0 (WorldRenderer.doEvent case 2000).
        let mut ps = Particles::new();
        ps.world_effect(2000, [0, 64, 0], 5);
        assert_eq!(ps.len(), 10);
        assert!(ps.list.iter().all(|p| p.behavior == Behavior::Smoke));
        // Spawn x is 0.5 + 0.6 + 0.01; the sideways spread acts on z only.
        assert!(ps.list.iter().all(|p| (p.pos[0] - 1.11).abs() < 1e-9));
        assert!(ps.list.iter().all(|p| (0.25..=0.75).contains(&p.pos[2])));
    }

    #[test]
    fn bone_meal_effect_spawns_15_sparkles_by_default_or_data_many_in_the_block() {
        let mut ps = Particles::new();
        ps.bone_meal([2, 5, 3], 0, 0.5);
        assert_eq!(ps.len(), 15);
        let mut ps = Particles::new();
        ps.bone_meal([2, 5, 3], 7, 0.5);
        assert_eq!(ps.len(), 7);
        for p in &ps.list {
            assert!((2.0..=3.0).contains(&p.pos[0]) && (3.0..=4.0).contains(&p.pos[2]));
            assert!((5.0..=5.5).contains(&p.pos[1]), "y {}", p.pos[1]);
            assert!(matches!(
                p.sprite,
                Sprite::Sheet {
                    base: 82,
                    anim: false
                }
            ));
        }
    }

    #[test]
    fn splash_potion_tints_100_swirls_and_ender_eye_ring_is_portal() {
        let mut ps = Particles::new();
        ps.splash_potion([0, 0, 0], 16_421, 0xF8_2423, true);
        let swirls: Vec<_> = ps
            .list
            .iter()
            .filter(|p| p.behavior == Behavior::Spell)
            .collect();
        assert_eq!(swirls.len(), 100);
        assert!(swirls
            .iter()
            .all(|p| p.color[0] > p.color[1] && p.color[0] <= 1.0));
        let mut ring = Particles::new();
        ring.world_effect(2003, [0, 0, 0], 0);
        let portal = |ps: &Particles| {
            ps.list
                .iter()
                .filter(|p| p.behavior == Behavior::Portal)
                .count()
        };
        assert_eq!(portal(&ring), 2 * 40);
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
            ps.tick(|_, _, _| 0);
        }
        assert!(ps.list[0].pos[0].abs() < 0.05, "{:?}", ps.list[0].pos);
    }

    #[test]
    fn water_drip_splashes_on_the_ground_and_lava_stays() {
        let floor = |_: i32, y: i32, _: i32| if y < 0 { 16 } else { 0 };
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

    fn at(cell: [i32; 3], state: u16) -> impl Fn(i32, i32, i32) -> u16 {
        move |x, y, z| if [x, y, z] == cell { state } else { 0 }
    }

    #[test]
    fn suspended_dies_on_the_first_tick_outside_water() {
        let mut ps = Particles::new();
        ps.spawn(Kind::Suspended, [0.5, 0.5, 0.5], [0.0; 3], 0);
        let cell = ps.list[0].pos.map(|v| v.floor() as i32);
        ps.tick(at(cell, 9 << 4));
        assert_eq!(ps.list.len(), 1);
        ps.tick(|_, _, _| 0);
        assert!(ps.is_empty());
        let mut dry = Particles::new();
        dry.spawn(Kind::Suspended, [0.5, 0.5, 0.5], [0.0; 3], 0);
        dry.tick(|_, _, _| 0);
        assert!(dry.is_empty());
    }

    #[test]
    fn drip_dies_below_a_liquid_surface_but_not_above_it() {
        // Water level 7 (height loss 8/9): the surface is 1/9 above the cell floor.
        let water = |y: f64| {
            let mut ps = Particles::new();
            ps.spawn(Kind::DripWater, [0.5, y, 0.5], [0.0; 3], 0);
            ps.tick(at([0, 0, 0], (9 << 4) | 7));
            ps.is_empty()
        };
        assert!(water(0.05));
        assert!(!water(0.5));
    }

    #[test]
    fn drip_dies_inside_a_solid_cell() {
        let mut ps = Particles::new();
        ps.spawn(Kind::DripLava, [0.5, 0.9, 0.5], [0.0; 3], 0);
        ps.tick(at([0, 0, 0], 1 << 4));
        assert!(ps.is_empty());
    }

    #[test]
    fn drip_dies_in_shaped_solid_cells_but_not_in_snow_layers() {
        let mut slab = Particles::new();
        slab.spawn(Kind::DripLava, [0.5, 0.9, 0.5], [0.0; 3], 0);
        slab.tick(at([0, 0, 0], 44 << 4));
        assert!(slab.is_empty());
        let mut snow = Particles::new();
        snow.spawn(Kind::DripLava, [0.5, 0.9, 0.5], [0.0; 3], 0);
        snow.tick(at([0, 0, 0], 78 << 4));
        assert!(!snow.is_empty());
    }

    fn item_of(p: &Particle) -> Option<(i16, i16)> {
        match p.sprite {
            Sprite::Item { id, damage, .. } => Some((id, damage)),
            _ => None,
        }
    }

    #[test]
    fn splash_potion_and_ender_eye_shed_eight_item_shards_each() {
        let mut potion = Particles::new();
        potion.splash_potion([0, 0, 0], 16_421, 0xF8_2423, false);
        let shards =
            |ps: &Particles, item| ps.list.iter().filter(|p| item_of(p) == Some(item)).count();
        assert_eq!(shards(&potion, (POTION, 16_421)), 8);
        let mut eye = Particles::new();
        eye.world_effect(2003, [0, 0, 0], 0);
        assert_eq!(shards(&eye, (ENDER_EYE, 0)), 8);
    }

    #[test]
    fn item_crack_packet_reads_the_item_and_adds_the_motion_to_a_tenth_of_the_random_start() {
        let mut ps = Particles::new();
        ps.spawn_packet(36, [0.0; 3], [0.0, 1.0, 0.0], 1.0, 0, &[332, 0]);
        let p = &ps.list[0];
        assert_eq!(item_of(p), Some((332, 0)));
        // base speed <= 0.4 * 0.1, plus the 1.0 upward motion.
        assert!(p.vel[1] > 0.9 && p.vel[1] <= 1.05);
        assert_eq!(p.gravity, 1.0);
    }

    #[test]
    fn snowball_slime_and_barrier_use_their_item_icons() {
        let icon = |id| {
            let mut ps = Particles::new();
            ps.spawn_packet(id, [0.0; 3], [0.0; 3], 0.0, 0, &[]);
            (item_of(&ps.list[0]), ps.list[0].max_age)
        };
        assert_eq!(icon(31).0, Some((SNOWBALL, 0)));
        assert_eq!(icon(33).0, Some((SLIME_BALL, 0)));
        // The barrier hovers still for 80 ticks as a whole block-sized icon.
        assert_eq!(icon(35), (Some((BARRIER, 0)), 80));
    }

    #[test]
    fn footprint_stays_flat_for_200_ticks_and_fades() {
        let mut ps = Particles::new();
        ps.spawn_packet(28, [0.0, 5.0, 0.0], [0.0; 3], 0.0, 0, &[]);
        let floor = |_, _, _| 0_u16;
        for _ in 0..199 {
            ps.tick(floor);
        }
        assert_eq!(ps.len(), 1);
        assert_eq!(ps.list[0].pos, [0.0, 5.0, 0.0]);
        let atlas = crate::assets::Atlas::flat();
        let fresh = ps
            .vertices(&atlas, 0.0, 0.0, [0.0; 3], 0.0, |_| [1.0; 2])
            .footprint;
        assert_eq!(fresh.len(), 4);
        assert!(fresh.iter().all(|v| v.color[3] < 0.05 && v.pos[1] == 5.0));
        ps.tick(floor);
        assert!(ps.is_empty());
    }

    #[test]
    fn particles_take_the_world_light_at_eye_height_like_vanilla() {
        // Dark below y 6, sky 0.5 and block 0.25 from y 6 up.
        let light = |p: [f64; 3]| if p[1] < 6.0 { [0.0; 2] } else { [0.5, 0.25] };
        let atlas = crate::assets::Atlas::flat();
        let lights = |ps: &Particles| {
            let q = ps.vertices(&atlas, 0.0, 0.0, [0.0; 3], 0.0, light);
            let mut v: Vec<[f32; 2]> = [q.sheet, q.block, q.explosion]
                .into_iter()
                .flatten()
                .chain(q.items.into_iter().flat_map(|g| g.1))
                .map(|v| v.light)
                .collect();
            v.dedup();
            v
        };
        let one = |id, y, data: &[i32]| {
            let mut ps = Particles::new();
            ps.spawn_packet(id, [0.0, y, 0.0], [0.0; 3], 0.0, 0, data);
            ps
        };
        // Slime at y 5.9: eye height 0.17 reaches the lit block; at 5.8 it stays dark.
        assert_eq!(lights(&one(33, 5.9, &[])), [[0.5, 0.25]]);
        assert_eq!(lights(&one(33, 5.8, &[])), [[0.0, 0.0]]);
        // Lava: full block light, world sky light.
        assert_eq!(lights(&one(27, 6.0, &[])), [[0.5, 1.0]]);
        // Firework spark: full bright even in the dark.
        assert_eq!(lights(&one(3, 0.0, &[])), [[1.0, 1.0]]);
        // Large explosion: block light only.
        assert_eq!(lights(&one(1, 6.0, &[])), [[0.0, 1.0]]);
        // Flame and portal brighten with age: block light for the flame, sky light for the portal.
        let mut flame = one(26, 6.0, &[]);
        flame.list[0].age = flame.list[0].max_age;
        assert_eq!(lights(&flame), [[0.5, 1.0]]);
        let mut portal = one(24, 6.0, &[]);
        (portal.list[0].age, portal.list[0].max_age) = (20, 40);
        assert_eq!(lights(&portal), [[0.5 + 0.0625, 0.25]]);
        // Block crack in a dark spot takes the light of its source cell.
        let mut crack = one(37, 6.0, &[1]);
        crack.list[0].pos[1] = 0.0;
        assert_eq!(lights(&crack), [[0.5, 0.25]]);
    }
}
