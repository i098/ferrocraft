//! The witch (mob 66), ported from the 1.8.9 `ModelWitch`, `RenderWitch` and `LayerHeldItemWitch`.
//! Every constant was read from the `WitchModel` and `WitchItemInHandLayer` bytecode; the part
//! layout is checked against `harness/mobmodels.sh`.

use super::mobs::{model, villager};
use super::wielded;
use super::{EntityDraw, Model, Part, Skin};
use crate::hand::{chain, rotate, scale3, translate, Mat};
use std::f32::consts::PI;

pub(super) const WITCH: u8 = 66;
const BOW: i16 = 261;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == WITCH).then(|| witch(d))
}

/// The nose after `ModelWitch.setRotationAngles`: its rotation, and the `translateX/Y/Z` offset in
/// model pixels (blocks times 16). An item in the hand tilts it and moves it forward and down.
struct Nose {
    rot: [f32; 3],
    offset: [f32; 3],
}

/// The nose rests at pivot `(0, -2, 0)` under the head.
const NOSE_PIVOT: [f32; 3] = [0.0, -2.0, 0.0];

fn nose(d: &EntityDraw) -> Nose {
    let speed = 0.01 * (d.look.id % 10) as f32;
    let ticks = d.age.floor();
    let mut n = Nose {
        rot: [
            (ticks * speed).sin() * 4.5 * PI / 180.0,
            0.0,
            (ticks * speed).cos() * 2.5 * PI / 180.0,
        ],
        offset: [0.0; 3],
    };
    if d.held != 0 {
        n.rot[0] = -0.9;
        n.offset = [0.0, 0.1875 * 16.0, -0.093_75 * 16.0];
    }
    n
}

/// `ModelWitch`: the villager with the nose carrying a mole and a hat of four boxes on the head.
/// The head is part 0, as the villager has it.
fn witch(d: &EntityDraw) -> Model {
    let mut parts = villager(d).parts;
    let head = (parts[0].pivot, parts[0].rot);
    let n = nose(d);
    let nose_pivot = [0, 1, 2].map(|i| NOSE_PIVOT[i] + n.offset[i]);
    let under_nose = vec![(nose_pivot, n.rot), head];
    parts[1] = Part::new(nose_pivot, [-1.0, -1.0, -6.0], [2.0, 4.0, 2.0], (24.0, 0.0))
        .rot(n.rot)
        .under(vec![head]);
    let mut mole = Part::new(NOSE_PIVOT, [0.0, 3.0, -6.75], [1.0; 3], (0.0, 0.0)).under(under_nose);
    mole.inflate = -0.25;
    parts.push(mole);
    parts.extend(hat(head));
    model(Skin::Witch, parts)
}

/// The brim and the three boxes that stack on it, each a child of the one below.
fn hat(head: ([f32; 3], [f32; 3])) -> Vec<Part> {
    let brim = ([-5.0, -10.031_25, -5.0], [0.0; 3]);
    let tiers = [
        ([1.75, -4.0, 2.0], [-0.052_359_88, 0.0, 0.026_179_94]),
        ([1.75, -4.0, 2.0], [-0.104_719_76, 0.0, 0.052_359_88]),
        ([1.75, -2.0, 2.0], [-0.209_439_52, 0.0, 0.104_719_76]),
    ];
    let boxes = [
        ([7.0, 4.0, 7.0], (0.0, 76.0), 0.0),
        ([4.0; 3], (0.0, 87.0), 0.0),
        ([1.0, 2.0, 1.0], (0.0, 95.0), 0.25),
    ];
    let mut parents = vec![brim, head];
    let mut out =
        vec![Part::new(brim.0, [0.0; 3], [10.0, 2.0, 10.0], (0.0, 64.0)).under(vec![head])];
    for ((pivot, rot), (size, uv, inflate)) in tiers.into_iter().zip(boxes) {
        let mut tier = Part::new(pivot, [0.0; 3], size, uv)
            .rot(rot)
            .under(parents.clone());
        tier.inflate = inflate;
        out.push(tier);
        parents.insert(0, (pivot, rot));
    }
    out
}

