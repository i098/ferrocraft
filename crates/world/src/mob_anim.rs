//! Timers that entity status bytes start: iron golem arms (4, 11) and wolf shake (8).

const GOLEM: u8 = 99;
const WOLF: u8 = 95;
const RABBIT: u8 = 101;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MobAnim {
    /// Iron golem `attackTimer`: 10 at status 4, counts down every tick.
    pub golem_attack: i32,
    /// Iron golem `holdRoseTick`: 400 at status 11, counts down every tick.
    pub golem_rose: i32,
    /// Rabbit `jumpTicks` and `jumpDuration`: the jump runs 10 ticks from status 1.
    pub jump_ticks: i32,
    pub jump_duration: i32,
    /// Wolf `isShaking`.
    pub shaking: bool,
    /// Wolf `timeWolfIsShaking`.
    pub shake_time: f32,
    /// Wolf `prevTimeWolfIsShaking`.
    pub prev_shake_time: f32,
    /// Wolf `isWet`: set while the wolf is in water, cleared when a shake ends.
    pub wet: bool,
    /// Input of the tick: `Entity.isWet()` of the wolf, set by `Entities::update_wet`.
    pub in_water: bool,
    /// Input of the tick: `EntityWolf.isBegging` (metadata byte 19).
    pub begging: bool,
    /// Wolf `headRotationCourse` and `headRotationCourseOld`.
    pub begging_course: f32,
    pub prev_begging_course: f32,
    /// Horse eating, rearing and mouth animation.
    pub horse: crate::horse::HorseAnim,
    /// Guardian tail wag and spike extension.
    pub guardian: Guardian,
}

impl MobAnim {
    /// `EntityIronGolem` and `EntityWolf` `handleStatusUpdate`.
    pub fn set_status(&mut self, mob: u8, status: i8) {
        match (mob, status) {
            (GOLEM, 4) => self.golem_attack = 10,
            (GOLEM, 11) => self.golem_rose = 400,
            (RABBIT, 1) => {
                self.jump_duration = 10;
                self.jump_ticks = 0;
            }
            (WOLF, 8) => {
                self.shaking = true;
                self.shake_time = 0.0;
                self.prev_shake_time = 0.0;
            }
            _ => {}
        }
    }

    /// `EntityIronGolem.onLivingUpdate` and the shake step of `EntityWolf.onUpdate`.
    pub fn tick(&mut self) {
        self.golem_attack = (self.golem_attack - 1).max(0);
        self.golem_rose = (self.golem_rose - 1).max(0);
        // `EntityRabbit.onLivingUpdate`: count the jump up, then end it.
        if self.jump_ticks != self.jump_duration {
            self.jump_ticks += 1;
        } else if self.jump_duration != 0 {
            self.jump_ticks = 0;
            self.jump_duration = 0;
        }
        self.tick_wolf();
    }

    /// The wolf part of `EntityWolf.onUpdate`: the begging ease, wetness and the shake timer.
    fn tick_wolf(&mut self) {
        self.prev_begging_course = self.begging_course;
        let target = if self.begging { 1.0 } else { 0.0 };
        self.begging_course += (target - self.begging_course) * 0.4;
        if self.in_water {
            self.wet = true;
            self.shaking = false;
            self.shake_time = 0.0;
            self.prev_shake_time = 0.0;
        } else if self.shaking {
            self.prev_shake_time = self.shake_time;
            self.shake_time += 0.05;
            if self.prev_shake_time >= 2.0 {
                self.wet = false;
                self.shaking = false;
                self.prev_shake_time = 0.0;
                self.shake_time = 0.0;
            }
        }
    }

    /// `EntityRabbit.getJumpCompletion`.
    pub fn jump_completion(&self, partial: f32) -> f32 {
        if self.jump_duration == 0 {
            0.0
        } else {
            (self.jump_ticks as f32 + partial) / self.jump_duration as f32
        }
    }

    /// `EntityWolf.getShadingWhileWet`, `None` for a dry wolf (`isWolfWet`).
    pub fn wet_shading(&self, partial: f32) -> Option<f32> {
        self.wet.then(|| {
            0.75 + (self.prev_shake_time + (self.shake_time - self.prev_shake_time) * partial) / 2.0
                * 0.25
        })
    }

    /// `EntityWolf.getInterestedAngle`: the head roll of a begging wolf, in radians.
    pub fn interested_angle(&self, partial: f32) -> f32 {
        (self.prev_begging_course + (self.begging_course - self.prev_begging_course) * partial)
            * 0.15
            * std::f32::consts::PI
    }

