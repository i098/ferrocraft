//! World values the mods change, asked once per frame (wire message `'O'`).
//!
//! Rust to Java: `'O'`, f32 partial tick, f32 yaw, f32 pitch (the player), f32 yaw delta, f32 pitch delta (the
//! look of this frame in degrees, as the engine would apply it), u8 perspective, i64 world time, f32 rain, f32
//! thunder, f32 FOV setting, f32 hand FOV modifier (previous, now), i32 night vision ticks left (-1 without the
//! effect).
//! Java to Rust: u8 valid bits (see the `VALID_*` constants), i64 time, f32 rain, f32 thunder, f32 camera yaw,
//! f32 camera pitch, f32 camera distance, f32 FOV, f32 night vision brightness, f32 yaw delta, f32 pitch delta
//! (what the look did to the player). A group whose bit is clear is vanilla: Java could not run its getter.

use std::io::{self, Read, Write};

use crate::wire::{f32_, u8_};

const VALID_TIME: u8 = 1;
const VALID_WEATHER: u8 = 2;
const VALID_CAMERA: u8 = 4;
const VALID_FOV: u8 = 8;
const VALID_NIGHT_VISION: u8 = 16;
const VALID_LOOK: u8 = 32;

/// What the engine knows this frame, which the mixed getters start from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldState {
    pub partial: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// The look of this frame, as `game.look` takes it.
    pub look: (f32, f32),
    /// `thirdPersonView`: 0 first person, 1 back, 2 front.
    pub perspective: u8,
    pub world_time: i64,
    pub rain: f32,
    pub thunder: f32,
    /// The FOV option in degrees.
    pub fov: f32,
    /// `EntityRenderer.fovModifierHand` before and now.
    pub fov_mod: (f32, f32),
    /// Ticks of the night vision effect that are left.
    pub night_vision: Option<i32>,
}

/// The camera as `EntityRenderer.orientCamera` leaves it, yaw and pitch of the view direction in the player's terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

/// What the mods leave of each value; `None` is the vanilla value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Overrides {
    /// `World.getWorldTime`.
    pub time: Option<i64>,
    /// `World.getRainStrength` and `getThunderStrength`.
    pub weather: Option<(f32, f32)>,
    pub camera: Option<Camera>,
    /// `EntityRenderer.getFOVModifier` in degrees, before the death and water zoom.
    pub fov: Option<f32>,
    /// `EntityRenderer.getNightVisionBrightness`.
    pub night_vision: Option<f32>,
    /// What `Entity.setAngles` did to the player's yaw and pitch: a mod can keep it from turning the player.
    pub look: Option<(f32, f32)>,
}

pub fn write_state(w: &mut impl Write, s: &WorldState) -> io::Result<()> {
    let mut b = vec![b'O'];
    for v in [s.partial, s.yaw, s.pitch, s.look.0, s.look.1] {
        b.extend(v.to_be_bytes());
    }
    b.push(s.perspective);
    b.extend(s.world_time.to_be_bytes());
    for v in [s.rain, s.thunder, s.fov, s.fov_mod.0, s.fov_mod.1] {
        b.extend(v.to_be_bytes());
    }
    b.extend(s.night_vision.unwrap_or(-1).to_be_bytes());
    w.write_all(&b)?;
    w.flush()
}

pub fn read_overrides(r: &mut impl Read) -> io::Result<Overrides> {
    let valid = u8_(r)?;
    let mut time = [0; 8];
    r.read_exact(&mut time)?;
    let mut f = [0.0; 9];
    for v in &mut f {
        *v = f32_(r)?;
    }
    let has = |bit: u8| valid & bit != 0;
    Ok(Overrides {
        time: has(VALID_TIME).then(|| i64::from_be_bytes(time)),
        weather: has(VALID_WEATHER).then_some((f[0], f[1])),
        camera: has(VALID_CAMERA).then_some(Camera {
            yaw: f[2],
            pitch: f[3],
            distance: f[4],
        }),
        fov: has(VALID_FOV).then_some(f[5]),
        night_vision: has(VALID_NIGHT_VISION).then_some(f[6]),
        look: has(VALID_LOOK).then_some((f[7], f[8])),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_request_java_reads() {
        let s = WorldState {
            partial: 0.5,
            yaw: 90.0,
            pitch: -10.0,
            look: (1.0, 2.0),
            perspective: 1,
            world_time: 18000,
            rain: 0.25,
            thunder: 0.0,
            fov: 70.0,
            fov_mod: (1.0, 1.1),
            night_vision: None,
        };
        let mut out = Vec::new();
        write_state(&mut out, &s).unwrap();
        let mut want = vec![b'O'];
        for v in [0.5f32, 90.0, -10.0, 1.0, 2.0] {
            want.extend(v.to_be_bytes());
        }
        want.push(1);
        want.extend(18000i64.to_be_bytes());
        for v in [0.25f32, 0.0, 70.0, 1.0, 1.1] {
            want.extend(v.to_be_bytes());
        }
        want.extend((-1i32).to_be_bytes());
        assert_eq!(out, want);
    }

    #[test]
    fn reads_only_the_groups_java_marked_valid() {
        // Noon from PolyTime and a camera from FreeLook; weather, FOV and night vision stay vanilla.
        let mut b = vec![VALID_TIME | VALID_CAMERA | VALID_LOOK];
        b.extend(6000i64.to_be_bytes());
        for v in [9.0f32, 9.0, 10.0, -5.0, 4.0, 9.0, 9.0, 0.0, 0.0] {
            b.extend(v.to_be_bytes());
        }
        let o = read_overrides(&mut b.as_slice()).unwrap();
        assert_eq!(
            o,
            Overrides {
                time: Some(6000),
                camera: Some(Camera {
                    yaw: 10.0,
                    pitch: -5.0,
                    distance: 4.0
                }),
                look: Some((0.0, 0.0)),
                ..Overrides::default()
            }
        );
        assert!(read_overrides(&mut &b[..b.len() - 1]).is_err());
    }
}
