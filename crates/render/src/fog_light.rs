//! The camera light that scales the fog color (`GameRenderer.updateFogColor`, fields `fogColor1`/`fogColor2`).
//! Each client tick the value moves a tenth of the way to the light at the viewer's block.

/// `WorldProvider.generateLightBrightnessTable` for the overworld, indexed by light level `0..=15`.
pub fn light_brightness(level: u8) -> f32 {
    let f1 = 1.0 - f32::from(level.min(15)) / 15.0;
    (1.0 - f1) / (f1 * 3.0 + 1.0)
}

/// `GameRenderer.updateRenderer`: the target is the block light brightness, pulled toward 1 as the render distance grows.
fn target(brightness: f32, render_distance_chunks: u32) -> f32 {
    let far = (render_distance_chunks as f32 / 32.0).clamp(0.0, 1.0);
    brightness * (1.0 - far) + far
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FogLight {
    previous: f32,
    current: f32,
    /// Client tick of the last update; `None` before the first frame.
    ticks: Option<u64>,
}

impl FogLight {
    /// Run the ticks since the last call, at most one second of them. The first call snaps to the
    /// target; vanilla starts at 0 and fades in, which no frame should show.
    pub fn advance(&mut self, ticks: u64, brightness: f32, render_distance_chunks: u32) {
        let goal = target(brightness, render_distance_chunks);
        let Some(from) = self.ticks.replace(ticks) else {
            *self = FogLight {
                previous: goal,
                current: goal,
                ticks: Some(ticks),
            };
            return;
        };
        for _ in 0..ticks.saturating_sub(from).min(20) {
            self.previous = self.current;
            self.current += (goal - self.current) * 0.1;
        }
    }

    /// The value for a frame `partial` of the way through the tick.
    pub fn at(&self, partial: f32) -> f32 {
        self.previous + (self.current - self.previous) * partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_table_runs_from_dark_to_full() {
        assert_eq!(light_brightness(0), 0.0);
        assert_eq!(light_brightness(15), 1.0);
        let want = (1.0 - 8.0 / 15.0) / (8.0 / 15.0 * 3.0 + 1.0);
        assert!((light_brightness(7) - want).abs() < 1e-7);
    }

    #[test]
    fn fog_light_moves_a_tenth_per_tick_and_far_views_stay_bright() {
        let mut f = FogLight::default();
        f.advance(100, 1.0, 8);
        assert_eq!(f.at(0.0), 1.0, "first frame snaps");
        f.advance(101, 0.0, 8);
        // Goal at 8 chunks is 0.25, so one tick from 1.0 gives 0.925; the frame blends the last step.
        assert!((f.at(1.0) - 0.925).abs() < 1e-6);
        assert!((f.at(0.5) - 0.9625).abs() < 1e-6);
        let mut far = FogLight::default();
        far.advance(0, 0.0, 32);
        assert_eq!(far.at(0.0), 1.0);
    }
}
