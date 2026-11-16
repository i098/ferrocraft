//! The camera light that scales the fog color (`GameRenderer.updateFogColor`, fields `fogColor1`/`fogColor2`).
//! Each client tick the value moves a tenth of the way to the light at the viewer's block.
//! Also holds the other fog colour terms: void depth, Night Vision and the liquid colour.

use super::Fluid;

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

/// `updateFogColor`: the color goes dark below y 32, as `(y / 32)^2`; Blindness scales the depth
/// down over its last second and removes it before that.
pub(super) fn void_darkness(view_y: f32, blindness: Option<i32>, flat: bool) -> f32 {
    let mut depth = f64::from(view_y) * if flat { 1.0 } else { 0.03125 };
    if let Some(ticks) = blindness {
        depth = if ticks < 20 {
            depth * f64::from(1.0 - ticks as f32 / 20.0)
        } else {
            0.0
        };
    }
    if depth < 1.0 {
        depth.max(0.0).powi(2) as f32
    } else {
        1.0
    }
}

/// `updateFogColor`: Night Vision scales the color until its brightest channel is 1, by `strength`.
pub(super) fn night_vision_fog(fog: [f32; 3], strength: f32) -> [f32; 3] {
    // Java's 1/0 is +Inf, so zero channels drop out of the min; an all-zero fog stays black.
    let boost = fog
        .iter()
        .filter(|c| **c > 0.0)
        .map(|c| 1.0 / c)
        .fold(f32::INFINITY, f32::min);
    if strength <= 0.0 || boost.is_infinite() {
        return fog;
    }
    fog.map(|c| c * (1.0 - strength) + c * boost * strength)
}

/// `updateFogColor` and `setupFog`: the liquid replaces the fog color and uses exponential fog;
/// Respiration and Water Breathing lift the water color and thin its density.
pub(super) fn fluid_fog(
    fluid: Option<Fluid>,
    respiration: i32,
    water_breathing: bool,
    fog: [f32; 3],
) -> ([f32; 3], Option<f32>) {
    match fluid {
        Some(Fluid::Water) => {
            let mut lift = respiration as f32 * 0.2;
            let mut density = 0.1 - respiration as f32 * 0.03;
            if water_breathing {
                lift = lift * 0.3 + 0.6;
                density = 0.01;
            }
            ([0.02 + lift, 0.02 + lift, 0.2 + lift], Some(density))
        }
        Some(Fluid::Lava) => ([0.6, 0.1, 0.0], Some(2.0)),
        None => (fog, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::Environment;

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

    fn fog_with(f: impl Fn(&mut Environment)) -> [f32; 3] {
        let mut env = Environment::default();
        f(&mut env);
        env.sky_state(0.8, [0.0; 3]).fog
    }

    #[test]
    fn fog_darkens_with_the_square_of_depth_below_y32() {
        let top = fog_with(|_| {});
        assert_eq!(fog_with(|e| e.view_y = 40.0), top, "no change above y 32");
        assert_eq!(fog_with(|e| e.view_y = 0.0), [0.0; 3]);
        let half = fog_with(|e| e.view_y = 16.0);
        for i in 0..3 {
            assert!((half[i] - top[i] * 0.25).abs() < 1e-6, "{i}");
        }
        assert_eq!(fog_with(|e| e.view_y = -5.0), [0.0; 3]);
    }

    /// Oracle: vanilla `Dimension.getFogSize` is 1 on FLAT worlds, so `y * 1` is at least 1 from y 1 up.
    #[test]
    fn superflat_fog_keeps_its_color_above_y1() {
        let top = fog_with(|_| {});
        assert_eq!(fog_with(|e| e.flat_world = true), top);
        assert_eq!(fog_with(|e| (e.flat_world, e.view_y) = (true, 16.0)), top);
        assert_eq!(fog_with(|e| (e.flat_world, e.view_y) = (true, 1.0)), top);
        assert_eq!(
            fog_with(|e| (e.flat_world, e.view_y) = (true, 0.0)),
            [0.0; 3]
        );
        assert_ne!(fog_with(|e| e.view_y = 16.0), top);
    }

    #[test]
    fn fog_follows_the_camera_light() {
        let top = fog_with(|_| {});
        assert_eq!(fog_with(|e| e.fog_light = 0.0), [0.0; 3]);
        let dim = fog_with(|e| e.fog_light = 0.3);
        for i in 0..3 {
            assert!((dim[i] - top[i] * 0.3).abs() < 1e-6, "{i}");
        }
    }

    #[test]
    fn night_vision_scales_the_brightest_channel_to_one_in_the_dark() {
        let base = fog_with(|e| e.view_y = 16.0);
        let lit = fog_with(|e| {
            e.view_y = 16.0;
            e.night_vision = 1.0;
        });
        // Full strength scales the color until its brightest channel (blue) reaches 1.
        let boost = 1.0 / base[2];
        assert!((lit[2] - 1.0).abs() < 1e-5);
        for i in 0..3 {
            assert!((lit[i] - base[i] * boost).abs() < 1e-5, "{i}");
        }
        let half = fog_with(|e| {
            e.view_y = 16.0;
            e.night_vision = 0.5;
        });
        assert!((half[2] - base[2] * (0.5 + 0.5 * boost)).abs() < 1e-5);
        assert_eq!(fog_with(|e| e.night_vision = 0.0), fog_with(|_| {}));
    }

    #[test]
    fn night_vision_in_lava_scales_to_the_red_channel() {
        let lit = night_vision_fog([0.6, 0.1, 0.0], 1.0);
        for (got, want) in lit.iter().zip([1.0, 0.1 / 0.6, 0.0]) {
            assert!((got - want).abs() < 1e-5);
        }
        assert_eq!(night_vision_fog([0.0; 3], 1.0), [0.0; 3]);
    }

    #[test]
    fn respiration_and_water_breathing_clear_the_water_fog() {
        let water = |respiration, water_breathing| {
            let e = Environment {
                fluid: Some(Fluid::Water),
                respiration,
                water_breathing,
                ..Environment::default()
            };
            let s = e.sky_state(0.8, [0.0; 3]);
            (s.fog, s.fog_density.unwrap())
        };
        let (plain, d) = water(0, false);
        assert_eq!((plain, d), ([0.02, 0.02, 0.2], 0.1));
        let (r3, d3) = water(3, false);
        assert!((r3[0] - 0.62).abs() < 1e-6 && (r3[2] - 0.8).abs() < 1e-6);
        assert!((d3 - 0.01).abs() < 1e-6);
        let (wb, dw) = water(0, true);
        assert!((wb[0] - 0.62).abs() < 1e-6 && (wb[2] - 0.8).abs() < 1e-6);
        assert_eq!(dw, 0.01);
        let (both, _) = water(3, true);
        assert!((both[0] - 0.8).abs() < 1e-6);
    }
}
