//! Client-side animation of a squid, ported from `EntitySquid.onLivingUpdate` and the parts of
//! `EntityLivingBase` that act on it (`handleStatusUpdate` 19, the 0.98 motion decay and `func_110146_f`,
//! the body turn). Feather names: `bodyPitch` (squidPitch), `bodyRoll` (squidYaw), `tentacleMovement`
//! (squidRotation), `tentacleRotation` (tentacleAngle), `tentacleSpeed` (rotationVelocity) and
//! `rotationSpeed` (rotateSpeed). `RenderSquid` reads [`Squid::pose`].

use crate::body::wrap_degrees;
use std::f32::consts::PI;

const TAU: f32 = std::f32::consts::TAU;

/// What `RenderSquid` and `ModelSquid` need for a frame, already eased by the partial tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// `renderYawOffset`, degrees.
    pub body_yaw: f32,
    /// `squidPitch`, degrees.
    pub pitch: f32,
    /// `squidYaw`, degrees: the roll about the body axis.
    pub roll: f32,
    /// `tentacleAngle`, radians: the x rotation of every tentacle.
    pub tentacle: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Squid {
    pitch: f32,
    last_pitch: f32,
    roll: f32,
    last_roll: f32,
    movement: f32,
    tentacle: f32,
    last_tentacle: f32,
    speed: f32,
    tentacle_speed: f32,
    rotation_speed: f32,
    body_yaw: f32,
    last_body_yaw: f32,
    control: crate::body::BodyControl,
}

impl Squid {
    /// A squid spawned with entity `id` at body yaw `yaw`. The constructor seeds `random` with
    /// `1 + id` and sets `tentacleSpeed = 1 / (nextFloat() + 1) * 0.2`.
    pub fn new(id: i32, yaw: f32) -> Squid {
        const MULT: i64 = 0x5_DEEC_E66D;
        let seed = ((i64::from(id) + 1) ^ MULT) & ((1 << 48) - 1);
        let next = seed.wrapping_mul(MULT).wrapping_add(0xB) & ((1 << 48) - 1);
        let float = (next >> 24) as f32 / (1 << 24) as f32;
        Squid {
            tentacle_speed: 1.0 / (float + 1.0) * 0.2,
            body_yaw: yaw,
            last_body_yaw: yaw,
            ..Squid::default()
        }
    }

    /// `doEvent(19)`: the server finished a tentacle cycle.
    pub fn set_status(&mut self, status: i8) {
        if status == 19 {
            self.movement = 0.0;
        }
    }

    /// One client tick, after the entity moved by `moved` (x and z of `pos - prevPos`). `vel` is the
    /// motion the server sent; `interpolating` tells that the position still eased toward the server
    /// one this tick (`newPosRotationIncrements > 0`), which keeps the motion from decaying. `yaw` is
    /// the entity's `rotationYaw`.
    pub fn tick(
        &mut self,
        vel: &mut [f64; 3],
        moved: [f64; 2],
        yaw: f32,
        interpolating: bool,
        in_water: bool,
        head_yaw: &mut f32,
    ) {
        self.last_body_yaw = self.body_yaw;
        // `EntityLivingBase.onLivingUpdate`: remote motion decays and snaps to zero.
        if !interpolating {
            vel.iter_mut().for_each(|v| *v *= 0.98);
        }
        vel.iter_mut()
            .filter(|v| v.abs() < 0.005)
            .for_each(|v| *v = 0.0);
        self.last_pitch = self.pitch;
        self.last_roll = self.roll;
        self.last_tentacle = self.tentacle;
        self.movement += self.tentacle_speed;
        // A client squid waits at one turn for the server's status 19.
        if f64::from(self.movement) > std::f64::consts::TAU {
            self.movement = TAU;
        }
        if in_water {
            self.swim(vel);
        } else {
            self.tentacle = (crate::sin(self.movement)).abs() * PI * 0.25;
            self.pitch += (-90.0 - self.pitch) * 0.02;
        }
        // In water the squid sets `rotationYaw = renderYawOffset` before the body turn.
        let rotation_yaw = if in_water { self.body_yaw } else { yaw };
        self.control
            .tick(rotation_yaw, moved, &mut self.body_yaw, head_yaw);
    }

