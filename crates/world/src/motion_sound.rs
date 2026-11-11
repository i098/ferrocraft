//! Sounds the 1.8.9 client plays for its own player's movement: water contact, swimming and
//! landing. Ported from `Entity.checkWaterCollisions`, `doSplashEffect`, `moveEntity` (swim part),
//! `checkFallDamage` and `LivingEntity.takeFallDamage`.

/// Player hitbox width; the height is passed in.
const WIDTH: f64 = 0.6;
/// `Block.SLIME` id.
const SLIME: u16 = 165;

/// `Entity.checkWaterCollisions`: the hitbox grown by -0.4 below and contracted by 0.001
/// touches a water block whose surface is not below the top of that box (`World.applyLiquidDrag`).
/// `block` returns the block state (`id << 4 | meta`); unloaded space counts as air.
pub fn in_water(pos: [f64; 3], height: f64, block: impl Fn(i32, i32, i32) -> u16) -> bool {
    let half = WIDTH / 2.0;
    let min = [
        pos[0] - half + 0.001,
        pos[1] - 0.4 + 0.001,
        pos[2] - half + 0.001,
    ];
    let max = [
        pos[0] + half - 0.001,
        pos[1] + height - 0.001,
        pos[2] + half - 0.001,
    ];
    let lo = min.map(|v| v.floor() as i32);
    let hi = max.map(|v| (v + 1.0).floor() as i32);
    (lo[0]..hi[0]).any(|x| {
        (lo[1]..hi[1]).any(|y| {
            (lo[2]..hi[2]).any(|z| {
                let state = block(x, y, z);
                matches!(state >> 4, 8 | 9) && max[1] >= f64::from(surface(y, state & 15))
            })
        })
    })
}

/// `(float)(y + 1) - LiquidBlock.getHeightLoss(level)`: level 8 and up is a falling column, full height.
fn surface(y: i32, level: u16) -> f32 {
    let level = if level >= 8 { 0 } else { level };
    (y + 1) as f32 - (f32::from(level) + 1.0) / 9.0
}

/// Volume of the splash on entering water, from the velocity before the move (`doSplashEffect`).
pub fn splash_volume(vel: [f64; 3]) -> f32 {
    speed(vel, 0.2).min(1.0)
}

/// Volume of the swim sound, from the velocity of the move (`Entity.moveEntity`).
pub fn swim_volume(vel: [f64; 3]) -> f32 {
    speed(vel, 0.35).min(1.0)
}

/// `MathHelper.sqrt(vx² * 0.2f + vy² + vz² * 0.2f) * scale`.
fn speed(v: [f64; 3], scale: f32) -> f32 {
    let drag = f64::from(0.2f32);
    ((v[0] * v[0] * drag + v[1] * v[1] + v[2] * v[2] * drag).sqrt() as f32) * scale
}

/// `Entity.fallDistance`, fed once per move with the vertical change and the ground flag.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fall(f32);

impl Fall {
    /// Water, like `checkWaterCollisions`, resets the distance.
    pub fn reset(&mut self) {
        self.0 = 0.0;
    }

    /// `Entity.checkFallDamage`: grows while moving down, and on landing returns the distance fallen.
    pub fn step(&mut self, dy: f64, on_ground: bool) -> Option<f32> {
        if on_ground {
            let d = std::mem::take(&mut self.0);
            (d > 0.0).then_some(d)
        } else {
            if dy < 0.0 {
                self.0 = (f64::from(self.0) - dy) as f32;
            }
            None
        }
    }
}

/// Sound of landing after `distance` blocks on `block` (`LivingEntity.takeFallDamage`, player
/// override of `getFallSound`). Slime absorbs the fall unless the player sneaks.
pub fn landing(distance: f32, block: u16, sneaking: bool) -> Option<&'static str> {
    let multiplier = if block >> 4 == SLIME && !sneaking {
        0.0
    } else {
        1.0
    };
    let damage = ((distance - 3.0) * multiplier).ceil();
    (damage > 0.0).then_some(if damage > 4.0 {
        "game.player.hurt.fall.big"
    } else {
        "game.player.hurt.fall.small"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water(level: u16) -> impl Fn(i32, i32, i32) -> u16 {
        move |_, y, _| if y == 63 { 9 << 4 | level } else { 0 }
    }

    #[test]
    fn water_needs_a_surface_at_or_above_the_box_top() {
        // Level 0 water in cell y=63 has its surface at 63.889.
        assert!(in_water([0.5, 62.5, 0.5], 1.8, water(0)));
        assert!(!in_water([0.5, 61.5, 0.5], 1.8, water(0)));
        assert!(!in_water([0.5, 70.0, 0.5], 1.8, water(0)));
        // A lower surface (level 7: 63.111) is reached from lower down.
        assert!(in_water([0.5, 61.5, 0.5], 1.8, water(7)));
    }

    #[test]
    fn fall_counts_down_moves_and_reports_on_landing() {
        let mut f = Fall::default();
        assert_eq!(f.step(-2.0, false), None);
        assert_eq!(f.step(0.5, false), None);
        assert_eq!(f.step(-3.0, false), None);
        assert_eq!(f.step(0.0, true), Some(5.0));
        assert_eq!(f.step(0.0, true), None);
    }

    #[test]
    fn landing_sound_follows_damage_and_slime() {
        assert_eq!(landing(3.0, 1 << 4, false), None);
        assert_eq!(
            landing(3.5, 1 << 4, false),
            Some("game.player.hurt.fall.small")
        );
        assert_eq!(
            landing(7.0, 1 << 4, false),
            Some("game.player.hurt.fall.small")
        );
        assert_eq!(
            landing(7.5, 1 << 4, false),
            Some("game.player.hurt.fall.big")
        );
        assert_eq!(landing(20.0, SLIME << 4, false), None);
        assert_eq!(
            landing(20.0, SLIME << 4, true),
            Some("game.player.hurt.fall.big")
        );
    }

    #[test]
    fn splash_and_swim_volumes_are_capped() {
        assert_eq!(splash_volume([0.0; 3]), 0.0);
        assert_eq!(splash_volume([0.0, -50.0, 0.0]), 1.0);
        assert!((swim_volume([0.0, 1.0, 0.0]) - 0.35).abs() < 1e-6);
    }
}