    /// `EntityWolf.getShakeAngle`: roll in radians; `ModelWolf` passes offset -0.08 (mane), -0.16 (body), -0.2 (tail).
    pub fn shake_angle(&self, partial: f32, offset: f32) -> f32 {
        let f =
            ((self.prev_shake_time + (self.shake_time - self.prev_shake_time) * partial + offset)
                / 1.8)
                .clamp(0.0, 1.0);
        (f * std::f32::consts::PI).sin()
            * (f * std::f32::consts::PI * 11.0).sin()
            * 0.15
            * std::f32::consts::PI
    }
}

/// `Random.nextFloat` of the 48-bit LCG; the guardian's unseeded `random` is not observable, so any seed works.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Lcg(i64);

impl Lcg {
    fn new(seed: i64) -> Lcg {
        Lcg((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next_float(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> 24) as f32 / (1 << 24) as f32
    }
}

/// Metadata index 16 bit of `GuardianEntity.isMoving`: the spikes are out.
pub const GUARDIAN_MOVING: i32 = 2;

/// The client part of `GuardianEntity.mobTick` that drives the model: `tailAngle`, `spikesExtension` and
/// the rate that turns the tail. The beam, bubbles and flop sound are not ported.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Guardian {
    tail: f32,
    prev_tail: f32,
    rate: f32,
    spikes: f32,
    prev_spikes: f32,
    rng: Lcg,
}

impl Guardian {
    /// The constructor draws the tail phase from `random`.
    pub fn new(seed: i64) -> Guardian {
        let mut rng = Lcg::new(seed);
        let tail = rng.next_float();
        Guardian {
            tail,
            prev_tail: tail,
            rng,
            ..Guardian::default()
        }
    }

    /// One client tick, given `isInWater` and `isMoving` (bit 2 of metadata 16).
    pub fn tick(&mut self, in_water: bool, moving: bool) {
        self.prev_tail = self.tail;
        if !in_water {
            self.rate = 2.0;
        } else if moving {
            if self.rate < 0.5 {
                self.rate = 4.0;
            } else {
                self.rate += (0.5 - self.rate) * 0.1;
            }
        } else {
            self.rate += (0.125 - self.rate) * 0.2;
        }
        self.tail += self.rate;
        self.prev_spikes = self.spikes;
        if !in_water {
            self.spikes = self.rng.next_float();
        } else if moving {
            self.spikes += (0.0 - self.spikes) * 0.25;
        } else {
            self.spikes += (1.0 - self.spikes) * 0.06;
        }
    }

    /// `getTailAngle`.
    pub fn tail_angle(&self, partial: f32) -> f32 {
        self.prev_tail + (self.tail - self.prev_tail) * partial
    }

    /// `getSpikesExtension`.
    pub fn spikes_extension(&self, partial: f32) -> f32 {
        self.prev_spikes + (self.spikes - self.prev_spikes) * partial
    }
}

/// Segments of `EnderDragonEntity.getSegmentProperties` that `EnderDragonModel` and `EnderDragonRenderer` read.
pub const DRAGON_SEGMENTS: usize = 24;

/// What the dragon renderer needs for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonPose {
    /// `lastWingPosition` eased to `wingPosition`: the wing clock (`animTime` of the model).
    pub wing: f32,
    /// `getSegmentProperties(i, partial)`: yaw and height of the body `i` ticks back.
    pub offsets: [[f64; 2]; DRAGON_SEGMENTS],
}

/// `MathHelper.wrapDegrees(float)`.
fn wrap_degrees_f32(mut f: f32) -> f32 {
    f %= 360.0;
    if f >= 180.0 {
        f -= 360.0;
    }
    if f < -180.0 {
        f += 360.0;
    }
    f
}

