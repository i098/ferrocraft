//! `LayerMooshroomMushroom` of the mooshroom (mob 96): red mushrooms on the back and the head of an
//! adult. The body is the cow model with `mooshroom.png`.
//!
//! The transforms were read from the real 1.8.9 `MushroomLayer.render` bytecode. The layer draws
//! three `Blocks.red_mushroom` models, not two: two on the back (`translate(0.2, 0.35, 0.5)`,
//! `rotate(42)`; the second `translate(0.1, 0, -0.6)`, `rotate(42)`) and one on the head
//! (`head.transform`, `translate(0, 0.7, -0.2)`, `rotate(12)`). `BlockModelRenderer.render` turns
//! each block 90 degrees about y, and with its `translate(-0.5, -0.5, 0.5)` the block ends up
//! centred on the origin, which is how `held_mesh` builds it.

use super::{model, Class, EntityDraw, V3};
use crate::hand::{chain, rotate, scale3, translate, Mat};

/// The damage that asks for the block model of an id whose item is a flat sprite, as
/// `BlockRenderDispatcher.renderAsItem` draws the plant.
pub(super) const BLOCK_FORM: i16 = -1;
/// `Blocks.red_mushroom` as the block model; its item is a flat sprite.
pub(super) const RED_MUSHROOM: (i16, i16) = (40, BLOCK_FORM);
const MOOSHROOM: u8 = 96;

/// The face of each quad of the red mushroom model, as `BakedQuad.getFace` gives it to
/// `renderModelBrightnessColorQuads`: the planes of `cross.json` turn 45 degrees, and the axis
/// their face keeps the larger part of wins, so the first plane is north and south, the second
/// west and east. The mesh holds the quads in that order.
pub(super) const FACES: [V3; 4] = [
    [0.0, 0.0, -1.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
];

/// What the two lights of `RenderHelper.enableStandardItemLighting` leave of a block drawn at
/// brightness 1 under a world-space normal `n`: ambient 0.4 plus 0.6 per light, each along
/// `(0.2, 1, -0.7)` and `(-0.2, 1, 0.7)`.
pub(crate) fn gl_light(n: V3) -> f32 {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    let n = n.map(|c| c / len);
    let lights = [[0.2_f32, 1.0, -0.7], [-0.2, 1.0, 0.7]];
    let lit: f32 = lights
        .iter()
        .map(|l| {
            let ll = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            ((n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / ll).max(0.0)
        })
        .sum();
    (0.4 + 0.6 * lit).min(1.0)
}

fn shows(d: &EntityDraw) -> bool {
    d.class == Class::Mob(MOOSHROOM) && !d.child && !d.invisible
}

/// The block model `build` needs: the red mushroom, unless the layer draws nothing.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    shows(d).then_some(RED_MUSHROOM).into_iter()
}

/// The three block placements, in model space as GL sees it (pixels over 16): two on the back, then
/// the one on the head. The head part has the given pivot and rotation about x, y and z.
fn placements(head_pivot: V3, head_rot: V3) -> [Mat; 3] {
    let flip = scale3([1.0, -1.0, 1.0]);
    let back = chain(&[flip, translate(0.2, 0.35, 0.5), rotate(42.0, 1)]);
    let turn = rotate(90.0, 1);
    let on_head = chain(&[
        translate(
            head_pivot[0] / 16.0,
            head_pivot[1] / 16.0,
            head_pivot[2] / 16.0,
        ),
        rotate(head_rot[2].to_degrees(), 2),
        rotate(head_rot[1].to_degrees(), 1),
        rotate(head_rot[0].to_degrees(), 0),
        flip,
        translate(0.0, 0.7, -0.2),
        rotate(12.0, 1),
        turn,
    ]);
    [
        chain(&[back, turn]),
        chain(&[back, translate(0.1, 0.0, -0.6), rotate(42.0, 1), turn]),
        on_head,
    ]
}

/// The model-space frames of the mushrooms of `d`, empty when the layer draws nothing.
pub fn frames(d: &EntityDraw) -> Vec<Mat> {
    if !shows(d) {
        return Vec::new();
    }
    let Some(head) = model(d).parts.into_iter().next() else {
        return Vec::new();
    };
    placements(head.pivot, head.rot).to_vec()
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::*;
    use crate::hand::point;

    fn near(a: V3, b: V3) {
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-3), "{a:?} vs {b:?}");
    }

    #[test]
    fn the_mushrooms_sit_where_the_layer_puts_them() {
        let [first, second, head] = placements([0.0, 4.0, -8.0], [0.0; 3]);
        near(point(&first, [0.0; 3]), [0.2, -0.35, 0.5]);
        // The second block: the first's frame, then (0.1, 0, -0.6) turned by 42 degrees.
        near(point(&second, [0.0; 3]), [-0.1272, -0.35, -0.0127]);
        // From the head pivot (0, 0.25, -0.5), the flipped frame puts the block 0.7 lower and 0.2 forward.
        near(point(&head, [0.0; 3]), [0.0, 0.25 - 0.7, -0.5 - 0.2]);
    }

    #[test]
    fn the_gl_light_shades_the_faces_like_vanilla() {
        assert!((gl_light([0.0, 1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((gl_light([0.0, 0.0, -1.0]) - 0.7395).abs() < 1e-3);
        assert!((gl_light([1.0, 0.0, 0.0]) - 0.4969).abs() < 1e-3);
        assert!((gl_light([0.0, -1.0, 0.0]) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_baby_or_invisible_mooshroom_has_no_mushrooms() {
        let mut d = steve();
        d.class = Class::Mob(MOOSHROOM);
        assert_eq!(wanted(&d).collect::<Vec<_>>(), [RED_MUSHROOM]);
        d.child = true;
        assert_eq!(wanted(&d).count(), 0);
        d.child = false;
        d.invisible = true;
        assert_eq!(wanted(&d).count(), 0);
        d.class = Class::Mob(92);
        d.invisible = false;
        assert_eq!(wanted(&d).count(), 0);
    }

    /// Oracle: 1.8.9 `RenderMooshroom` uses `mooshroom.png` on the cow model; `LayerMooshroomMushroom`
    /// draws three red mushrooms on an adult and none on a baby.
    #[test]
    fn the_mooshroom_wears_its_skin_and_three_mushrooms_unless_baby() {
        let mut d = steve();
        d.class = Class::Mob(MOOSHROOM);
        assert_eq!(model(&d).skin, super::super::Skin::Mooshroom);
        d.class = Class::Mob(92);
        assert_ne!(model(&d).skin, super::super::Skin::Mooshroom);
        d.class = Class::Mob(MOOSHROOM);
        assert_eq!(frames(&d).len(), 3);
        d.child = true;
        assert_eq!(model(&d).skin, super::super::Skin::Mooshroom);
        assert!(frames(&d).is_empty());
    }
}