    /// The `inWater` branch of `onLivingUpdate`: the tentacle stroke and the body angles.
    fn swim(&mut self, vel: &[f64; 3]) {
        if self.movement < PI {
            let f = self.movement / PI;
            self.tentacle = crate::sin(f * f * PI) * PI * 0.25;
            if f64::from(f) > 0.75 {
                self.speed = 1.0;
                self.rotation_speed = 1.0;
            } else {
                self.rotation_speed *= 0.8;
            }
        } else {
            self.tentacle = 0.0;
            self.speed *= 0.9;
            self.rotation_speed *= 0.99;
        }
        let horizontal = (vel[0] * vel[0] + vel[2] * vel[2]).sqrt() as f32;
        // `MathHelper.fastAtan2` is a table approximation of `atan2`; the difference is far below a pixel.
        let heading = (-(vel[0].atan2(vel[2])) as f32) * 180.0 / PI;
        self.body_yaw += (heading - self.body_yaw) * 0.1;
        self.roll = (f64::from(self.roll)
            + std::f64::consts::PI * f64::from(self.rotation_speed) * 1.5)
            as f32;
        let climb = (-(f64::from(horizontal).atan2(vel[1])) as f32) * 180.0 / PI;
        self.pitch += (climb - self.pitch) * 0.1;
    }

    /// The pose of the frame at `partial` of the tick.
    pub fn pose(&self, partial: f32) -> Pose {
        let ease = |last: f32, now: f32| last + (now - last) * partial;
        Pose {
            body_yaw: ease(
                self.last_body_yaw,
                self.last_body_yaw + wrap_degrees(self.body_yaw - self.last_body_yaw),
            ),
            pitch: ease(self.last_pitch, self.pitch),
            roll: ease(self.last_roll, self.roll),
            tentacle: ease(self.last_tentacle, self.tentacle),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_of(hex: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
    }

    fn f64_of(hex: &str) -> f64 {
        f64::from_bits(u64::from_str_radix(hex, 16).unwrap())
    }

    /// `Harness squid`: the real `SquidEntity`, run as a client with the given motion, yaw and status
    /// tick. The oracle's movement and yaw feed the port; every state value must follow the real one.
    /// `fastAtan2` is a table, so the angles agree to a few thousandths of a degree.
    fn follows_the_real_squid(water: bool, velocity: &str, yaw: f32) {
        let script = format!(
            "start 0.5 70 0.5 0\nsquid 120 {} {velocity} {yaw} 50\n",
            u8::from(water)
        );
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), 121);
        let mut head = yaw;
        let mut s = Squid::new(out[0].trim().parse().unwrap(), yaw);
        let v: Vec<f64> = velocity.split(' ').map(|v| v.parse().unwrap()).collect();
        let mut vel = [v[0], v[1], v[2]];
        for (tick, line) in out[1..].iter().enumerate() {
            let f: Vec<&str> = line.split(' ').collect();
            if tick == 50 {
                s.set_status(19);
            }
            s.tick(
                &mut vel,
                [f64_of(f[0]), f64_of(f[1])],
                f32_of(f[2]),
                false,
                water,
                &mut head,
            );
            let at = format!("water {water} tick {tick}");
            assert_eq!(vel, [f64_of(f[3]), f64_of(f[4]), f64_of(f[5])], "{at}");
            let want = |i: usize| f32_of(f[i]);
            assert_eq!(s.movement, want(8), "{at}");
            for (got, i, what) in [
                (s.pitch, 6, "pitch"),
                (s.roll, 7, "roll"),
                (s.tentacle, 9, "tentacle"),
                (s.body_yaw, 10, "body yaw"),
                (s.last_body_yaw, 11, "last body yaw"),
                (head, 12, "head yaw"),
            ] {
                assert!(
                    (got - want(i)).abs() < 5e-3,
                    "{at}: {what} {got} vs {}",
                    want(i)
                );
            }
        }
    }

    #[test]
    fn swimming_squid_follows_the_real_one() {
        follows_the_real_squid(true, "0.1 0.05 0.08", 40.0);
        follows_the_real_squid(true, "-0.02 -0.1 0.03", -150.0);
    }

    #[test]
    fn squid_out_of_water_follows_the_real_one() {
        follows_the_real_squid(false, "0.1 0.05 0.08", 40.0);
    }
}
