//! Horse riding state of `EntityPlayerSP`: the jump charge and the `EntityHorse` metadata flags.

/// `EntityHorse` metadata index 16 flag: the horse is tame (`isTame`).
const TAME: i32 = 2;
/// `EntityHorse` metadata index 16 flag: the horse wears a saddle (`isHorseSaddled`).
const SADDLED: i32 = 4;
/// `EntityHorse` metadata index 16 flag: a donkey or mule carries a chest (`isChested`).
const CHESTED: i32 = 8;

pub fn is_tame(flags: i32) -> bool {
    flags & TAME != 0
}

pub fn is_saddled(flags: i32) -> bool {
    flags & SADDLED != 0
}

pub fn is_chested(flags: i32) -> bool {
    flags & CHESTED != 0
}

/// `EntityHorse` metadata index 16 flags: the horse eats grass, rears (`isAngry`), opens its mouth.
const EATING_HAY: i32 = 32;
const ANGRY: i32 = 64;
const MOUTH_OPEN: i32 = 128;

/// The three animation progress values `EntityHorse.onUpdate` eases on the client toward the metadata
/// flags, each with its value one tick ago for the partial tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HorseAnim {
    grass: [f32; 2],
    angry: [f32; 2],
    mouth: [f32; 2],
}

impl HorseAnim {
    /// The animation part of `EntityHorse.onUpdate`, once per tick with the flags of metadata 16.
    pub fn tick(&mut self, flags: i32) {
        self.grass[0] = self.grass[1];
        if flags & EATING_HAY != 0 {
            self.grass[1] = (self.grass[1] + (1.0 - self.grass[1]) * 0.4 + 0.05).min(1.0);
        } else {
            self.grass[1] = (self.grass[1] + ((0.0 - self.grass[1]) * 0.4 - 0.05)).max(0.0);
        }
        self.angry[0] = self.angry[1];
        if flags & ANGRY != 0 {
            self.grass = [0.0; 2];
            self.angry[1] = (self.angry[1] + (1.0 - self.angry[1]) * 0.4 + 0.05).min(1.0);
        } else {
            let a = self.angry[1];
            self.angry[1] = (a + ((0.8 * a * a * a - a) * 0.6 - 0.05)).max(0.0);
        }
        self.mouth[0] = self.mouth[1];
        if flags & MOUTH_OPEN != 0 {
            self.mouth[1] = (self.mouth[1] + (1.0 - self.mouth[1]) * 0.7 + 0.05).min(1.0);
        } else {
            self.mouth[1] = (self.mouth[1] + ((0.0 - self.mouth[1]) * 0.7 - 0.05)).max(0.0);
        }
    }

    /// `prevRearingAmount`: the rearing of the last tick, which `updateRiderPosition` reads.
    pub fn prev_rearing(&self) -> f32 {
        self.angry[0]
    }

    /// `getGrassAnimationProgress`, `getAngryAnimationProgress` and `getEatingAnimationProgress`.
    pub fn progress(&self, partial: f32) -> [f32; 3] {
        [self.grass, self.angry, self.mouth].map(|[last, now]| last + (now - last) * partial)
    }
}

/// `EntityPlayerSP.horseJumpPowerCounter` and `horseJumpPower`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HorseJump {
    counter: i32,
    /// `getHorseJumpPower`: 0..=1, drawn in place of the experience bar.
    pub power: f32,
    jump_held: bool,
}

impl HorseJump {
    /// The riding part of `onLivingUpdate`, once per tick with the jump key now. Returns the
    /// `EntityAction` 5 boost (`(int)(power * 100)`) when the key was released this tick.
    pub fn tick(&mut self, riding_horse: bool, jump: bool) -> Option<i32> {
        let was = std::mem::replace(&mut self.jump_held, jump);
        if !riding_horse {
            self.power = 0.0;
            return None;
        }
        if self.counter < 0 {
            self.counter += 1;
            if self.counter == 0 {
                self.power = 0.0;
            }
        }
        if was && !jump {
            self.counter = -10;
            return Some((self.power * 100.0) as i32);
        }
        if !was && jump {
            self.counter = 0;
            self.power = 0.0;
        } else if was {
            self.counter += 1;
            self.power = if self.counter < 10 {
                self.counter as f32 * 0.1
            } else {
                0.8 + 2.0 / (self.counter - 9) as f32 * 0.1
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eating_eases_up_and_rearing_cuts_it_off() {
        let mut a = HorseAnim::default();
        a.tick(EATING_HAY);
        assert!((a.progress(1.0)[0] - 0.45).abs() < 1e-6);
        assert_eq!(a.progress(0.0)[0], 0.0);
        for _ in 0..20 {
            a.tick(EATING_HAY);
        }
        assert_eq!(a.progress(1.0)[0], 1.0);
        a.tick(EATING_HAY | ANGRY);
        let p = a.progress(1.0);
        assert_eq!((p[0], p[2]), (0.0, 0.0));
        assert!((p[1] - 0.45).abs() < 1e-6);
        for _ in 0..20 {
            a.tick(0);
        }
        assert_eq!(a.progress(1.0), [0.0; 3]);
    }

    #[test]
    fn release_sends_the_charge() {
        let mut h = HorseJump::default();
        assert_eq!(h.tick(true, true), None);
        for _ in 0..4 {
            h.tick(true, true);
        }
        assert!((h.power - 0.4).abs() < 1e-6);
        assert_eq!(h.tick(true, false), Some(40));
    }

    #[test]
    fn full_charge_stays_near_one() {
        let mut h = HorseJump::default();
        for _ in 0..30 {
            h.tick(true, true);
        }
        // counter 29: 0.8 + 2 / 20 * 0.1
        assert_eq!(h.tick(true, false), Some(81));
    }

    #[test]
    fn nothing_on_foot() {
        let mut h = HorseJump::default();
        assert_eq!(h.tick(false, true), None);
        assert_eq!(h.tick(false, false), None);
        assert_eq!(h.power, 0.0);
    }
}
