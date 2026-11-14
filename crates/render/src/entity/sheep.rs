//! The grass-eating head of the sheep, ported from `EntitySheep.getHeadRotationPointY` and
//! `getHeadRotationAngleX` and read by `ModelSheep1/2.setLivingAnimations` and `setRotationAngles`.

use super::{mobs::head_rot, EntityDraw};
use std::f32::consts::PI;

/// `getHeadRotationPointY`: how far the head dips, 0 to 1.
fn dip(timer: i32, partial: f32) -> f32 {
    let t = timer as f32;
    match timer {
        ..=0 => 0.0,
        4..=36 => 1.0,
        1..=3 => (t - partial) / 4.0,
        _ => -(t - 40.0 - partial) / 4.0,
    }
}

/// `getHeadRotationAngleX`, or `None` when the sheep is not eating (the head then follows the pitch).
fn nod(timer: i32, partial: f32) -> Option<f32> {
    match timer {
        ..=0 => None,
        5..=36 => {
            let f = (timer as f32 - 4.0 - partial) / 32.0;
            Some(PI / 5.0 + PI * 7.0 / 100.0 * (f * 28.7).sin())
        }
        _ => Some(PI / 5.0),
    }
}

/// Pivot and angles of the sheep head (`headRotationPointY = 6 + dip * 9`, then `rotateAngleX`).
pub(super) fn head(d: &EntityDraw) -> ([f32; 3], [f32; 3]) {
    let (timer, partial) = (d.look.eat_timer, d.look.partial);
    let mut rot = head_rot(d);
    if let Some(x) = nod(timer, partial) {
        rot[0] = x;
    }
    ([0.0, 6.0 + dip(timer, partial) * 9.0, -8.0], rot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_dips_in_and_out_around_the_flat_middle() {
        assert_eq!(dip(0, 0.5), 0.0);
        assert_eq!(dip(20, 0.5), 1.0);
        assert_eq!(dip(40, 0.0), 0.0);
        assert_eq!(dip(2, 0.0), 0.5);
        assert_eq!(dip(38, 0.0), 0.5);
    }

    #[test]
    fn head_holds_down_at_the_ends_and_bobs_in_the_middle() {
        assert_eq!(nod(0, 0.0), None);
        assert_eq!(nod(4, 0.3), Some(PI / 5.0));
        assert_eq!(nod(40, 0.3), Some(PI / 5.0));
        let bob = nod(20, 0.0).unwrap();
        assert!((bob - (PI / 5.0 + PI * 7.0 / 100.0 * (16.0f32 / 32.0 * 28.7).sin())).abs() < 1e-6);
    }
}
