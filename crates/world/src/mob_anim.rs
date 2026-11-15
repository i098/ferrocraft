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
