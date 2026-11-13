//! Horse riding state of `EntityPlayerSP`: the jump charge and the `EntityHorse` metadata flags.

/// `EntityHorse` metadata index 16 flag: the horse wears a saddle (`isHorseSaddled`).
const SADDLED: i32 = 4;
/// `EntityHorse` metadata index 16 flag: a donkey or mule carries a chest (`isChested`).
const CHESTED: i32 = 8;

pub fn is_saddled(flags: i32) -> bool {
    flags & SADDLED != 0
}

pub fn is_chested(flags: i32) -> bool {
    flags & CHESTED != 0
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
