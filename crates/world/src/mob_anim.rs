//! Timers that entity status bytes start: iron golem arms (4, 11) and wolf shake (8).

const GOLEM: u8 = 99;
const WOLF: u8 = 95;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MobAnim {
    /// Iron golem `attackTimer`: 10 at status 4, counts down every tick.
    pub golem_attack: i32,
    /// Iron golem `holdRoseTick`: 400 at status 11, counts down every tick.
    pub golem_rose: i32,
    /// Wolf `isShaking`.
    pub shaking: bool,
    /// Wolf `timeWolfIsShaking`.
    pub shake_time: f32,
    /// Wolf `prevTimeWolfIsShaking`.
    pub prev_shake_time: f32,
}

impl MobAnim {
    /// `EntityIronGolem` and `EntityWolf` `handleStatusUpdate`.
    pub fn set_status(&mut self, mob: u8, status: i8) {
        match (mob, status) {
            (GOLEM, 4) => self.golem_attack = 10,
            (GOLEM, 11) => self.golem_rose = 400,
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
        self.prev_shake_time = self.shake_time;
        if self.shaking {
            self.shake_time += 0.05;
            if self.prev_shake_time >= 2.0 {
                *self = MobAnim {
                    golem_attack: self.golem_attack,
                    golem_rose: self.golem_rose,
                    ..MobAnim::default()
                };
            }
        }
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
}
