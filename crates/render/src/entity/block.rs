//! Primed TNT (`PrimedTntRenderer`, object 50) and falling blocks (`FallingBlockRenderer`,
//! object 70): the block model of the state, not the generic box.
//!
//! The constants were read from the real 1.8.9 `PrimedTntRenderer.render` bytecode: lift 0.5,
//! pulse below `fuseTimer - partial + 1 < 10` as `1 + 0.3 * clamp(1 - x/10)^4`, a flash while
//! `fuseTimer / 5 % 2 == 0`, and a 90 degree turn about y. The flash is fully opaque white: the
//! block renderer resets the color alpha to 1, so the `(1 - x/100) * 0.8` set before it is lost.
//!
//! shortcut: the flash is the additive `SRC_ALPHA, ONE` pipeline over a copy grown by 0.1 percent
//! instead of vanilla's polygon offset; the falling block takes the light at the entity.

use super::item::{point, Items, Models};
use super::{light_level, rotate as turn_px, Class, EntityDraw, Xf, V3};
use crate::hand::{chain, rotate, scale, translate, Mat};

/// `PrimedTntEntity.fuseTimer` at spawn; the 1.8.9 server never sends it.
const FUSE: i32 = 80;
const TNT: i16 = 46;

/// Block models `build` needs: TNT for primed TNT, the state of a falling block.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let key = match d.class {
        Class::Object(50) => Some((TNT, 0)),
        Class::Object(70) => d.stack.map(|s| (s.id, s.damage)),
        Class::Object(id) if super::vehicle::is_cart(id) => super::vehicle::block_model(d),
        Class::Mob(super::mobs::ENDERMAN) if !d.invisible && d.look.carried != 0 => {
            Some(((d.look.carried >> 4) as i16, (d.look.carried & 15) as i16))
        }
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

/// Whether the white flash shows at the integer `fuse_timer`.
pub fn flashing(fuse_timer: i32) -> bool {
    fuse_timer / 5 % 2 == 0
}

/// `LayerHeldBlock`: the carried block of an enderman as an 8 pixel cube in front of the chest.
fn carried(out: &mut Items, d: &EntityDraw, m: &super::item::ItemModel, level: [f32; 2]) {
    let xf = Xf::new(d, false);
    let to_world = |p: V3| -> V3 {
        let q = turn_px(
            [-p[0] * 8.0, -p[1] * 8.0 - 1.0, p[2] * 8.0],
            [0.0, 45f32.to_radians(), 0.0],
        );
        let q = turn_px(q, [20f32.to_radians(), 0.0, 0.0]);
        xf.world([q[0], q[1] + 11.0, q[2] - 12.0])
    };
    let origin = to_world([0.0; 3]);
    let normal = |n: V3| {
        let t = to_world(n);
        [t[0] - origin[0], t[1] - origin[1], t[2] - origin[2]]
    };
    out.gl_lit = true;
    out.piece(m, &to_world, &normal, level, false, None);
    out.gl_lit = false;
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
        if matches!(d.class, Class::Mob(_)) {
            carried(out, d, m, level);
            continue;
        }
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
        if let Class::Object(id) = d.class {
            if super::vehicle::is_cart(id) {
                let mat = super::vehicle::block_frame(d);
                out.piece(m, &to(mat), &|n| n, level, false, None);
                continue;
            }
        }
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
            out.flash(m, &to(grown), 1.0);
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
    }
}
