//! The turn of a living entity's body (`renderYawOffset`, Feather `bodyYaw`) toward its movement
//! and its head, ported from `LivingEntity.tick` / `bodyMovement` (players) and `BodyControl.tick`
//! (mobs).

/// `MathHelper.wrapDegrees`.
pub fn wrap_degrees(v: f32) -> f32 {
    let mut v = v % 360.0;
    if v >= 180.0 {
        v -= 360.0;
    }
    if v < -180.0 {
        v += 360.0;
    }
    v
}

/// `BodyControl.clampAndWrapAngle`: `target` less the offset to `from`, kept within `bound`.
pub fn clamp_angle(target: f32, from: f32, bound: f32) -> f32 {
    target - wrap_degrees(target - from).clamp(-bound, bound)
}

/// The yaw the body aims at in `LivingEntity.tick`: the move direction once the entity moved more
/// than 0.05 blocks, else the current body yaw; the look yaw while an arm swing runs.
/// `moved` is `[x, z]` of `pos - prevPos`.
pub fn player_aim(body_yaw: f32, yaw: f32, moved: [f64; 2], swinging: bool) -> f32 {
    let dist2 = (moved[0] * moved[0] + moved[1] * moved[1]) as f32;
    let mut aim = body_yaw;
    if dist2 > 0.002_500_000_2 {
        // `MathHelper.atan2` is a table approximation of `atan2`; the difference is far below a pixel.
        aim = (moved[1].atan2(moved[0]) as f32) * 180.0 / std::f32::consts::PI - 90.0;
    }
    if swinging {
        aim = yaw;
    }
    aim
}

/// `LivingEntity.bodyMovement`: ease the body 0.3 of the way to `aim`, keep it within 75 degrees of
/// the look `yaw` and push it a further 0.2 of the way once past 50 degrees.
pub fn body_movement(body_yaw: f32, yaw: f32, aim: f32) -> f32 {
    let mut body = body_yaw + wrap_degrees(aim - body_yaw) * 0.3;
    let off = wrap_degrees(yaw - body).clamp(-75.0, 75.0);
    body = yaw - off;
    if off * off > 2500.0 {
        body += off * 0.2;
    }
    body
}

/// `BodyControl`: what a mob (`MobEntity`) keeps between ticks to turn its body.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyControl {
    active_ticks: i32,
    last_head_yaw: f32,
}

impl BodyControl {
    /// `BodyControl.tick`: a moving mob takes its yaw as the body yaw and drags the head within 75
    /// degrees of it; a still one turns the body toward the head, the more slowly the longer the head
    /// rests. `moved` is `[x, z]` of `pos - prevPos`.
    pub fn tick(
        &mut self,
        rotation_yaw: f32,
        moved: [f64; 2],
        body_yaw: &mut f32,
        head_yaw: &mut f32,
    ) {
        if moved[0] * moved[0] + moved[1] * moved[1] > 2.500_000_277_905_201e-7 {
            *body_yaw = rotation_yaw;
            *head_yaw = clamp_angle(*body_yaw, *head_yaw, 75.0);
            self.last_head_yaw = *head_yaw;
            self.active_ticks = 0;
            return;
        }
        let mut bound = 75.0;
        if (*head_yaw - self.last_head_yaw).abs() > 15.0 {
            self.active_ticks = 0;
            self.last_head_yaw = *head_yaw;
        } else {
            self.active_ticks += 1;
            if self.active_ticks > 10 {
                bound = (1.0 - (self.active_ticks - 10) as f32 / 10.0).max(0.0) * 75.0;
            }
        }
        *body_yaw = clamp_angle(*head_yaw, *body_yaw, bound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_of(hex: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
    }

    /// `Harness body`: the real `PlayerEntity.tick` on a strafe path. The player strafes east at 0.2
    /// blocks a tick while it looks south, turns its look to the west, stops, then swings its arm. The
    /// body yaw and its previous value must follow the real ones every tick. `atan2` is a table in
    /// `MathHelper`, so the angles agree to a few thousandths of a degree.
    #[test]
    fn follows_the_real_player_on_a_strafe_path() {
        const TICKS: usize = 80;
        const SWING: usize = 60;
        let (mut x, mut yaw) = (0.5_f64, 40.0_f32);
        let mut path = Vec::new();
        for t in 0..TICKS {
            if t < 30 {
                x += 0.2;
            }
            if (15..45).contains(&t) {
                yaw += 5.0;
            }
            path.push((x, yaw));
        }
        let steps: String = path
            .iter()
            .map(|(x, yaw)| format!(" {x} 0.5 {yaw}"))
            .collect();
        let script = format!("start 0.5 70 0.5 0\nbody {SWING} {TICKS}{steps}\n");
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), TICKS + 1);
        let mut body = f32_of(out[0].trim());
        let mut prev_x = path[0].0;
        for (t, ((x, yaw), line)) in path.iter().zip(&out[1..]).enumerate() {
            // `swingProgress` is still 0 on the tick of `swingArm`, so the swing aim starts one tick later.
            let swinging = (SWING + 1..=SWING + 5).contains(&t);
            let aim = player_aim(body, *yaw, [x - prev_x, 0.0], swinging);
            let last = body;
            body = body_movement(body, *yaw, aim);
            prev_x = *x;
            let f: Vec<&str> = line.split(' ').collect();
            for (got, want, what) in [(body, f32_of(f[0]), "body"), (last, f32_of(f[1]), "last")] {
                assert!(
                    (got - want).abs() < 5e-3,
                    "tick {t} {what}: {got} vs {want}"
                );
            }
        }
    }
}
