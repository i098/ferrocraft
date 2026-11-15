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
mod tests;
