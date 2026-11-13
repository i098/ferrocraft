//! `LayerArrow`: the arrows stuck in a player, drawn on random spots of the model's boxes.
//!
//! Only `RenderPlayer` adds the layer in 1.8.9 (checked with `javap` on the remapped client jar:
//! `StuckArrowLayer` is referenced by `PlayerRenderer` alone), so mobs show no arrows.

use super::{projectile, rotate, Class, EntityDraw, Model, Part, Skins, Xf, V3};
use crate::jrandom::JavaRandom;
use crate::mesh::MeshData;

/// `ModelBase.boxList` of `ModelPlayer`, in construction order. `ModelBiped` builds head, hat,
/// body and the four limbs; `ModelPlayer` then adds ears and cape and builds its own left arm,
/// (thin arms: right arm,) sleeves, left leg, pants and jacket. The replaced limbs stay in the
/// list at their first slot with the pose of the constructor, because nothing animates them.
///
/// `m` holds the live parts in the order of `biped`: head, hat, body, right arm, left arm, right
/// leg, left leg, jacket, right sleeve, left sleeve, right pants, left pants.
fn boxes(m: &Model, thin: bool) -> Vec<Part> {
    let stale = |pivot: V3, min: V3, size: V3| Part::new(pivot, min, size, (0.0, 0.0));
    let live = |i: usize| m.parts[i].clone();
    let (head, hat, body, r_arm, l_arm, r_leg, l_leg) = (
        live(0),
        live(1),
        live(2),
        live(3),
        live(4),
        live(5),
        live(6),
    );
    let (jacket, r_sleeve, l_sleeve, r_pants, l_pants) =
        (live(7), live(8), live(9), live(10), live(11));
    let ears = stale([0.0; 3], [-3.0, -6.0, -1.0], [6.0, 6.0, 1.0]);
    let cape = stale([0.0; 3], [-5.0, 0.0, -1.0], [10.0, 16.0, 1.0]);
    let old_r_arm = stale([-5.0, 2.0, 0.0], [-3.0, -2.0, -2.0], [4.0, 12.0, 4.0]);
    let old_l_arm = stale([5.0, 2.0, 0.0], [-1.0, -2.0, -2.0], [4.0, 12.0, 4.0]);
    let old_l_leg = stale([1.9, 12.0, 0.0], [-2.0, 0.0, -2.0], [4.0, 12.0, 4.0]);
    if thin {
        vec![
            head, hat, body, old_r_arm, old_l_arm, r_leg, old_l_leg, ears, cape, l_arm, r_arm,
            l_sleeve, r_sleeve, l_leg, l_pants, r_pants, jacket,
        ]
    } else {
        vec![
            head, hat, body, r_arm, old_l_arm, r_leg, old_l_leg, ears, cape, l_arm, l_sleeve,
            r_sleeve, l_leg, l_pants, r_pants, jacket,
        ]
    }
}

/// One stuck arrow: the part it sits on, its point in that part's box (model pixels) and its
/// yaw and pitch in degrees.
struct Stuck {
    part: usize,
    at: V3,
    yaw: f32,
    pitch: f32,
}

/// The draws of `LayerArrow.doRenderLayer` for `count` arrows in an entity with `parts` boxes:
/// a `java.util.Random` seeded by the entity id picks a part, a box, a point and a direction.
fn place(id: i32, count: u8, parts: &[Part]) -> Vec<Stuck> {
    let mut rng = JavaRandom::new(i64::from(id));
    (0..count)
        .map(|_| {
            let part = rng.next_int(parts.len() as i32) as usize;
            // A part has one box; vanilla still draws from the generator for it.
            rng.next_int(1);
            let (f, f1, f2) = (rng.next_float(), rng.next_float(), rng.next_float());
            let p = &parts[part];
            let at = [
                p.min[0] + p.size[0] * f,
                p.min[1] + p.size[1] * f1,
                p.min[2] + p.size[2] * f2,
            ];
            let (a, b, c) = (-(f * 2.0 - 1.0), -(f1 * 2.0 - 1.0), -(f2 * 2.0 - 1.0));
            let flat = (a * a + c * c).sqrt();
            let deg = |y: f32, x: f32| {
                (f64::from(y).atan2(f64::from(x)) * 180.0 / std::f64::consts::PI) as f32
            };
            Stuck {
                part,
                at,
                yaw: deg(a, c),
                pitch: deg(b, flat),
            }
        })
        .collect()
}