/// `LayerHeldItemWitch`, up to `ItemInHandRenderer.render`: from the nose pivot and rotation
/// (`ModelRenderer.postRender` skips the translate offset and the head), then the pose of the
/// item kind. `block3d` is a chest (`isRenderTypeChest`).
pub(super) fn hand(d: &EntityDraw, block3d: bool) -> Mat {
    let n = nose(d);
    let seat = chain(&[
        translate(
            NOSE_PIVOT[0] / 16.0,
            NOSE_PIVOT[1] / 16.0,
            NOSE_PIVOT[2] / 16.0,
        ),
        rotate(n.rot[2].to_degrees(), 2),
        rotate(n.rot[1].to_degrees(), 1),
        rotate(n.rot[0].to_degrees(), 0),
        translate(-0.0625, 0.531_25, 0.218_75),
    ]);
    chain(&[
        seat,
        item_pose(d.held, block3d),
        rotate(-15.0, 0),
        rotate(40.0, 2),
    ])
}

/// The pose of the item kind: a 3D block, a bow, or anything else (a potion).
fn item_pose(held: i16, block3d: bool) -> Mat {
    if block3d {
        chain(&[
            translate(0.0, 0.0625, -0.25),
            rotate(30.0, 0),
            rotate(-5.0, 1),
            scale3([0.375, -0.375, 0.375]),
        ])
    } else if held == BOW {
        chain(&[
            translate(0.0, 0.125, -0.125),
            rotate(-45.0, 1),
            scale3([0.625, -0.625, 0.625]),
            rotate(-100.0, 0),
            rotate(-20.0, 1),
        ])
    } else if wielded::is_wielded(held) {
        wielded::pose(held)
    } else {
        chain(&[
            translate(0.1875, 0.1875, 0.0),
            scale3([0.875; 3]),
            rotate(-20.0, 2),
            rotate(-60.0, 0),
            rotate(-30.0, 2),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, Class};
    use super::*;

    fn witch_draw() -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(WITCH);
        d
    }

    type Row = (
        [f32; 3],
        [f32; 3],
        [f32; 3],
        (f32, f32),
        [f32; 3],
        f32,
        usize,
    );

    fn rows(d: &EntityDraw) -> Vec<Row> {
        drawn(d)
            .parts
            .iter()
            .map(|p| {
                (
                    p.pivot,
                    p.min,
                    p.size,
                    p.uv,
                    p.rot,
                    p.inflate,
                    p.parents.len(),
                )
            })
            .collect()
    }

    /// Oracle: `WitchModel` from `harness/mobmodels.sh`, head and nose at rest; the hat tiers
    /// carry their parents in order.
    #[test]
    fn the_parts_match_the_oracle_layout() {
        let mut d = witch_draw();
        d.limb_swing_amount = 0.0;
        let r = rows(&d);
        assert_eq!(drawn(&d).skin, Skin::Witch);
        let nose = r[1];
        assert_eq!(
            (nose.0, nose.1, nose.2, nose.3, nose.6),
            (
                [0.0, -2.0, 0.0],
                [-1.0, -1.0, -6.0],
                [2.0, 4.0, 2.0],
                (24.0, 0.0),
                1
            )
        );
        let tail: Vec<_> = r[r.len() - 5..].to_vec();
        assert_eq!(
            tail,
            [
                (
                    [0.0, -2.0, 0.0],
                    [0.0, 3.0, -6.75],
                    [1.0; 3],
                    (0.0, 0.0),
                    [0.0; 3],
                    -0.25,
                    2
                ),
                (
                    [-5.0, -10.031_25, -5.0],
                    [0.0; 3],
                    [10.0, 2.0, 10.0],
                    (0.0, 64.0),
                    [0.0; 3],
                    0.0,
                    1
                ),
                (
                    [1.75, -4.0, 2.0],
                    [0.0; 3],
                    [7.0, 4.0, 7.0],
                    (0.0, 76.0),
                    [-0.052_359_88, 0.0, 0.026_179_94],
                    0.0,
                    2
                ),
                (
                    [1.75, -4.0, 2.0],
                    [0.0; 3],
                    [4.0; 3],
                    (0.0, 87.0),
                    [-0.104_719_76, 0.0, 0.052_359_88],
                    0.0,
                    3
                ),
                (
                    [1.75, -2.0, 2.0],
                    [0.0; 3],
                    [1.0, 2.0, 1.0],
                    (0.0, 95.0),
                    [-0.209_439_52, 0.0, 0.104_719_76],
                    0.25,
                    4
                ),
            ]
        );
    }

    /// A held item tilts the nose by -0.9 and moves it by `(0, 0.1875, -0.09375)` blocks.
    #[test]
    fn a_held_potion_tips_the_nose_forward_and_down() {
        let mut d = witch_draw();
        d.held = 373;
        let nose = drawn(&d).parts[1].clone();
        assert_eq!(nose.rot[0], -0.9);
        assert_eq!(nose.pivot, [0.0, 1.0, -1.5]);
    }
}
