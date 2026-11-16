//! `LocalClientPlayerEntity` (`EntityPlayerSP.onLivingUpdate`) eased arm look: `renderArmPitch` and `renderArmYaw` close half the gap to the look each tick.

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ArmLook {
    prev: [f32; 2],
    now: [f32; 2],
}

impl ArmLook {
    /// One tick towards `[pitch, yaw]`.
    pub fn tick(&mut self, look: [f32; 2]) {
        self.prev = self.now;
        for (v, l) in self.now.iter_mut().zip(look) {
            *v += (l - *v) * 0.5;
        }
    }

    /// `[pitch, yaw]` between the last two ticks.
    pub fn at(&self, frac: f32) -> [f32; 2] {
        std::array::from_fn(|i| self.prev[i] + (self.now[i] - self.prev[i]) * frac)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_half_the_gap_per_tick_and_interpolates() {
        let mut a = ArmLook::default();
        a.tick([20.0, -40.0]);
        assert_eq!(a.at(1.0), [10.0, -20.0]);
        a.tick([20.0, -40.0]);
        assert_eq!(a.at(0.0), [10.0, -20.0]);
        assert_eq!(a.at(0.5), [12.5, -25.0]);
        assert_eq!(a.at(1.0), [15.0, -30.0]);
    }
}
