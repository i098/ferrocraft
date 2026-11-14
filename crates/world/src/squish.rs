//! Client-side jump squish of a slime or magma cube: `EntitySlime.onUpdate`'s `squishAmount`,
//! `squishFactor` and `prevSquishFactor` (Feather: `targetStretch`, `stretch`, `lastStretch`).
//! The renderer reads [`Squish::factor`] for `RenderSlime`, `RenderMagmaCube` and `ModelMagmaCube`.

/// Squish state of one slime or magma cube.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Squish {
    target: f32,
    stretch: f32,
    last: f32,
    was_on_ground: bool,
}

impl Squish {
    /// The stretch part of `EntitySlime.onUpdate`, with `on_ground` as the tick leaves it: ease
    /// toward the target, kick it on landing (-0.5) or take-off (1.0), then decay it by
    /// `updateStretch` (0.6 for a slime, 0.9 for a magma cube).
    pub fn tick(&mut self, on_ground: bool, magma_cube: bool) {
        self.stretch += (self.target - self.stretch) * 0.5;
        self.last = self.stretch;
        if on_ground && !self.was_on_ground {
            self.target = -0.5;
        } else if !on_ground && self.was_on_ground {
            self.target = 1.0;
        }
        self.was_on_ground = on_ground;
        self.target *= if magma_cube { 0.9 } else { 0.6 };
    }

    /// `prevSquishFactor + (squishFactor - prevSquishFactor) * partial`.
    pub fn factor(&self, partial: f32) -> f32 {
        self.last + (self.stretch - self.last) * partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Harness squish`: the real `SlimeEntity.tick` dropped onto the floor, then kicked into a jump.
    /// The ground flags of the oracle drive the port, and every float must match bit for bit.
    #[test]
    fn easing_matches_the_real_slime_and_magma_cube() {
        for magma in [false, true] {
            let script = format!(
                "floor stone\nstart 0.5 70 0.5 0\nsquish {} 40 25\n",
                u8::from(magma)
            );
            let out = oracle::Oracle::get().run(&script).unwrap();
            assert_eq!(out.len(), 40);
            let mut s = Squish::default();
            for (tick, line) in out.iter().enumerate() {
                let f: Vec<&str> = line.split(' ').collect();
                s.tick(f[0] == "1", magma);
                let bits = |v: f32| format!("{:x}", v.to_bits());
                assert_eq!(
                    (bits(s.stretch), bits(s.last)),
                    (f[1].into(), f[2].into()),
                    "magma {magma} tick {tick}"
                );
            }
        }
    }

    #[test]
    fn landing_squashes_then_recovers() {
        let mut s = Squish::default();
        s.tick(false, false);
        s.tick(true, false);
        s.tick(true, false);
        assert!(s.factor(1.0) < 0.0);
        for _ in 0..30 {
            s.tick(true, false);
        }
        assert!(s.factor(1.0).abs() < 1e-6);
    }
}
