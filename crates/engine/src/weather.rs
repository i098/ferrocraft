//! Client-side weather: `World.updateWeatherBody` and `NetHandlerPlayClient.handleChangeGameState` of 1.8.9.

const STEP: f32 = 0.01;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Weather {
    /// `WorldInfo.raining`, set only by Change Game State reasons 1 and 2.
    raining: bool,
    rain: f32,
    prev_rain: f32,
    thunder: f32,
    prev_thunder: f32,
}

impl Weather {
    /// Change Game State: 1 begin rain, 2 end rain, 7 rain strength, 8 thunder strength; other reasons are not weather.
    pub fn apply(&mut self, reason: u8, value: f32) {
        match reason {
            1 => {
                self.raining = true;
                self.rain = 0.0;
            }
            2 => {
                self.raining = false;
                self.rain = 1.0;
            }
            7 => self.rain = value,
            8 => self.thunder = value,
            _ => {}
        }
    }

    /// One 20 TPS step. The client never learns `WorldInfo.thundering`, so thunder only decays between server updates.
    pub fn tick(&mut self) {
        self.prev_thunder = self.thunder;
        self.thunder = (self.thunder - STEP).clamp(0.0, 1.0);
        self.prev_rain = self.rain;
        let dir = if self.raining { STEP } else { -STEP };
        self.rain = (self.rain + dir).clamp(0.0, 1.0);
    }

    /// `World.getRainStrength(partial)` and `getThunderStrength(partial)`.
    pub fn strengths(&self, partial: f32) -> (f32, f32) {
        let lerp = |prev: f32, now: f32| prev + (now - prev) * partial;
        (
            lerp(self.prev_rain, self.rain),
            lerp(self.prev_thunder, self.thunder),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_ramps_up_after_begin_and_down_after_end() {
        let mut w = Weather::default();
        w.apply(1, 0.0);
        for _ in 0..50 {
            w.tick();
        }
        assert!((w.strengths(1.0).0 - 0.5).abs() < 1e-4);
        w.apply(2, 0.0);
        assert_eq!(w.strengths(1.0).0, 1.0);
        w.tick();
        assert!((w.strengths(1.0).0 - 0.99).abs() < 1e-4);
        w.apply(7, 0.25);
        for _ in 0..100 {
            w.tick();
        }
        assert_eq!(w.strengths(1.0).0, 0.0);
    }

    #[test]
    fn strength_interpolates_between_ticks() {
        let mut w = Weather::default();
        w.apply(8, 1.0);
        w.tick();
        assert!((w.strengths(0.5).1 - 0.995).abs() < 1e-4);
    }
}
