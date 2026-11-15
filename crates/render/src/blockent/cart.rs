//! The chest a minecart carries. `MinecartRenderer.renderBlockInMinecart` draws a block of render
//! type 2 through `TileEntityRendererChestHelper.renderByItem`, which runs the chest renderer for no
//! tile: a single chest facing the default way with its lid shut (the ender chest and the trapped
//! chest take their own texture). The block frame is that of any carried block, `vehicle::block_frame`.

use super::{emit_chest, Batches, Mat, Tex};
use crate::entity::{vehicle, EntityDraw};
use crate::hand;
use crate::mesh::MeshData;

/// The chest model of every cart in `draws` that carries a chest, ender chest or trapped chest.
pub fn build_cart_chests(
    draws: &[EntityDraw],
    light: impl Fn([f64; 3]) -> [f32; 2],
) -> Vec<(Tex, MeshData)> {
    let mut out = Batches::default();
    for d in draws {
        let Some(id) = vehicle::carried_chest(d) else {
            continue;
        };
        // `block_frame` maps a model centred on the origin; the chest model is in block
        // coordinates, with the corner at the origin.
        let frame = hand::chain(&[vehicle::block_frame(d), hand::translate(-0.5, -0.5, -0.5)]);
        let mark = out.mark();
        let m = from_hand(&frame, d.pos)
            .translate([0.0, 1.0, 1.0])
            .scale([1.0, -1.0, -1.0])
            .translate([0.5, 0.5, 0.5])
            .translate([-0.5, -0.5, -0.5]);
        emit_chest(&mut out, id, false, &m, 0.0);
        out.light_since(mark, crate::entity::light_level(d, &light, false));
    }
    out.0
}

/// A column-major `hand::Mat` as a matrix placed in the world at `origin`.
fn from_hand(m: &hand::Mat, origin: [f64; 3]) -> Mat {
    Mat(std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c == 3 {
                m[3][r] + origin[r] as f32
            } else {
                m[c][r]
            }
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockent::{CHEST, ENDER_CHEST};
    use crate::entity::{tests::steve, Class};

    fn cart(kind: i8, block: u16) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Object(kind);
        d.look.vehicle.block = block;
        d
    }

    /// A chest cart gets the three boxes of `ModelChest`; other blocks and other objects get none.
    #[test]
    fn only_a_cart_with_a_chest_draws_the_chest_model() {
        let light = |_: [f64; 3]| [1.0, 0.0];
        let batches = build_cart_chests(&[cart(10, ENDER_CHEST << 4)], light);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].1.vertices.len(), 3 * 24);
        assert!(build_cart_chests(&[cart(10, 46 << 4), cart(50, CHEST << 4)], light).is_empty());
    }
}
