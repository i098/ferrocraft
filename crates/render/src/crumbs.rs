//! Food crumbs of `PlayerEntity.onConsumeItem` (1.8.9, EAT action): the `ITEM_CRACK` particles that
//! fly from the mouth while a player eats.

use crate::jrandom::JavaRandom;
use world::{cos, sin};

/// One crumb: where it appears and the motion given to the particle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crumb {
    pub at: [f64; 3],
    pub motion: [f64; 3],
}

/// `Vec3d.rotateX` / `rotateY` with `MathHelper` table trig.
fn rotate(v: [f64; 3], pitch: f32, yaw: f32) -> [f64; 3] {
    let angle = |degrees: f32| -degrees * std::f32::consts::PI / 180.0_f32;
    let (c, s) = (f64::from(cos(angle(pitch))), f64::from(sin(angle(pitch))));
    let v = [v[0], v[1] * c + v[2] * s, v[2] * c - v[1] * s];
    let (c, s) = (f64::from(cos(angle(yaw))), f64::from(sin(angle(yaw))));
    [v[0] * c + v[2] * s, v[1], v[2] * c - v[0] * s]
}

/// The `count` crumbs of one `onConsumeItem` call for a player at `feet` looking along `yaw`, `pitch`.
/// `rng` is the entity random (`nextFloat`: motion x, mouth y, mouth x, in that order); `math_random`
/// is `Math.random()`, the motion's rise.
pub fn crumbs(
    count: u32,
    feet: [f64; 3],
    eye_height: f32,
    (yaw, pitch): (f32, f32),
    rng: &mut JavaRandom,
    mut math_random: impl FnMut() -> f64,
) -> Vec<Crumb> {
    (0..count)
        .map(|_| {
            let vx = (f64::from(rng.next_float()) - 0.5) * 0.1;
            let rise = math_random() * 0.1 + 0.1;
            let motion = rotate([vx, rise, 0.0], pitch, yaw);
            let y = -f64::from(rng.next_float()) * 0.6 - 0.3;
            let x = (f64::from(rng.next_float()) - 0.5) * 0.3;
            let mouth = rotate([x, y, 0.6], pitch, yaw);
            Crumb {
                at: [
                    mouth[0] + feet[0],
                    mouth[1] + (feet[1] + f64::from(eye_height)),
                    mouth[2] + feet[2],
                ],
                motion: [motion[0], motion[1] + 0.05, motion[2]],
            }
        })
        .collect()
}

/// `Entity.tick` of a remote player: `itemUseTimer` when the use started at tick 0 (the flag first
/// seen), as `RemoteClientPlayerEntity.tick` sets it after `PlayerEntity.tick` ran. Crumbs fall on
/// `itemUseTimer <= 25 && itemUseTimer % 4 == 0`; the client never finishes the use, so it goes on
/// below zero while the flag stays.
pub fn remote_due(ticks_in_use: u32) -> bool {
    let timer = 33 - i64::from(ticks_in_use);
    ticks_in_use >= 1 && timer <= 25 && timer % 4 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_crumbs_fall_every_fourth_tick_from_timer_24() {
        let due: Vec<u32> = (0..40).filter(|&n| remote_due(n)).collect();
        assert_eq!(due, [9, 13, 17, 21, 25, 29, 33, 37]);
    }

    #[test]
    fn five_crumbs_in_front_of_the_face_when_looking_along_z() {
        let mut rng = JavaRandom::new(7);
        let out = crumbs(5, [10.0, 64.0, 20.0], 1.62, (0.0, 0.0), &mut rng, || 0.5);
        assert_eq!(out.len(), 5);
        for c in out {
            assert!((c.at[2] - 20.6).abs() < 1e-3, "0.6 in front");
            assert!((c.at[0] - 10.0).abs() <= 0.15 + 1e-3);
            assert!(c.at[1] <= 65.62 - 0.3 + 1e-3 && c.at[1] >= 65.62 - 0.9 - 1e-3);
            assert!((c.motion[1] - 0.2).abs() < 1e-3);
        }
    }

    /// Runs the real `PlayerEntity.onConsumeItem` and compares the particles it adds.
    #[test]
    fn crumbs_match_the_real_on_consume_item() {
        let mut script = String::new();
        let mut cases = Vec::new();
        for (yaw, pitch, seed) in [
            (0.0, 0.0, 1),
            (90.0, 30.0, 2),
            (-135.5, -45.0, 3),
            (200.0, 60.0, 4),
        ] {
            script.push_str(&format!(
                "start 3.5 70 -2.25 0\neat {yaw} {pitch} {seed} 5 260\n"
            ));
            cases.push((yaw, pitch, seed));
        }
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), cases.len() * 5, "{out:?}");
        for (case, lines) in cases.iter().zip(out.chunks(5)) {
            let (yaw, pitch, seed) = *case;
            let feet = [3.5, 70.0, -2.25];
            // The rise is `Math.random()`, which the JVM seeds itself: solve it from the real motion y.
            let at = |rise: f64| {
                crumbs(
                    5,
                    feet,
                    1.62,
                    (yaw, pitch),
                    &mut JavaRandom::new(seed),
                    || rise,
                )
            };
            let (a, b) = (at(0.0), at(1.0));
            for (i, line) in lines.iter().enumerate() {
                let v: Vec<f64> = line
                    .split_whitespace()
                    .map(|f| f.parse().unwrap())
                    .collect();
                assert_eq!(v.len(), 6, "{line}");
                let rise = (v[4] - a[i].motion[1]) / (b[i].motion[1] - a[i].motion[1]);
                assert!(
                    (-1e-6..=1.0 + 1e-6).contains(&rise),
                    "Math.random() out of range: {rise}"
                );
                for k in 0..3 {
                    assert_eq!(v[k], a[i].at[k], "mouth {k} of {line}");
                    let want = a[i].motion[k] + rise * (b[i].motion[k] - a[i].motion[k]);
                    assert!((v[3 + k] - want).abs() < 1e-9, "motion {k} of {line}");
                }
            }
        }
    }
}
