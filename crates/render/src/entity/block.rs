//! Primed TNT (`PrimedTntRenderer`, object 50) and falling blocks (`FallingBlockRenderer`,
//! object 70): the block model of the state, not the generic box.
//!
//! The constants were read from the real 1.8.9 `PrimedTntRenderer.render` bytecode: lift 0.5,
//! pulse below `fuseTimer - partial + 1 < 10` as `1 + 0.3 * clamp(1 - x/10)^4`, flash alpha
//! `(1 - x/100) * 0.8` while `fuseTimer / 5 % 2 == 0`, and a 90 degree turn about y.
//!
//! shortcut: the flash is the additive `SRC_ALPHA, ONE` pipeline over a copy grown by 0.1 percent
//! instead of vanilla's polygon offset; the falling block takes the light at the entity.

use super::item::{point, Items, Models};
use super::{light_level, Class, EntityDraw, V3};
use crate::hand::{chain, rotate, scale, translate, Mat};

/// `PrimedTntEntity.fuseTimer` at spawn; the 1.8.9 server never sends it.
const FUSE: i32 = 80;
const TNT: i16 = 46;

/// Block models `build` needs: TNT for primed TNT, the state of a falling block.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let key = match d.class {
        Class::Object(50) => Some((TNT, 0)),
        Class::Object(70) => d.stack.map(|s| (s.id, s.damage)),
        _ => None,
    };
    key.into_iter()
}

/// The scale of the pulse for `x = fuseTimer - partial + 1`.
pub fn pulse(x: f32) -> f32 {
    if x >= 10.0 {
        return 1.0;
    }
    let f = (1.0 - x / 10.0).clamp(0.0, 1.0);
    let f = f * f;
    1.0 + f * f * 0.3
}

/// Alpha of the white flash for `x = fuseTimer - partial + 1`.
pub fn flash_alpha(x: f32) -> f32 {
    (1.0 - x / 100.0) * 0.8
}

/// Whether the white flash shows at the integer `fuse_timer`.
pub fn flashing(fuse_timer: i32) -> bool {
    fuse_timer / 5 % 2 == 0
}

/// Meshes every primed TNT and falling block of `draws` into `out`.
pub fn build(
    out: &mut Items,
    draws: &[EntityDraw],
    models: &Models,
    light: impl Fn([f64; 3]) -> [f32; 2],
) {
    for d in draws {
        let Some(key) = wanted(d).next() else {
            continue;
        };
        let Some(Some(m)) = models.get(&key) else {
            continue;
        };
        let level = light_level(d, &light, false);
        let to = |mat: Mat| {
            move |p: V3| {
                let [x, y, z] = point(&mat, p);
                [
                    (d.pos[0] + f64::from(x)) as f32,
                    (d.pos[1] + f64::from(y)) as f32,
                    (d.pos[2] + f64::from(z)) as f32,
                ]
            }
        };
        if d.class == Class::Object(70) {
            out.piece(m, &to(translate(0.0, 0.5, 0.0)), &|n| n, level, false, None);
            continue;
        }
        let ticks = d.age.floor();
        let timer = FUSE - ticks as i32;
        let x = timer as f32 - (d.age - ticks) + 1.0;
        let mat = chain(&[translate(0.0, 0.5, 0.0), scale(pulse(x)), rotate(90.0, 1)]);
        out.piece(m, &to(mat), &|n| n, level, false, None);
        if flashing(timer) {
            let grown = chain(&[mat, scale(1.001)]);
            out.flash(m, &to(grown), flash_alpha(x));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values of `PrimedTntRenderer.render` for `x = fuseTimer - partial + 1`.
    #[test]
    fn pulse_matches_vanilla_at_three_fuses() {
        assert_eq!(pulse(20.0), 1.0);
        assert!((pulse(5.0) - 1.018_75).abs() < 1e-6);
        assert!((pulse(1.0) - 1.196_83).abs() < 1e-5);
        assert!((pulse(-3.0) - 1.3).abs() < 1e-6);
    }

    #[test]
    fn flash_follows_the_integer_timer() {
        assert!(flashing(80) && flashing(4) && flashing(0));
        assert!(!flashing(79) && !flashing(5) && !flashing(9));
        assert!((flash_alpha(81.0) - 0.152).abs() < 1e-6);
    }
}
