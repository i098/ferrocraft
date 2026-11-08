//! Vanilla 1.8.9 `MusicTicker`: schedules one background track at a time, 20 ticks per second.

pub struct MusicTicker {
    delay: u32,
}

impl MusicTicker {
    pub fn new(rand: u32) -> Self {
        Self { delay: rand % 100 }
    }

    /// Call once per game tick. `playing` = a track is audible now. Returns the event to start:
    /// `music.game` in a world (gap 12000..24000 ticks) or `music.menu` (gap 20..600 ticks).
    pub fn tick(&mut self, in_world: bool, playing: bool, rand: u32) -> Option<&'static str> {
        if playing {
            return None;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return None;
        }
        let (min, max, event) = if in_world { (12000, 24000, "music.game") } else { (20, 600, "music.menu") };
        self.delay = min + rand % (max - min);
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_then_starts_then_rearms_within_vanilla_gap() {
        let mut m = MusicTicker { delay: 2 };
        assert_eq!(m.tick(true, false, 7), None);
        assert_eq!(m.tick(true, false, 7), None);
        assert_eq!(m.tick(true, false, 7), Some("music.game"));
        assert!((12000..24000).contains(&m.delay));
        assert_eq!(m.tick(true, true, 7), None);
        let mut m = MusicTicker { delay: 0 };
        assert_eq!(m.tick(false, false, 1), Some("music.menu"));
    }
}
