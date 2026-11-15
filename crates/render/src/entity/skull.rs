//! Wither skulls (object 66). Ported from `WitherSkullRenderer.render` (`RenderWitherSkull`) and
//! `SkullModel` (`ModelSkeletonHead`) of the real 1.8.9 client, read from the bytecode: the
//! renderer builds `SkullModel()`, which is `(0, 35, 64, 64)`, so one 8x8x8 head box sits at
//! texture offset (0, 35) of a 64x64 texture. It is turned by the interpolated yaw and pitch and
//! drawn under `scale(-1, -1, 1)` with culling off. The blue skull (`isCharged`, metadata byte 10)
//! uses `wither_invulnerable.png`. The entity has no brightness override, so it is lit normally.

use super::{add_box, skin_dims, EntityDraw, Skin, Skins, V3};
use crate::hand::{chain, point, rotate};
use crate::mesh::MeshData;

/// Object type of a wither skull.
pub const ID: i8 = 66;

/// The skull texture: blue when the skull is `invulnerable`.
fn skin(blue: bool) -> Skin {
    if blue {
        Skin::WitherInvulnerable
    } else {
        Skin::Wither
    }
}

/// Maps a head-model point in pixels to world blocks: `ModelRenderer.render` turns by yaw (Y)
/// then pitch (X), then the renderer's `scale(-1, -1, 1)` and the 1/16 pixel scale apply.
fn place(d: &EntityDraw, p: V3) -> V3 {
    let m = chain(&[rotate(d.yaw, 1), rotate(d.pitch, 0)]);
    let q = point(&m, p);
    [-q[0] / 16.0, -q[1] / 16.0, q[2] / 16.0]
}

/// The skull, textured and placed in the world.
pub fn build(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let s = skin(d.look.invulnerable);
    let color = if skins.has(s) {
        [1.0; 3]
    } else {
        [0.6, 0.6, 0.6]
    };
    let v_off = skins.v_offset(s);
    let [tw, th] = skin_dims();
    let tf = |p: V3| -> V3 {
        let q = place(d, p);
        [0, 1, 2].map(|i| (d.pos[i] + f64::from(q[i])) as f32)
    };
    let size = [8.0; 3];
    add_box(
        out,
        &tf,
        [-4.0, -8.0, -4.0],
        size,
        size,
        (0.0, 35.0),
        false,
        v_off,
        color,
        [tw, th],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::tests::steve;
    use crate::entity::Class;

    fn bounds(yaw: f32, pitch: f32) -> ([f32; 3], [f32; 3]) {
        let mut d = steve();
        d.class = Class::Object(66);
        d.pos = [0.0; 3];
        d.yaw = yaw;
        d.pitch = pitch;
        let mut out = MeshData::default();
        build(&mut out, &d, &Skins::flat());
        assert_eq!(out.vertices.len(), 24);
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &out.vertices {
            for i in 0..3 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
        (lo, hi)
    }

    /// Unturned, the 8 px head spans half a block, hanging from the model's y = 0 to y = 8 px
    /// after the y flip: it sits from the entity position up by half a block.
    #[test]
    fn the_skull_is_a_half_block_head() {
        let (lo, hi) = bounds(0.0, 0.0);
        assert!((lo[0] + 0.25).abs() < 1e-5 && (hi[0] - 0.25).abs() < 1e-5);
        assert!(lo[1].abs() < 1e-5 && (hi[1] - 0.5).abs() < 1e-5);
    }

    /// Yaw and pitch turn the head about its origin, so the bounds change.
    #[test]
    fn the_skull_turns_with_yaw_and_pitch() {
        assert_ne!(bounds(0.0, 0.0), bounds(45.0, 30.0));
    }
}
