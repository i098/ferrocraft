//! Particle classes of 1.8.9 that differ from `EntityFX` in more than a texture: `EntityFlameFX`, `EntityLavaFX`,
//! `EntityNoteFX`, `EntityHeartFX` (also the angry villager), `EntityBubbleFX` and `EntityFishWakeFX`.

use super::{creep_on_floor, is_water, step, Behavior, Kind, Particle, Particles, Sprite};

/// `(double) 0.01F`: the factor of the `EntityFlameFX`, `EntityNoteFX` and `EntityHeartFX` constructors.
const SMALL: f64 = 0.01_f32 as f64;

const fn sheet(base: u32) -> Sprite {
    Sprite::Sheet { base, anim: false }
}

/// `EntityNoteFX` colour for the packet motion x (`note / 24`): `sin((x + k / 3) * 2 pi) * 0.65 + 0.35`
/// per channel. GL clamps what is out of range.
pub(super) fn note_color(x: f64) -> [f32; 3] {
    [0.0, 0.333_333_34, 0.666_666_7].map(|k| {
        let angle = (x as f32 + k) * std::f32::consts::PI * 2.0;
        (world::sin(angle) * 0.65 + 0.35).clamp(0.0, 1.0)
    })
}

/// One tick of motion: `EntityFlameFX` ignores blocks; `EntityNoteFX` and `EntityHeartFX` collide and creep on a floor.
pub(super) fn move_fx(p: &mut Particle, solid: &impl Fn(i32, i32, i32) -> bool) {
    if p.behavior == Behavior::Drift {
        step(p, solid);
        creep_on_floor(p);
    } else {
        p.pos = [0, 1, 2].map(|i| p.pos[i] + p.vel[i]);
    }
    p.vel = p.vel.map(|v| v * p.drag);
}

impl Particles {
    /// The three `drift` particles by kind: hearts, the angry villager (half a block up) and notes (pitch colour).
    pub(super) fn drift_icon(&mut self, kind: Kind, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        match kind {
            Kind::Heart => self.drift(pos, 80, 0.1, 0.86, 16),
            // `AngryVillagerFactory` spawns half a block above the given place.
            Kind::AngryVillager => self.drift([pos[0], pos[1] + 0.5, pos[2]], 81, 0.1, 0.86, 16),
            _ => {
                let mut p = self.drift(pos, 64, 0.2, 0.66, 6);
                p.color = note_color(motion[0]);
                p
            }
        }
    }

    /// Lava, bubble and wake ticks.
    pub(super) fn tick_fx(
        &mut self,
        p: &mut Particle,
        block: &impl Fn(i32, i32, i32) -> u16,
        spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
    ) -> bool {
        match p.behavior {
            Behavior::Lava => self.tick_lava(p, block, spawned),
            Behavior::Bubble => self.tick_bubble(p, block),
            _ => self.tick_wake(p, block),
        }
    }

