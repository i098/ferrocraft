//! The compass and clock item sprites of 1.8.9: `TextureCompass.updateCompass` and
//! `TextureClock.updateAnimation` pick one frame of the vertical sprite strip from the world,
//! once per client tick (`TextureManager.tick`). The constants come from the 1.8.9 jar's `bmp`
//! (compass) and `bmo` (clock) classes, read with `javap`.

use render::{celestial_angle, compass_target, dial_frame as frame, DialTurns};
use std::f64::consts::PI;

/// The world state a tick reads from: `Minecraft.theWorld` and `thePlayer`.
#[derive(Clone, Copy, Debug)]
pub struct DialWorld {
    /// `World.getSpawnPoint` (S05 `SpawnPosition`), x and z.
    pub spawn: [i32; 2],
    /// `thePlayer.posX` and `posZ`.
    pub pos: [f64; 2],
    /// `thePlayer.rotationYaw`.
    pub yaw: f32,
    /// `World.getWorldTime`.
    pub time: i64,
}

/// One eased needle: `angle` and its `delta` (the `smoothing` fields of the vanilla classes).
#[derive(Clone, Copy, Debug, Default)]
struct Needle {
    angle: f64,
    delta: f64,
}

impl Needle {
    /// The wobble: pull `delta` a tenth of the way to `target` (wrapped to `period / 2`), damp by 0.8.
    fn ease(&mut self, target: f64, period: f64) {
        let mut d = target - self.angle;
        while d < -period / 2.0 {
            d += period;
        }
        while d >= period / 2.0 {
            d -= period;
        }
        self.delta = (self.delta + d.clamp(-1.0, 1.0) * 0.1) * 0.8;
        self.angle += self.delta;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Dials {
    compass: Needle,
    clock: Needle,
    spawn: [i32; 2],
}

impl Dials {
    /// One client tick. Without a world the compass snaps to 0 and the clock eases towards 0.
    /// shortcut: the nether and end spin both needles at random in vanilla; the client does not
    /// track the dimension, so every world counts as the overworld.
    pub fn tick(&mut self, world: Option<DialWorld>) {
        let Some(w) = world else {
            self.compass.angle = 0.0;
            self.clock.ease(0.0, 1.0);
            return;
        };
        self.spawn = w.spawn;
        self.compass
            .ease(compass_target(w.spawn, w.pos, f64::from(w.yaw)), 2.0 * PI);
        self.clock
            .ease(f64::from(celestial_angle(w.time, 1.0)), 1.0);
    }

    /// The needle positions, for the hand and dropped items in the renderer.
    pub fn turns(&self) -> DialTurns {
        DialTurns {
            compass: self.compass.angle / (2.0 * PI),
            clock: self.clock.angle,
            spawn: self.spawn,
        }
    }

    /// The compass frame of a strip of `frames` sprites.
    pub fn compass_frame(&self, frames: usize) -> usize {
        frame(self.turns().compass, frames)
    }

    /// The clock frame of a strip of `frames` sprites.
    pub fn clock_frame(&self, frames: usize) -> usize {
        frame(self.turns().clock, frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(spawn: [i32; 2], yaw: f32) -> DialWorld {
        DialWorld {
            spawn,
            pos: [0.0, 0.0],
            yaw,
            time: 6000,
        }
    }

    fn settled(w: DialWorld) -> Dials {
        let mut d = Dials::default();
        for _ in 0..400 {
            d.tick(Some(w));
        }
        d
    }

    #[test]
    fn compass_settles_on_the_spawn_bearing() {
        // Spawn due south (+z) of a player facing south (yaw 0): half a turn, frame 16.
        assert_eq!(settled(at([0, 100], 0.0)).compass_frame(32), 16);
        // Turning a quarter (yaw 90, facing west) turns the needle back a quarter: frame 8.
        assert_eq!(settled(at([0, 100], 90.0)).compass_frame(32), 8);
        // Spawn west of a south-facing player: three quarters, frame 24.
        assert_eq!(settled(at([-100, 0], 0.0)).compass_frame(32), 24);
        // Negative yaw wraps like Java's `%`.
        assert_eq!(settled(at([0, 100], -270.0)).compass_frame(32), 8);
    }

    #[test]
    fn compass_needle_wobbles_instead_of_jumping() {
        let mut d = settled(at([0, 100], 0.0));
        d.tick(Some(at([0, 100], 90.0)));
        // The first tick moves the delta by 0.08 rad at most: under half a frame.
        assert!((15..=17).contains(&d.compass_frame(32)));
    }

    #[test]
    fn clock_follows_the_celestial_angle() {
        let mut d = Dials::default();
        for _ in 0..400 {
            d.tick(Some(DialWorld {
                time: 18000,
                ..at([0, 0], 0.0)
            }));
        }
        // Midnight is celestial angle 0.5: half way along the strip.
        assert_eq!(d.clock_frame(64), 32);
    }

    #[test]
    fn no_world_parks_the_compass() {
        let mut d = settled(at([0, 100], 90.0));
        d.tick(None);
        assert_eq!(d.compass_frame(32), 0);
    }
}