/// `MathHelper.wrapDegrees(double)`.
fn wrap_degrees(mut d: f64) -> f64 {
    d %= 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

/// The client part of `EnderDragonEntity.mobTick`: the wing clock and the ring buffer of past yaws
/// and heights (`circularSegmentBuffer`, `latestSegment`).
#[derive(Clone, Debug, PartialEq)]
pub struct Dragon {
    buffer: [[f64; 2]; 64],
    latest: i32,
    prev_wing: f32,
    wing: f32,
}

impl Default for Dragon {
    fn default() -> Dragon {
        Dragon {
            buffer: [[0.0; 2]; 64],
            latest: -1,
            prev_wing: 0.0,
            wing: 0.0,
        }
    }
}

impl Dragon {
    /// One client tick before the position eases toward the server's: `vel` is the entity velocity,
    /// `yaw` and `y` are the current ones. A dead dragon only latches `lastWingPosition`.
    pub fn tick(&mut self, vel: [f64; 3], yaw: f32, y: f64, dead: bool) {
        self.prev_wing = self.wing;
        if dead {
            return;
        }
        let flat = (vel[0] * vel[0] + vel[2] * vel[2]).sqrt() as f32;
        let step = 0.2 / (flat * 10.0 + 1.0) * 2.0_f64.powf(vel[1]) as f32;
        self.wing += step;
        let yaw = f64::from(wrap_degrees_f32(yaw));
        if self.latest < 0 {
            self.buffer = [[yaw, y]; 64];
        }
        self.latest = (self.latest + 1) % 64;
        self.buffer[self.latest as usize] = [yaw, y];
    }

    /// `getSegmentProperties(i, partial)`: a dead dragon (`health <= 0`) reads the newer entry whole.
    pub fn segment(&self, i: i32, partial: f32, dead: bool) -> [f64; 2] {
        let f = 1.0 - if dead { 0.0 } else { partial };
        let at = self.buffer[((self.latest - i) & 63) as usize];
        let next = self.buffer[((self.latest - i - 1) & 63) as usize];
        [
            at[0] + wrap_degrees(next[0] - at[0]) * f64::from(f),
            at[1] + (next[1] - at[1]) * f64::from(f),
        ]
    }

    pub fn pose(&self, partial: f32, dead: bool) -> DragonPose {
        DragonPose {
            wing: self.prev_wing + (self.wing - self.prev_wing) * partial,
            offsets: std::array::from_fn(|i| self.segment(i as i32, partial, dead)),
        }
    }
}

#[cfg(test)]
mod flyer_tests {
    use super::*;

    fn f32_of(hex: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
    }

    fn f64_of(hex: &str) -> f64 {
        f64::from_bits(u64::from_str_radix(hex, 16).unwrap())
    }

    /// `Harness guardian`: every tail and spike value must equal the real `GuardianEntity` bit for bit,
    /// in the air (random spikes) and in the water, moving or not. A guardian that is not moving also
    /// changes between air and water at tick 20 (a moving one draws bubble randoms in the water).
    fn follows_the_real_guardian(water: bool, moving: bool) {
        let switch = if moving { 1000 } else { 20 };
        let script = format!(
            "start 0.5 70 0.5 0\nguardian 60 {} {} 77 {switch}\n",
            u8::from(water),
            u8::from(moving)
        );
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), 61);
        let tail = f32_of(out[0].trim());
        let mut g = Guardian::new(0);
        (g.tail, g.prev_tail, g.rng) = (tail, tail, Lcg::new(77));
        for (tick, line) in out[1..].iter().enumerate() {
            let f: Vec<f32> = line.split(' ').map(f32_of).collect();
            let in_water = water != (tick >= switch);
            g.tick(in_water, moving);
            let at = format!("water {water} moving {moving} tick {tick}");
            assert_eq!(g.tail_angle(0.0), f[0], "{at}");
            assert_eq!(g.tail_angle(1.0), f[1], "{at}");
            assert_eq!(g.spikes_extension(0.0), f[2], "{at}");
            assert_eq!(g.spikes_extension(1.0), f[3], "{at}");
        }
    }

    #[test]
    fn guardian_in_the_water_follows_the_real_one() {
        follows_the_real_guardian(true, false);
        follows_the_real_guardian(true, true);
    }

    #[test]
    fn guardian_that_leaves_the_water_follows_the_real_one() {
        follows_the_real_guardian(false, false);
    }

    /// `Harness dragon`: the wing clock and the ring buffer must equal the real `EnderDragonEntity` after
    /// 3, 40 and 120 ticks of a turning, bobbing flight (the buffer wraps after 64 ticks).
    fn follows_the_real_dragon(velocity: [f64; 3], yaw: f32, turn: f32) {
        let script = format!(
            "start 0.5 70 0.5 0\ndragon 120 {} {} {} {yaw} {turn} 70 4\n",
            velocity[0], velocity[1], velocity[2]
        );
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), 120);
        let mut d = Dragon::default();
        for (tick, line) in out.iter().enumerate() {
            let f: Vec<&str> = line.split(' ').collect();
            d.tick(
                velocity,
                yaw + turn * tick as f32,
                70.0 + 4.0 * (0.3 * tick as f64).sin(),
                false,
            );
            let at = format!("tick {tick}");
            assert_eq!(d.wing, f32_of(f[0]), "{at}");
            assert_eq!(d.prev_wing, f32_of(f[1]), "{at}");
            let pose = d.pose(0.25, false);
            for (k, o) in pose.offsets.iter().enumerate() {
                assert_eq!(o[0], f64_of(f[2 + 2 * k]), "{at} yaw {k}");
                assert_eq!(o[1], f64_of(f[3 + 2 * k]), "{at} height {k}");
            }
        }
    }

    #[test]
    fn dragon_follows_the_real_one() {
        follows_the_real_dragon([0.3, -0.1, 0.2], 10.0, 7.0);
        follows_the_real_dragon([0.0, 0.0, 0.0], -170.0, -3.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values of `WolfEntity.getShakeAngle(0.5, offset)` in the 1.8.9 oracle (`harness/mobmodels.sh`)
    /// with `lastShakeProgress` 0.8 and `shakeProgress` 0.85.
    #[test]
    fn shake_angle_matches_the_oracle() {
        let a = MobAnim {
            shaking: true,
            shake_time: 0.85,
            prev_shake_time: 0.8,
            ..MobAnim::default()
        };
        for (offset, want) in [
            (0.0, -0.060_967_486),
            (-0.08, 0.447_871_83),
            (-0.16, 0.086_133_55),
            (-0.2, -0.224_598_1),
        ] {
            let got = a.shake_angle(0.5, offset);
            // The oracle reads the 65536-entry `MathHelper.sin` table, off by up to about 5e-5 from `f32::sin`.
            assert!((got - want).abs() < 1e-4, "{offset}: {got} vs {want}");
        }
    }

    #[test]
    fn begging_eases_toward_one_and_back() {
        let mut a = MobAnim {
            begging: true,
            ..MobAnim::default()
        };
        a.tick();
        assert!((a.begging_course - 0.4).abs() < 1e-6);
        a.tick();
        assert!((a.begging_course - 0.64).abs() < 1e-6);
        assert!((a.interested_angle(1.0) - 0.64 * 0.15 * std::f32::consts::PI).abs() < 1e-6);
        a.begging = false;
        a.tick();
        assert!((a.begging_course - 0.384).abs() < 1e-6);
        assert!((a.prev_begging_course - 0.64).abs() < 1e-6);
    }

    #[test]
    fn water_makes_a_wolf_wet_and_the_shake_dries_it() {
        let mut a = MobAnim {
            in_water: true,
            ..MobAnim::default()
        };
        a.tick();
        assert_eq!(a.wet_shading(0.0), Some(0.75));
        a.in_water = false;
        a.tick();
        assert!(a.wet, "wet until a shake runs");
        a.set_status(95, 8);
        for _ in 0..42 {
            a.tick();
        }
        assert!(!a.wet && !a.shaking);
        assert_eq!(a.wet_shading(0.0), None);
    }

    #[test]
    fn wet_shading_brightens_with_the_shake() {
        let a = MobAnim {
            wet: true,
            prev_shake_time: 1.0,
            shake_time: 1.5,
            ..MobAnim::default()
        };
        assert_eq!(a.wet_shading(0.0), Some(0.875));
        assert_eq!(a.wet_shading(1.0), Some(0.9375));
    }

    /// `WolfEntity.getHeadRollAngle(0.5)` and `getShakeProgress(0.5)` in the 1.8.9 oracle (`harness/mobmodels.sh`)
    /// with head roll 0.64 then 0.84 and shake progress 0.8 then 0.85.
    #[test]
    fn begging_roll_and_wet_shading_match_the_oracle() {
        let a = MobAnim {
            wet: true,
            prev_begging_course: 0.64,
            begging_course: 0.84,
            prev_shake_time: 0.8,
            shake_time: 0.85,
            ..MobAnim::default()
        };
        assert!((a.interested_angle(0.5) - 0.348_716_83).abs() < 1e-6);
        assert!((a.wet_shading(0.5).unwrap() - 0.853_125).abs() < 1e-6);
    }

    #[test]
    fn rabbit_jump_status_runs_ten_ticks_then_ends() {
        let mut a = MobAnim::default();
        a.set_status(101, 1);
        assert_eq!(a.jump_completion(0.5), 0.05);
        for _ in 0..10 {
            a.tick();
        }
        assert_eq!(a.jump_completion(0.0), 1.0);
        a.tick();
        assert_eq!(a.jump_completion(0.0), 0.0);
        let mut other = MobAnim::default();
        other.set_status(95, 1);
        assert_eq!(other.jump_completion(0.0), 0.0);
    }
}