    /// `EntityFlameFX`: motion `base * 0.01 + packet`, each axis moved by `(rand - rand) * 0.05`.
    pub(super) fn flame(&mut self, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sheet(48));
        p.vel = [0, 1, 2].map(|i| p.vel[i] * SMALL + motion[i]);
        for c in &mut p.pos {
            *c += f64::from((self.rand() as f32 - self.rand() as f32) * 0.05);
        }
        p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32 + 4;
        p.drag = f64::from(0.96_f32);
        p.behavior = Behavior::Flame;
        p
    }

    /// `EntityLavaFX`: horizontal start motion `base * 0.8`, rise `rand * 0.4 + 0.05`, size `base * (rand * 2 + 0.2)`.
    pub(super) fn lava(&mut self, pos: [f64; 3]) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sheet(49));
        let keep = f64::from(0.8_f32);
        p.vel = [
            p.vel[0] * keep,
            f64::from(self.rand() as f32 * 0.4 + 0.05),
            p.vel[2] * keep,
        ];
        p.scale *= (self.rand() * 2.0 + 0.2) as f32;
        p.init_scale = p.scale;
        p.max_age = (16.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = Behavior::Lava;
        p
    }

    /// `EntityNoteFX` (tile 64, rise 0.2, drag 0.66, 6 ticks) and `EntityHeartFX` (tile 80 or 81, rise 0.1, drag
    /// 0.86, 16 ticks): motion `base * 0.01` plus a rise, size `base * 0.75 * 2`.
    pub(super) fn drift(
        &mut self,
        pos: [f64; 3],
        tile: u32,
        rise: f64,
        drag: f32,
        max_age: u32,
    ) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sheet(tile));
        p.vel = [p.vel[0] * SMALL, p.vel[1] * SMALL + rise, p.vel[2] * SMALL];
        p.scale *= 0.75;
        p.scale *= 2.0;
        p.init_scale = p.scale;
        p.max_age = max_age;
        p.drag = f64::from(drag);
        p.behavior = Behavior::Drift;
        p
    }

    /// `EntityBubbleFX`: motion `packet * 0.2 + (rand * 2 - 1) * 0.02`, size `base * (rand * 0.6 + 0.2)`, box 0.02.
    pub(super) fn bubble(&mut self, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sheet(32));
        p.half = 0.01;
        p.scale *= (self.rand() * 0.6 + 0.2) as f32;
        p.init_scale = p.scale;
        p.vel = [0, 1, 2].map(|i| {
            motion[i] * f64::from(0.2_f32) + (self.rand() * 2.0 - 1.0) * f64::from(0.02_f32)
        });
        p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = Behavior::Bubble;
        p
    }

    /// `EntityFishWakeFX`: the packet motion exactly (the constructor sets its own, then overwrites it), no gravity.
    pub(super) fn wake(&mut self, pos: [f64; 3], motion: [f64; 3]) -> Particle {
        let mut p = self.base(pos, [0.0; 3], sheet(19));
        p.vel = motion;
        p.half = 0.005;
        p.max_age = (8.0 / (self.rand() * 0.8 + 0.2)) as u32;
        p.behavior = Behavior::Wake;
        p
    }

    /// `EntityLavaFX.onUpdate`: with probability `1 - age / max_age` a smoke puff with the pop's own motion,
    /// then gravity 0.03 and drag 0.999.
    pub(super) fn tick_lava(
        &mut self,
        p: &mut Particle,
        block: &impl Fn(i32, i32, i32) -> u16,
        spawned: &mut Vec<(Kind, [f64; 3], [f64; 3])>,
    ) -> bool {
        let alive = p.age < p.max_age;
        p.age += 1;
        if self.rand() as f32 > p.age as f32 / p.max_age as f32 {
            spawned.push((Kind::Smoke, p.pos, p.vel));
        }
        p.vel[1] -= 0.03;
        step(p, &|x, y, z| crate::is_solid(block(x, y, z)));
        p.vel = p.vel.map(|v| v * f64::from(0.999_f32));
        if p.on_ground {
            p.vel[0] *= f64::from(0.7_f32);
            p.vel[2] *= f64::from(0.7_f32);
        }
        alive
    }

    /// `EntityBubbleFX.onUpdate`: it rises by 0.002 a tick, drag 0.85, and dies where the block is not water.
    pub(super) fn tick_bubble(
        &mut self,
        p: &mut Particle,
        block: &impl Fn(i32, i32, i32) -> u16,
    ) -> bool {
        p.vel[1] += 0.002;
        step(p, &|x, y, z| crate::is_solid(block(x, y, z)));
        p.vel = p.vel.map(|v| v * f64::from(0.85_f32));
        let c = p.pos.map(|v| v.floor() as i32);
        let wet = is_water(block(c[0], c[1], c[2]));
        let alive = p.age < p.max_age;
        p.age += 1;
        wet && alive
    }

    /// `EntityFishWakeFX.onUpdate`: drag 0.98; the box grows by `0.001` per tick of life and the tile turns
    /// through `19 + (60 - left) % 4`, `left` being the ticks to live before this one.
    pub(super) fn tick_wake(
        &mut self,
        p: &mut Particle,
        block: &impl Fn(i32, i32, i32) -> u16,
    ) -> bool {
        step(p, &|x, y, z| crate::is_solid(block(x, y, z)));
        p.vel = p.vel.map(|v| v * f64::from(0.98_f32));
        let lived = 60 - (p.max_age as i32 - p.age as i32);
        p.half = f64::from(lived as f32 * 0.001) / 2.0;
        p.sprite = sheet(19 + lived.rem_euclid(4) as u32);
        let alive = p.age < p.max_age;
        p.age += 1;
        alive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_alone(ps: &mut Particles, kind: Kind, wet: bool) -> Vec<Particle> {
        ps.spawn(kind, [0.5, 70.5, 0.5], [0.0; 3], 0);
        ps.tick(|_, _, _| if wet { 9 << 4 } else { 0 });
        ps.list.clone()
    }

    #[test]
    fn flame_lives_14_to_44_ticks_and_starts_up_to_twice_the_size() {
        let mut ps = Particles::new();
        for _ in 0..2000 {
            ps.spawn(Kind::Flame, [0.0; 3], [0.0; 3], 0);
        }
        let (min, max) = ps.list.iter().fold((u32::MAX, 0), |(lo, hi), p| {
            (lo.min(p.max_age), hi.max(p.max_age))
        });
        assert!(min >= 12 && max <= 44, "{min}..{max}");
        assert!(ps.list.iter().all(|p| (1.0..=2.0).contains(&p.init_scale)));
        // `scale = start * (1 - (age / max_age)^2 * 0.5)`: full at birth, half at the end.
        let p = ps.list[0];
        assert_eq!(super::super::render_scale(&p, 0.0), p.init_scale);
        let end = Particle {
            age: p.max_age,
            ..p
        };
        assert!((super::super::render_scale(&end, 0.0) - p.init_scale * 0.5).abs() < 1e-6);
    }

    #[test]
    fn flame_starts_within_a_twentieth_of_a_block_and_ignores_blocks() {
        let mut ps = Particles::new();
        for _ in 0..200 {
            ps.spawn(Kind::Flame, [5.0, 5.0, 5.0], [0.0; 3], 0);
        }
        assert!(ps
            .list
            .iter()
            .all(|p| (0..3).all(|i| (p.pos[i] - 5.0).abs() <= 0.05)));
        let mut p = ps.list[0];
        p.vel = [0.0, -0.5, 0.0];
        let at = p.pos;
        super::move_fx(&mut p, &|_, _, _| true);
        assert!((p.pos[1] - (at[1] - 0.5)).abs() < 1e-12);
    }

    #[test]
    fn lava_pop_shrinks_to_nothing_and_leaves_smoke() {
        let mut ps = Particles::new();
        ps.spawn(Kind::Lava, [0.5, 70.5, 0.5], [0.0; 3], 0);
        let pop = ps.list[0];
        assert!((pop.vel[1] - 0.05).abs() <= 0.4 && pop.vel[1] >= 0.05);
        let end = Particle {
            age: pop.max_age,
            ..pop
        };
        assert!(super::super::render_scale(&end, 0.0) <= 0.0);
        let mut smoke = 0;
        for _ in 0..=pop.max_age {
            smoke += ps
                .list
                .iter()
                .filter(|p| matches!(p.behavior, Behavior::Smoke))
                .count();
            ps.tick(|_, _, _| 0);
        }
        // The chance of a puff each tick is `1 - age / max_age`, about half the ticks on average.
        assert!(smoke >= 1, "no smoke");
        assert!(ps.list.iter().all(|p| p.behavior != Behavior::Lava));
    }

    #[test]
    fn note_color_follows_the_pitch() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        assert!(close(note_color(0.0), [0.35, 0.9129, 0.0]));
        assert!(close(note_color(0.5), [0.35, 0.0, 0.9129]));
    }

    #[test]
    fn note_and_hearts_grow_in_at_vanilla_size_and_the_angry_villager_sits_half_a_block_up() {
        let mut ps = Particles::new();
        ps.spawn(Kind::Note, [0.0, 10.0, 0.0], [0.5, 0.0, 0.0], 0);
        ps.spawn(Kind::Heart, [0.0, 10.0, 0.0], [0.0; 3], 0);
        ps.spawn(Kind::AngryVillager, [0.0, 10.0, 0.0], [0.0; 3], 0);
        let [note, heart, angry] = [0, 1, 2].map(|i| ps.list[i]);
        assert_eq!(note.color, note_color(0.5));
        assert!((1.5..=3.0).contains(&note.init_scale));
        assert_eq!(super::super::render_scale(&note, 0.0), 0.0);
        let grown = Particle { age: 1, ..heart };
        assert_eq!(super::super::render_scale(&grown, 0.0), heart.init_scale);
        assert_eq!(heart.pos[1], 10.0);
        assert_eq!(angry.pos[1], 10.5);
    }

    #[test]
    fn bubble_with_no_packet_motion_drifts_by_at_most_0_02_and_dies_outside_water() {
        let mut ps = Particles::new();
        for _ in 0..100 {
            ps.spawn(Kind::Bubble, [0.5, 70.5, 0.5], [0.0; 3], 0);
        }
        assert!(ps
            .list
            .iter()
            .all(|p| p.vel.iter().all(|v| v.abs() <= 0.02 + 1e-9)));
        assert_eq!(
            tick_alone(&mut Particles::new(), Kind::Bubble, true).len(),
            1
        );
        assert!(tick_alone(&mut Particles::new(), Kind::Bubble, false).is_empty());
        // Packet motion adds a fifth of itself.
        let mut ps = Particles::new();
        ps.spawn(Kind::Bubble, [0.0; 3], [1.0, 0.0, 0.0], 0);
        assert!((ps.list[0].vel[0] - 0.2).abs() <= 0.02 + 1e-9);
    }

    #[test]
    fn id_6_is_a_wake_that_follows_the_packet_motion_without_gravity() {
        assert_eq!(Kind::from_id(6), Some(Kind::Wake));
        assert_eq!(Kind::from_id(5), Some(Kind::Splash));
        let mut ps = Particles::new();
        ps.spawn(Kind::Wake, [0.5, 70.5, 0.5], [0.1, 0.0, -0.1], 0);
        let first = ps.list[0];
        assert_eq!(first.vel, [0.1, 0.0, -0.1]);
        assert_eq!(first.gravity, 0.0);
        ps.tick(|_, _, _| 0);
        let p = ps.list[0];
        assert!((p.pos[0] - 0.6).abs() < 1e-9 && (p.pos[1] - 70.5).abs() < 1e-9);
        assert!((p.vel[0] - 0.1 * f64::from(0.98_f32)).abs() < 1e-9);
        // The tile cycles with the time left: `19 + (60 - left) % 4`.
        let Sprite::Sheet { base, .. } = p.sprite else {
            panic!("sheet sprite");
        };
        assert_eq!(
            base,
            19 + (60 - (first.max_age as i32)).rem_euclid(4) as u32
        );
    }
}
