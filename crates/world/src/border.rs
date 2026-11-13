//! `WorldBorder` (1.8.9) as the client keeps it: set by `S44 WorldBorder`, read by the force-field
//! render, the vignette warning and the collision of `World.getCollidingBoundingBoxes`.

/// `WorldBorder.maxSize`.
const MAX_SIZE: f64 = 29_999_984.0;

/// `EnumBorderStatus`: the force-field color is `color()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Growing,
    Shrinking,
    Stationary,
}

impl Status {
    /// RGB of `EnumBorderStatus.getID`: 0x40FF80, 0xFF3030, 0x20A0FF.
    pub fn color(self) -> [f32; 3] {
        let id: u32 = match self {
            Status::Growing => 0x40FF80,
            Status::Shrinking => 0xFF3030,
            Status::Stationary => 0x20A0FF,
        };
        [16, 8, 0].map(|s| (id >> s & 255) as f32 / 255.0)
    }
}

/// What `S44` tells the client, in `NetHandlerPlayClient.handleWorldBorder` terms. Times are milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    SetSize(f64),
    LerpSize {
        old: f64,
        new: f64,
        millis: i64,
    },
    SetCenter {
        x: f64,
        z: f64,
    },
    Initialize {
        x: f64,
        z: f64,
        old: f64,
        new: f64,
        millis: i64,
        warning_time: i32,
        warning_blocks: i32,
    },
    WarningTime(i32),
    WarningBlocks(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldBorder {
    center: [f64; 2],
    start: f64,
    end: f64,
    start_ms: i64,
    end_ms: i64,
    pub warning_time: i32,
    pub warning_blocks: i32,
}

impl Default for WorldBorder {
    /// A new world: 60 million blocks wide, warning at 5 blocks or 15 seconds.
    fn default() -> Self {
        WorldBorder {
            center: [0.0; 2],
            start: 6.0e7,
            end: 6.0e7,
            start_ms: 0,
            end_ms: 0,
            warning_time: 15,
            warning_blocks: 5,
        }
    }
}

impl WorldBorder {
    pub fn apply(&mut self, u: Update, now: i64) {
        match u {
            Update::SetSize(d) => self.set_transition(d, now),
            Update::LerpSize { old, new, millis } => self.lerp(old, new, millis, now),
            Update::SetCenter { x, z } => self.center = [x, z],
            Update::Initialize {
                x,
                z,
                old,
                new,
                millis,
                warning_time,
                warning_blocks,
            } => {
                self.center = [x, z];
                if millis > 0 {
                    self.lerp(old, new, millis, now);
                } else {
                    self.set_transition(new, now);
                }
                self.warning_time = warning_time;
                self.warning_blocks = warning_blocks;
            }
            Update::WarningTime(t) => self.warning_time = t,
            Update::WarningBlocks(b) => self.warning_blocks = b,
        }
    }

    fn set_transition(&mut self, d: f64, now: i64) {
        (self.start, self.end, self.start_ms, self.end_ms) = (d, d, now, now);
    }

    fn lerp(&mut self, old: f64, new: f64, millis: i64, now: i64) {
        (self.start, self.end, self.start_ms, self.end_ms) = (old, new, now, now + millis);
    }

    pub fn status(&self) -> Status {
        if self.end < self.start {
            Status::Shrinking
        } else if self.end > self.start {
            Status::Growing
        } else {
            Status::Stationary
        }
    }

    /// `getDiameter`: the lerp is evaluated in `float`.
    pub fn diameter(&self, now: i64) -> f64 {
        if self.status() != Status::Stationary {
            let f = f64::from((now - self.start_ms) as f32 / (self.end_ms - self.start_ms) as f32);
            if f < 1.0 {
                return self.start + (self.end - self.start) * f;
            }
            return self.end;
        }
        self.start
    }

    /// A finished lerp becomes the new size, as the first `getDiameter` after it does in vanilla.
    pub fn settle(&mut self, now: i64) {
        if self.status() != Status::Stationary && now >= self.end_ms {
            self.set_transition(self.end, now);
        }
    }

    /// `getTargetSize`.
    pub fn target(&self) -> f64 {
        self.end
    }

    /// `getResizeSpeed`: blocks per second, 0 when it is not resizing.
    pub fn resize_speed(&self) -> f64 {
        if self.end_ms == self.start_ms {
            0.0
        } else {
            (self.end - self.start).abs() / ((self.end_ms - self.start_ms) as f64 / 1000.0)
        }
    }

    /// `[minX, minZ, maxX, maxZ]`, each clamped to `maxSize`.
    pub fn bounds(&self, now: i64) -> [f64; 4] {
        let half = self.diameter(now) / 2.0;
        let [cx, cz] = self.center;
        let c = |v: f64| v.clamp(-MAX_SIZE, MAX_SIZE);
        [c(cx - half), c(cz - half), c(cx + half), c(cz + half)]
    }

    /// `getClosestDistance`: the distance to the nearest of the four walls.
    pub fn closest_distance(&self, x: f64, z: f64, now: i64) -> f64 {
        let [min_x, min_z, max_x, max_z] = self.bounds(now);
        (z - min_z).min(max_z - z).min(x - min_x).min(max_x - x)
    }

    /// `contains(BlockPos)` for the `bounds` of [`WorldBorder::bounds`].
    pub fn contains_block([min_x, min_z, max_x, max_z]: [f64; 4], x: i32, z: i32) -> bool {
        f64::from(x) + 1.0 > min_x
            && f64::from(x) < max_x
            && f64::from(z) + 1.0 > min_z
            && f64::from(z) < max_z
    }

    /// `World.isInsideBorder`: the walls move one block out for an entity that is inside and one block in for
    /// one that is outside (`outside` is `Entity.isOutsideBorder`), so it does not flicker at the wall.
    pub fn inside([min_x, min_z, max_x, max_z]: [f64; 4], x: f64, z: f64, outside: bool) -> bool {
        let slack = if outside { 1.0 } else { -1.0 };
        x > min_x + slack && x < max_x - slack && z > min_z + slack && z < max_z - slack
    }

    /// The vignette warning of `GuiIngame.renderVignette`, `0..=1`: how deep the player is in the warning band.
    pub fn warning(&self, x: f64, z: f64, now: i64) -> f32 {
        let moved = self.resize_speed() * f64::from(self.warning_time) * 1000.0;
        let diameter = self.diameter(now);
        let band = moved
            .min((self.target() - diameter).abs())
            .max(f64::from(self.warning_blocks));
        let d = self.closest_distance(x, z, now);
        if d < band {
            (1.0 - d / band) as f32
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sized(d: f64) -> WorldBorder {
        let mut b = WorldBorder::default();
        b.apply(Update::SetSize(d), 0);
        b
    }

    #[test]
    fn walls_and_distance() {
        let b = sized(20.0);
        assert_eq!(b.bounds(0), [-10.0, -10.0, 10.0, 10.0]);
        assert_eq!(b.closest_distance(4.0, 0.0, 0), 6.0);
        let w = b.bounds(0);
        assert!(WorldBorder::contains_block(w, 9, 0) && !WorldBorder::contains_block(w, 10, 0));
    }

    #[test]
    fn lerp_runs_then_settles() {
        let mut b = WorldBorder::default();
        b.apply(
            Update::LerpSize {
                old: 100.0,
                new: 50.0,
                millis: 1000,
            },
            0,
        );
        assert_eq!(b.status(), Status::Shrinking);
        assert_eq!(b.diameter(500), 75.0);
        assert_eq!(b.diameter(2000), 50.0);
        b.settle(2000);
        assert_eq!(b.status(), Status::Stationary);
    }

    /// `isInsideBorder` keeps an entity inside one block past the wall and lets an outside one in only one block deep.
    #[test]
    fn inside_has_one_block_of_slack() {
        let w = sized(20.0).bounds(0);
        assert!(WorldBorder::inside(w, 10.5, 0.0, false));
        assert!(!WorldBorder::inside(w, 11.5, 0.0, false));
        assert!(!WorldBorder::inside(w, 9.5, 0.0, true));
        assert!(WorldBorder::inside(w, 8.5, 0.0, true));
    }

    #[test]
    fn warning_grows_toward_the_wall() {
        let b = sized(100.0);
        assert_eq!(b.warning(0.0, 0.0, 0), 0.0);
        assert_eq!(b.warning(47.5, 0.0, 0), 0.5);
    }
}
