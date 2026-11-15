//! The `Item.isFull3D` branch of `LayerHeldItemWitch`, with the item list read from the 1.8.9 registry.

use crate::hand::{chain, ident, rotate, scale3, translate, Mat};

/// `Item.isWielded` (`setWielded`) in 1.8.9: the sword, shovel, pickaxe, axe and hoe tiers, the
/// stick, bone, fishing rod, blaze rod and carrot on a stick.
pub(super) fn is_wielded(id: i16) -> bool {
    matches!(
        id,
        256..=258 | 267..=280 | 283..=286 | 290..=294 | 346 | 352 | 369 | 398
    )
}

/// `isFlippedWhenWielded`: only the fishing rod and the carrot on a stick.
fn is_flipped(id: i16) -> bool {
    matches!(id, 346 | 398)
}

/// The wielded branch of `LayerHeldItemWitch`: an optional half turn, the nose lift of
/// `RenderWitch.glTranslate`, then the 0.625 scale.
pub(super) fn pose(held: i16) -> Mat {
    let flip = if is_flipped(held) {
        chain(&[rotate(180.0, 2), translate(0.0, -0.0625, 0.0)])
    } else {
        ident()
    };
    chain(&[
        flip,
        translate(0.0, 0.1875, 0.0),
        translate(0.0625, -0.125, 0.0),
        scale3([0.625, -0.625, 0.625]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oracle's `Item.isWielded` list: swords and tools lift by 0.1875 and shrink to 0.625; the
    /// rod and the carrot on a stick turn half a turn first; a bow, a potion and an unlisted item are not wielded.
    #[test]
    fn wielded_items_take_the_full_3d_pose() {
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        let at = |id, p| crate::hand::point(&pose(id), p);
        assert!(near(at(276, [0.0; 3]), [0.0625, 0.0625, 0.0]));
        assert!(near(at(276, [1.0; 3]), [0.6875, -0.5625, 0.625]));
        assert!(near(at(346, [0.0; 3]), [-0.0625, 0.0, 0.0]));
        assert!(near(at(398, [0.0; 3]), [-0.0625, 0.0, 0.0]));
        assert!(!is_wielded(261) && !is_wielded(259) && !is_wielded(287));
        assert!(is_wielded(369) && is_wielded(280) && is_wielded(352));
    }
}
