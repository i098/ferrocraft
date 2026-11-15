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
            .scale([1.0, -1.0, -1.0]);
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

    /// Vanilla `renderByItem` flips the model (translate(0,1,1), scale(1,-1,-1)): the shut lid sits
    /// on top. Block-space oracle from `ModelChest`: body y 6..16, lid top y 2 (of 16), so after the
    /// flip the body spans 0..0.625 and the lid ends at 0.875, times the cart's 0.75 block scale.
    #[test]
    fn carried_chest_stands_upright_with_lid_on_top() {
        let batches = build_cart_chests(&[cart(10, CHEST << 4)], |_| [1.0, 0.0]);
        let v = &batches[0].1.vertices;
        let ys = || v.iter().map(|p| p.pos[1]);
        let min = ys().fold(f32::MAX, f32::min);
        let max = ys().fold(f32::MIN, f32::max);
        assert!((max - min - 0.875 * 0.75).abs() < 1e-4, "height {}", max - min);
        assert!(ys().any(|y| (y - min - 0.625 * 0.75).abs() < 1e-4), "body top");
        assert!(!ys().any(|y| (y - min - 0.25 * 0.75).abs() < 1e-4), "inverted body");
    }

    /// Vanilla frame at yaw 0: rotate 180 about Y, scale 0.75, rotate 90 about Y, chest z flipped.
    /// The latch (block z 15/16..1) lands on the -x side, outside the body's +-0.328 footprint.
    #[test]
    fn carried_chest_latch_faces_minus_x_at_yaw_zero() {
        let batches = build_cart_chests(&[cart(10, CHEST << 4)], |_| [1.0, 0.0]);
        let xs = || batches[0].1.vertices.iter().map(|p| p.pos[0] - 10.0);
        let (min, max) = (xs().fold(f32::MAX, f32::min), xs().fold(f32::MIN, f32::max));
        assert!((min + 0.375).abs() < 5e-3, "latch side {min}");
        assert!((max - 0.328125).abs() < 5e-3, "body far side {max}");
    }

    #[test]
    fn carried_chest_uses_its_own_texture() {
        let tex = |id: u16| build_cart_chests(&[cart(10, id << 4)], |_| [1.0, 0.0])[0].0.clone();
        let named = |n: &str| Tex::Named(format!("textures/entity/chest/{n}.png"));
        assert_eq!(tex(CHEST), named("normal"));
        assert_eq!(tex(ENDER_CHEST), named("ender"));
        assert_eq!(tex(crate::blockent::TRAPPED_CHEST), named("trapped"));
    }
}
