//! Vanilla 1.8.9 `MusicTicker`: schedules one background track at a time, 20 ticks per second.

pub struct MusicTicker {
    delay: u32,
    current: Option<&'static str>,
}

/// What the sound handler must do this tick: stop the current track first, then start `start`.
#[derive(Debug, Default, PartialEq)]
pub struct MusicTick {
    pub stop: bool,
    pub start: Option<&'static str>,
}

impl MusicTicker {
    pub fn new(rand: u32) -> Self {
        Self {
            delay: rand % 100,
            current: None,
        }
    }

    /// Call once per game tick. `playing` = a track is audible now. Plays `music.game` in a
    /// world (gap 12000..24000 ticks) or `music.menu` (gap 20..600 ticks); a type change
    /// stops the playing track and rearms the delay to `0..=min_gap / 2` of the new type.
    pub fn tick(&mut self, in_world: bool, mut playing: bool, rand: u32) -> MusicTick {
        let (min, max, event) = if in_world {
            (12000, 24000, "music.game")
        } else {
            (20, 600, "music.menu")
        };
        let mut out = MusicTick::default();
        if playing && self.current != Some(event) {
            out.stop = true;
            playing = false;
            self.delay = rand % (min / 2 + 1);
        }
        if playing {
            return out;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return out;
        }
        self.delay = min + rand % (max - min);
        self.current = Some(event);
        out.start = Some(event);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(e: &'static str) -> MusicTick {
        MusicTick {
            stop: false,
            start: Some(e),
        }
    }

    #[test]
    fn waits_then_starts_then_rearms_within_vanilla_gap() {
        let mut m = MusicTicker {
            delay: 2,
            current: None,
        };
        assert_eq!(m.tick(true, false, 7), MusicTick::default());
        assert_eq!(m.tick(true, false, 7), MusicTick::default());
        assert_eq!(m.tick(true, false, 7), start("music.game"));
        assert!((12000..24000).contains(&m.delay));
        assert_eq!(m.tick(true, true, 7), MusicTick::default());
        let mut m = MusicTicker::new(0);
        assert_eq!(m.tick(false, false, 1), start("music.menu"));
    }

    #[test]
    fn world_change_stops_menu_track_and_rearms_delay() {
        let mut m = MusicTicker::new(0);
        assert_eq!(m.tick(false, false, 100), start("music.menu"));
        let prior = m.delay;
        let roll = 12345;
        let t = m.tick(true, true, roll);
        assert!(t.stop && t.start.is_none());
        assert_eq!(m.delay, roll % 6001 - 1);
        assert!(m.delay > prior);
    }
}
