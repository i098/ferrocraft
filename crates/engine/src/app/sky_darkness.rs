//! `GameRenderer.skyDarkness`: a Wither on screen darkens the lightmap and the fog colour.

/// The darkness and its value one tick ago.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SkyDarkness {
    now: f32,
    last: f32,
}

impl SkyDarkness {
    /// `GameRenderer.tick`: +0.05 (to 1.0) while a boss bar sets `modifiesSkyColor`, else -0.0125.
    pub(super) fn tick(&mut self, wither: bool) {
        self.last = self.now;
        if wither {
            self.now = (self.now + 0.05).min(1.0);
        } else if self.now > 0.0 {
            self.now -= 0.0125;
        }
    }

    /// `updateLightmap` and `updateFogColor`: the value is used only while the current one is positive.
    pub(super) fn at(&self, partial: f32) -> f32 {
        if self.now > 0.0 {
            self.last + (self.now - self.last) * partial
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SkyDarkness;

    #[test]
    fn a_wither_reaches_full_darkness_in_20_ticks_and_clears_in_80() {
        let mut d = SkyDarkness::default();
        (0..20).for_each(|_| d.tick(true));
        assert_eq!(d.at(1.0), 1.0);
        (0..79).for_each(|_| d.tick(false));
        assert!(d.at(1.0) > 0.0);
        d.tick(false);
        assert!(d.at(1.0) < 1e-5, "{}", d.at(1.0));
        d.tick(false);
        assert_eq!(d.at(1.0), 0.0);
    }

    #[test]
    fn the_value_is_interpolated_with_the_partial_tick() {
        let mut d = SkyDarkness::default();
        d.tick(true);
        d.tick(true);
        assert!((d.at(0.5) - 0.075).abs() < 1e-6);
    }
}
