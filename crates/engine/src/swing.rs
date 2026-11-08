//! Arm swing: `EntityLivingBase.swingItem` and `updateArmSwingProgress` of 1.8.9 (6 ticks, no Haste or Mining Fatigue).

const DURATION: i32 = 6;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Swing {
    in_progress: bool,
    ticks: i32,
    prev: f32,
    now: f32,
}

impl Swing {
    /// Left click: restart only when no swing runs or it is at least half done.
    pub fn start(&mut self) {
        if !self.in_progress || self.ticks >= DURATION / 2 || self.ticks < 0 {
            self.ticks = -1;
            self.in_progress = true;
        }
    }

    pub fn tick(&mut self) {
        self.prev = self.now;
        if self.in_progress {
            self.ticks += 1;
            if self.ticks >= DURATION {
                self.ticks = 0;
                self.in_progress = false;
            }
        } else {
            self.ticks = 0;
        }
        self.now = self.ticks as f32 / DURATION as f32;
    }

    /// `getSwingProgress(partial)`, `0..1`.
    pub fn progress(&self, partial: f32) -> f32 {
        let mut d = self.now - self.prev;
        if d < 0.0 {
            d += 1.0;
        }
        self.prev + d * partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swing_runs_six_ticks_then_rests() {
        let mut s = Swing::default();
        s.start();
        let mut seen = Vec::new();
        for _ in 0..7 {
            s.tick();
            seen.push(s.progress(1.0));
        }
        assert_eq!(seen[0], 0.0);
        assert!((seen[3] - 0.5).abs() < 1e-6);
        assert!((seen[5] - 5.0 / 6.0).abs() < 1e-6);
        // The wrap tick interpolates upward to 1 instead of jumping back, then the arm rests.
        assert!((seen[6] - 1.0).abs() < 1e-6);
        s.tick();
        assert_eq!(s.progress(1.0), 0.0);
    }
}