pub(super) fn emit(out: &mut MeshData, xf: &Xf, m: &Model, d: &EntityDraw, skins: &Skins) {
    let Class::Player { slim } = d.class else {
        return;
    };
    if d.look.arrows == 0 {
        return;
    }
    let parts = boxes(m, slim);
    for s in place(d.look.id, d.look.arrows, &parts) {
        let part = &parts[s.part];
        // The arrow's block offsets are model pixels / 16 in the part's frame, which carries its pose.
        let to_world = |q: V3| -> V3 {
            let r = rotate(q, part.rot);
            xf.world([0, 1, 2].map(|i| (r[i] + part.pivot[i] + part.shift[i]) * part.scale))
        };
        projectile::arrow_model(out, skins, s.yaw, s.pitch, |p| {
            to_world([0, 1, 2].map(|i| s.at[i] + p[i] * 16.0))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::tests::steve;

    fn mesh(d: &EntityDraw) -> MeshData {
        let mut out = MeshData::default();
        crate::entity::draw_entity(&mut out, d, &Skins::flat());
        out
    }

    #[test]
    fn one_id_gives_the_same_arrows_every_frame() {
        let m = crate::entity::model(&steve());
        let parts = boxes(&m, false);
        let a = place(7, 5, &parts);
        let b = place(7, 5, &parts);
        let key = |v: &[Stuck]| {
            v.iter()
                .map(|s| (s.part, s.at, s.yaw, s.pitch))
                .collect::<Vec<_>>()
        };
        assert_eq!(key(&a), key(&b));
        assert_ne!(key(&a), key(&place(8, 5, &parts)));
    }

    #[test]
    fn first_arrow_matches_java_util_random() {
        let m = crate::entity::model(&steve());
        let angles = [
            ([-0.9027153f32, 0.52511847, -0.7151768], -128.38805f32, 24.510975f32),
        ];
        for (thin, idx) in [(false, [10]), (true, [5])] {
            let parts = boxes(&m, thin);
            let got = place(1234, 1, &parts);
            assert_eq!(got.len(), 1);
            for (i, s) in got.iter().enumerate() {
                let (dir, yaw, pitch) = angles[i];
                assert_eq!(s.part, idx[i], "thin={thin} arrow {i}");
                assert!((s.yaw - yaw).abs() < 1e-4, "yaw {} vs {yaw}", s.yaw);
                assert!((s.pitch - pitch).abs() < 1e-4, "pitch {} vs {pitch}", s.pitch);
                let p = &parts[s.part];
                for k in 0..3 {
                    let f = (1.0 - dir[k]) / 2.0;
                    let want = p.min[k] + p.size[k] * f;
                    assert!((s.at[k] - want).abs() < 1e-4, "at[{k}] {} vs {want}", s.at[k]);
                }
            }
        }
    }

    #[test]
    fn the_player_box_list_has_vanilla_length() {
        let m = crate::entity::model(&steve());
        assert_eq!(boxes(&m, false).len(), 16);
        assert_eq!(boxes(&m, true).len(), 17);
    }

    #[test]
    fn each_stuck_arrow_adds_six_quads_and_only_players_get_them() {
        let mut d = steve();
        let base = mesh(&d).vertices.len();
        d.look.arrows = 3;
        d.look.id = 42;
        assert_eq!(mesh(&d).vertices.len(), base + 3 * 6 * 4);
        d.class = Class::Mob(54);
        let zombie = mesh(&d).vertices.len();
        d.look.arrows = 0;
        assert_eq!(zombie, mesh(&d).vertices.len());
    }
}
