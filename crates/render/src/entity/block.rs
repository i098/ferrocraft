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
use super::{light_level, Class, EntityDraw, V3};
use crate::hand::{chain, rotate, scale, translate, Mat};

/// `PrimedTntEntity.fuseTimer` at spawn; the 1.8.9 server never sends it.
const FUSE: i32 = 80;
const TNT: i16 = 46;

/// Block models `build` needs: TNT for primed TNT, the state of a falling block.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let key = match d.class {
        Class::Object(50) => Some((TNT, 0)),
        // Plants have a flat item model, so they ask for the block form, keeping the metadata.
        Class::Object(70) => d.stack.map(|s| match s.id {
            37..=40 => (s.id, super::mooshroom::BLOCK_FORM - s.damage),
            _ => (s.id, s.damage),
        }),
        Class::Object(id) if super::vehicle::is_cart(id) => super::vehicle::block_model(d),
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
        if let Class::Object(id) = d.class {
            if super::vehicle::is_cart(id) {
                let mat = super::vehicle::block_frame(d);
                out.piece(m, &to(mat), &|n| n, level, None);
                continue;
            }
        }
        if d.class == Class::Object(70) {
            out.piece(m, &to(translate(0.0, 0.5, 0.0)), &|n| n, level, None);
            continue;
        }
        let ticks = d.age.floor();
        let timer = FUSE - ticks as i32;
        let x = timer as f32 - (d.age - ticks) + 1.0;
        let mat = chain(&[translate(0.0, 0.5, 0.0), scale(pulse(x)), rotate(90.0, 1)]);
        out.piece(m, &to(mat), &|n| n, level, None);
        if flashing(timer) {
            let grown = chain(&[mat, scale(1.001)]);
            out.flash(m, &to(grown), 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Atlas;
    use crate::entity::item::{resolve, Stack};
    use crate::hand::DialTurns;

    /// Oracle: `RenderFallingBlock` draws the block model; a flower (id 38) is the crossed planes
    /// (4 quads), and each metadata is its own model.
    #[test]
    fn falling_flowers_are_crossed_planes_keeping_meta() {
        let mut atlas = Atlas::flat();
        atlas.textured = true;
        atlas.cells.insert("flower_rose".to_owned(), 1);
        atlas.cells.insert("flower_blue_orchid".to_owned(), 2);
        let mut meshes = Vec::new();
        for damage in [0, 1] {
            let mut d = crate::entity::tests::steve();
            d.class = Class::Object(70);
            d.stack = Some(Stack {
                id: 38,
                damage,
                count: 1,
                glint: false,
                dye: None,
            });
            let key = wanted(&d).next().unwrap();
            // The plain (id, damage) key is the flat item model; vanilla draws the block form.
            assert_ne!(key, (38, damage));
            assert_eq!(key.0, 38);
            let model =
                resolve(&mut |_| None, &atlas, key.0, key.1, &DialTurns::default()).unwrap();
            let mut models = Models::new();
            models.insert(key, Some(model));
            let mut out = Items::default();
            build(&mut out, &[d], &models, |_| [1.0, 1.0]);
            assert_eq!(out.atlas.vertices.len(), 16);
            meshes.push(out.atlas.vertices);
        }
        assert_ne!(meshes[0], meshes[1]);
    }

    /// Oracle: sapling, flowers and mushrooms (ids 37..=40) are drawn from the block form, not the
    /// flat item model keyed by plain (id, damage); other blocks keep their plain key.
    #[test]
    fn falling_plants_ask_for_the_block_form() {
        for (id, plant) in [(37, true), (38, true), (39, true), (40, true), (1, false)] {
            let mut d = crate::entity::tests::steve();
            d.class = Class::Object(70);
            d.stack = Some(Stack {
                id,
                damage: 0,
                count: 1,
                glint: false,
                dye: None,
            });
            let key = wanted(&d).next().unwrap();
            assert_eq!(key.0, id);
            assert_eq!(key != (id, 0), plant, "id {id}");
        }
    }

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
