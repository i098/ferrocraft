//! Wing flap of a chicken: the `onLivingUpdate` state of `EntityChicken` (Feather: `oFlap`, `flap`,
//! `oFlapSpeed`, `flapSpeed`, `flapProgress`) and `RenderChicken.handleRotationFloat`, which the
//! model reads as the wing angle.

/// Flap state of one chicken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flap {
    wing_rotation: f32,
    old_wing_rotation: f32,
    dest_pos: f32,
    old_dest_pos: f32,
    wing_rot_delta: f32,
}

impl Default for Flap {
    fn default() -> Flap {
        Flap {
            wing_rotation: 0.0,
            old_wing_rotation: 0.0,
            dest_pos: 0.0,
            old_dest_pos: 0.0,
            wing_rot_delta: 1.0,
        }
    }
}

impl Flap {
    /// The flap part of `EntityChicken.onLivingUpdate`, with `on_ground` as the tick leaves it.
    pub fn tick(&mut self, on_ground: bool) {
        self.old_wing_rotation = self.wing_rotation;
        self.old_dest_pos = self.dest_pos;
        self.dest_pos += if on_ground { -1.0 } else { 4.0 } * 0.3;
        self.dest_pos = self.dest_pos.clamp(0.0, 1.0);
        if !on_ground && self.wing_rot_delta < 1.0 {
            self.wing_rot_delta = 1.0;
        }
        self.wing_rot_delta *= 0.9;
        self.wing_rotation += self.wing_rot_delta * 2.0;
    }

    /// `RenderChicken.handleRotationFloat`: `(sin(flap) + 1) * flapSpeed`, eased by the partial tick.
    /// `sin` is the caller's `MathHelper.sin`.
    pub fn angle(&self, partial: f32, sin: impl Fn(f32) -> f32) -> f32 {
        let flap = self.old_wing_rotation + (self.wing_rotation - self.old_wing_rotation) * partial;
        let speed = self.old_dest_pos + (self.dest_pos - self.old_dest_pos) * partial;
        (sin(flap) + 1.0) * speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_chicken_holds_its_wings_down() {
        let mut f = Flap::default();
        for _ in 0..5 {
            f.tick(true);
        }
        assert_eq!(f.angle(1.0, f32::sin), 0.0);
    }

    #[test]
    fn airborne_chicken_spreads_its_wings() {
        let mut f = Flap::default();
        for _ in 0..4 {
            f.tick(false);
        }
        // destPos saturates at 1 after one airborne tick, so the angle is sin(flap) + 1 in 0..2.
        assert!(f.dest_pos == 1.0 && f.wing_rot_delta < 1.0);
        let a = f.angle(0.5, f32::sin);
        assert!((0.0..=2.0).contains(&a) && a > 0.0);
        // Landing drains destPos by 0.3 per tick.
        f.tick(true);
        assert!((f.dest_pos - 0.7).abs() < 1e-6);
    }
}
