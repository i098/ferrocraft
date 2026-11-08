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
    /// `blockcrack`/`blockdust`: state id (`id << 4 | meta`) in the packet data.
    BlockCrack,
}

impl Kind {
    pub fn from_id(id: i32) -> Option<Kind> {
        const IDS: [(i32, i32, Kind); 21] = [
            (0, 0, Kind::Explode),
            (2, 2, Kind::Explode),
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
}

#[derive(Default)]
pub struct Particles {
    list: Vec<Particle>,
    rng: u64,
}

/// Quads split by texture: `block` samples the block atlas, `sheet` samples `particles.png`.
#[derive(Default)]
pub struct Quads {
    pub block: Vec<Vertex>,
    pub sheet: Vec<Vertex>,
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
        Particle {
            pos,
            prev: pos,
            vel,
            color: [1.0; 3],
            alpha: 1.0,
            scale: ((self.rand() * 0.5 + 0.5) * 2.0) as f32,
            gravity: 0.0,
            drag: 0.98,
            age: 0,
            max_age: (4.0 / (self.rand() * 0.9 + 0.1)) as u32,
            on_ground: false,
            sprite,
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
        if let Some(mut p) = made {
            p.prev = p.pos;
            self.push(p);
        }
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
                p = self.base(pos, [0.0; 3], sheet(0, true));
                p.vel = [
                    motion[0] + (self.rand() * 2.0 - 1.0) * 0.001,
                    motion[1] + 0.1,
                    motion[2],
                ];
                let g = (self.rand() * 0.3) as f32;
                p.color = [g; 3];
                p.scale *= if kind == Kind::LargeSmoke { 2.5 } else { 0.75 };
                p.max_age = ((8.0 / (self.rand() * 0.8 + 0.2)) as u32).max(1);
                p.gravity = -0.0;
                p.drag = 0.96;
            }
            Kind::Explode | Kind::LargeExplode => {
                p = self.base(pos, motion, sheet(0, true));
                p.vel = motion;
                p.scale = if kind == Kind::LargeExplode { 5.0 } else { 2.0 };
                p.max_age = 6 + (self.rand() * 4.0) as u32;
                p.drag = 0.9;
                p.gravity = -0.1;
                let g = (self.rand() * 0.3 + 0.7) as f32;
                p.color = [g; 3];
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
            Kind::Splash | Kind::Droplet | Kind::Bubble => {
                let base = if kind == Kind::Bubble { 32 } else { 19 };
                p = self.base(pos, motion, sheet(base, false));
                p.gravity = if kind == Kind::Bubble { -0.02 } else { 0.04 };
                p.drag = if kind == Kind::Bubble { 0.85 } else { 0.98 };
                p.scale *= 0.5;
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
            }
            Kind::Cloud => {
                p = self.base(pos, motion, sheet(0, true));
                p.color = [1.0; 3];
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.3)) as u32;
            }
            Kind::RedDust => {
                p = self.base(pos, motion, sheet(0, true));
                p.color = [1.0, 0.0, 0.0];
                p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
                p.scale *= 0.8;
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
                p = self.base(pos, [0.0; 3], sheet(65, false));
                p.vel = [motion[0] * 0.4, motion[1] * 0.4 + 0.2, motion[2] * 0.4];
                p.max_age = 20;
                p.drag = 0.7;
                p.gravity = 0.5;
                if kind == Kind::MagicCrit {
                    p.color = [0.3, 0.8, 1.0];
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
                p = self.base(pos, motion, sheet(160, false));
                p.color = [0.9, 0.3, 1.0];
                p.max_age = (4.0 / (self.rand() * 0.9 + 0.1)) as u32 + 40;
                p.drag = 1.0;
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
            data.first().copied().unwrap_or(0)
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
    pub fn dig_hit(&mut self, [x, y, z]: [i32; 3], face: u8, state: u16) {
        let r = |s: &mut Self| s.rand();
        let mut c = [
            f64::from(x) + r(self) * 0.8 + 0.1,
            f64::from(y) + r(self) * 0.8 + 0.1,
            f64::from(z) + r(self) * 0.8 + 0.1,
        ];
        let e = 0.1;
        match face {
            0 => c[1] = f64::from(y) - e,
            1 => c[1] = f64::from(y) + 1.0 + e,
            2 => c[2] = f64::from(z) - e,
            3 => c[2] = f64::from(z) + 1.0 + e,
            4 => c[0] = f64::from(x) - e,
            _ => c[0] = f64::from(x) + 1.0 + e,
        }
        self.spawn(Kind::BlockCrack, c, [0.0; 3], i32::from(state));
        if let Some(p) = self.list.last_mut() {
            p.vel = p.vel.map(|v| v * 0.6);
            p.scale *= 0.6;
        }
    }

    /// One 20 Hz tick (`EntityFX.onUpdate`); `solid(x, y, z)` reports blocks that stop particles.
    pub fn tick(&mut self, solid: impl Fn(i32, i32, i32) -> bool) {
        self.list.retain_mut(|p| {
            p.prev = p.pos;
            if p.age >= p.max_age {
                return false;
            }
            p.age += 1;
            p.vel[1] -= 0.04 * p.gravity;
            let want = p.vel;
            for axis in [1, 0, 2] {
                let mut next = p.pos;
                next[axis] += want[axis];
                if blocked(&solid, next) {
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
            p.vel = p.vel.map(|v| v * p.drag);
            if p.on_ground {
                p.vel[0] *= 0.7;
                p.vel[2] *= 0.7;
            }
            true
        });
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
            let t = f64::from(partial);
            let c = [0, 1, 2].map(|i| (p.prev[i] + (p.pos[i] - p.prev[i]) * t - cam_pos[i]) as f32);
            let (is_block, uv) = sprite_uv(p, atlas);
            let target = if is_block {
                &mut out.block
            } else {
                &mut out.sheet
            };
            push_quad(target, p, c, uv, right, up);
        }
        out
    }
}

/// Whether the sprite samples the block atlas, and its two corner uvs.
fn sprite_uv(p: &Particle, atlas: &Atlas) -> (bool, [[f32; 2]; 2]) {
    match p.sprite {
        Sprite::Block { state, jitter } => {
            let name = crate::blocks::look(state).tex[crate::blocks::NORTH];
            let a = atlas.uv(name, [jitter[0] / 4.0, jitter[1] / 4.0]);
            let b = atlas.uv(name, [(jitter[0] + 1.0) / 4.0, (jitter[1] + 1.0) / 4.0]);
            (true, [a, b])
        }
        Sprite::Sheet { base, anim } => {
            let frame = 7u32.saturating_sub(p.age * 8 / p.max_age.max(1));
            let tile = base + if anim { frame } else { 0 };
            let (u, v) = ((tile % 16) as f32 / 16.0, (tile / 16) as f32 / 16.0);
            (false, [[u, v], [u + 1.0 / 16.0, v + 1.0 / 16.0]])
        }
    }
}

fn push_quad(
    target: &mut Vec<Vertex>,
    p: &Particle,
    c: [f32; 3],
    uv: [[f32; 2]; 2],
    right: [f32; 3],
    up: [f32; 3],
) {
    let half = 0.1 * p.scale;
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

/// Whether a 0.2-wide particle box centred on `pos` touches a solid block (checked by its eight corners).
fn blocked(solid: &impl Fn(i32, i32, i32) -> bool, pos: [f64; 3]) -> bool {
    let h = 0.1;
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
}
