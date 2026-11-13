//! Models of the slime, magma cube, blaze, ghast and bat, ported from `ModelSlime` (with the
//! outer gel of `LayerSlimeGel`), `ModelMagmaCube`, `ModelBlaze`, `ModelGhast` and `ModelBat`: box
//! layout, texture offsets and `setRotationAngles`. Every constant was read from the 1.8.9 classes
//! with `harness/mobmodels.sh`, which also runs the constructors and animation of the models.
//!
//! Not ported: the squish of `RenderSlime` and `RenderMagmaCube` (it needs the client-side jump
//! state of the entity, which the draw does not carry, so the cube stays at its resting shape) and
//! the stuck rotation of a bat wing from a roost (`ModelBat` keeps the last pose in the shared
//! model; here a flying bat starts from flat wings).

use super::mobs::{head_rot, model, Layer};
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const SLIME: u8 = 55;
pub(super) const MAGMA_CUBE: u8 = 62;
pub(super) const BLAZE: u8 = 61;
pub(super) const GHAST: u8 = 56;
pub(super) const BAT: u8 = 65;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    Some(match id {
        SLIME => slime(),
        MAGMA_CUBE => magma_cube(),
        BLAZE => blaze(d),
        GHAST => ghast(d),
        BAT => bat(d),
        _ => return None,
    })
}

/// `LayerSlimeGel`: the translucent outer cube, unless the slime is invisible.
pub(super) fn layers(d: &EntityDraw) -> Vec<Layer> {
    if d.class != super::Class::Mob(SLIME) || d.invisible {
        return Vec::new();
    }
    vec![Layer {
        skin: Skin::Slime,
        tint: [1.0; 3],
        parts: vec![Part::new(
            [0.0; 3],
            [-4.0, 16.0, -4.0],
            [8.0; 3],
            (0.0, 0.0),
        )],
        bright: false,
    }]
}

/// `EntitySlime.getSlimeSize` from metadata 16; a slime that never sent it has size 1.
pub(super) fn cube_size(d: &EntityDraw) -> f32 {
    f32::from(d.look.variant.max(1))
}

/// `EntityMagmaCube.getBrightnessForRender` and `EntityBlaze.getBrightnessForRender` are 15728880.
pub(super) fn glows(d: &EntityDraw) -> bool {
    matches!(d.class, super::Class::Mob(MAGMA_CUBE | BLAZE))
}

/// `RenderBat.rotateCorpse`: a flying bat bobs by `cos(age * 0.3) * 0.1`, a roosting one hangs 0.1 low.
pub(super) fn bat_lift(d: &EntityDraw) -> f32 {
    if roosting(d) {
        -0.1
    } else {
        (d.age * 0.3).cos() * 0.1
    }
}

/// `EntityBat.getIsBatHanging`: bit 0 of metadata 16.
fn roosting(d: &EntityDraw) -> bool {
    d.look.variant & 1 != 0
}

/// `ModelSlime(16)`: the inner body, two eyes and the mouth. The first 8x8x8 box the constructor
/// adds is replaced and never drawn.
fn slime() -> Model {
    let at = [0.0; 3];
    model(
        Skin::Slime,
        vec![
            Part::new(at, [-3.0, 17.0, -3.0], [6.0; 3], (0.0, 16.0)),
            Part::new(at, [-3.25, 18.0, -3.5], [2.0; 3], (32.0, 0.0)),
            Part::new(at, [1.25, 18.0, -3.5], [2.0; 3], (32.0, 4.0)),
            Part::new(at, [0.0, 21.0, -3.5], [1.0; 3], (32.0, 8.0)),
        ],
    )
}

/// `ModelMagmaCube`: the core and eight one-pixel slices of the shell. The slices sit apart by
/// `-(4 - i) * squish * 1.7`, which is nil at rest.
fn magma_cube() -> Model {
    const SLICE_UV: [(f32, f32); 8] = [
        (0.0, 0.0),
        (0.0, 1.0),
        (24.0, 10.0),
        (24.0, 19.0),
        (0.0, 4.0),
        (0.0, 5.0),
        (0.0, 6.0),
        (0.0, 7.0),
    ];
    let at = [0.0; 3];
    let mut parts: Vec<Part> = SLICE_UV
        .iter()
        .enumerate()
        .map(|(i, &uv)| Part::new(at, [-4.0, 16.0 + i as f32, -4.0], [8.0, 1.0, 8.0], uv))
        .collect();
    parts.push(Part::new(at, [-2.0, 18.0, -2.0], [4.0; 3], (0.0, 16.0)));
    model(Skin::MagmaCube, parts)
}

/// One ring of blaze rods: (first rod, rod count, start angle, radius, height base, phase step per
/// rod, phase scale).
type Ring = (usize, usize, f32, f32, f32, f32, f32);

/// `ModelBlaze`: the head and twelve rods in three rings that spin and bob.
fn blaze(d: &EntityDraw) -> Model {
    let age = d.age;
    let rings: [Ring; 3] = [
        (0, 4, age * PI * -0.1, 9.0, -2.0, 2.0, 0.25),
        (4, 4, PI / 4.0 + age * PI * 0.03, 7.0, 2.0, 2.0, 0.25),
        (8, 4, 0.471_238_94 + age * PI * -0.05, 5.0, 11.0, 1.5, 0.5),
    ];
    let mut parts =
        vec![Part::new([0.0; 3], [-4.0, -4.0, -4.0], [8.0; 3], (0.0, 0.0)).rot(head_rot(d))];
    for ring in rings {
        rods(&mut parts, ring, age);
    }
    model(Skin::Blaze, parts)
}

/// Appends the rods of one ring, a quarter turn apart.
fn rods(parts: &mut Vec<Part>, (first, count, start, radius, base, step, scale): Ring, age: f32) {
    for i in first..first + count {
        let angle = start + (i - first) as f32 * PI / 2.0;
        let y = base + ((i as f32 * step + age) * scale).cos();
        let pivot = [angle.cos() * radius, y, angle.sin() * radius];
        parts.push(Part::new(pivot, [0.0; 3], [2.0, 8.0, 2.0], (0.0, 16.0)));
    }
}

/// Tentacle lengths of `ModelGhast`, from `new Random(1660).nextInt(7) + 8` nine times.
const TENTACLE_LEN: [f32; 9] = [8.0, 13.0, 9.0, 11.0, 11.0, 10.0, 12.0, 9.0, 12.0];

/// `ModelGhast`: a 16 cube and nine tentacles that sway. `RenderGhast` scales it by 4.5 and the model
/// lowers itself by 0.6 blocks (9.6 pixels).
fn ghast(d: &EntityDraw) -> Model {
    let drop = 9.6;
    let mut parts = vec![Part::new(
        [0.0, 8.0 + drop, 0.0],
        [-8.0; 3],
        [16.0; 3],
        (0.0, 0.0),
    )];
    for (i, &len) in TENTACLE_LEN.iter().enumerate() {
        let x = (((i % 3) as f32 - ((i / 3) % 2) as f32 * 0.5 + 0.25) / 2.0 * 2.0 - 1.0) * 5.0;
        let z = ((i / 3) as f32 / 2.0 * 2.0 - 1.0) * 5.0;
        let sway = 0.2 * (d.age * 0.3 + i as f32).sin() + 0.4;
        parts.push(
            Part::new(
                [x, 15.0 + drop, z],
                [-1.0, 0.0, -1.0],
                [2.0, len, 2.0],
                (0.0, 0.0),
            )
            .rot([sway, 0.0, 0.0]),
        );
    }
    // `EntityGhast` metadata 16 is 1 while it charges a fireball.
    let skin = if d.look.variant != 0 {
        Skin::GhastShooting
    } else {
        Skin::Ghast
    };
    model(skin, parts)
}

/// Pivot, rotation and tip rotation of one bat wing.
struct Wing {
    pivot: [f32; 3],
    rot: [f32; 3],
    tip: [f32; 3],
}

/// `ModelBat` when it hangs from a roost: head down, wings folded over the back.
fn roost_pose(d: &EntityDraw) -> ([f32; 3], [f32; 3], [f32; 3], [Wing; 2]) {
    let [pitch, yaw, _] = head_rot(d);
    let wing = |mirror: f32| Wing {
        pivot: [-3.0 * mirror, 0.0, 3.0],
        rot: [-0.157_079_64, -1.256_637_1 * mirror, 0.0],
        tip: [0.0, -1.727_876_1 * mirror, 0.0],
    };
    (
        [0.0, -2.0, 0.0],
        [pitch, PI - yaw, PI],
        [PI, 0.0, 0.0],
        [wing(1.0), wing(-1.0)],
    )
}

/// `ModelBat` in flight: head up, body tipped forward, wings flapping.
fn flight_pose(d: &EntityDraw) -> ([f32; 3], [f32; 3], [f32; 3], [Wing; 2]) {
    let [pitch, yaw, _] = head_rot(d);
    let flap = (d.age * 1.3).cos() * PI * 0.25;
    let wing = |mirror: f32| Wing {
        pivot: [0.0; 3],
        rot: [0.0, flap * mirror, 0.0],
        tip: [0.0, flap * 0.5 * mirror, 0.0],
    };
    (
        [0.0; 3],
        [pitch, yaw, 0.0],
        [PI / 4.0 + (d.age * 0.1).cos() * 0.15, 0.0, 0.0],
        [wing(1.0), wing(-1.0)],
    )
}

/// `ModelBat`: head with two ears, body, and two wings that each carry a wing tip.
fn bat(d: &EntityDraw) -> Model {
    let at = [0.0; 3];
    let (head_pivot, head_pose, body_rot, [right, left]) = if roosting(d) {
        roost_pose(d)
    } else {
        flight_pose(d)
    };
    let body = (at, body_rot);
    let head = vec![(head_pivot, head_pose)];
    let ear = |x: f32, mirror: bool| {
        Part::new(at, [x, -6.0, -2.0], [3.0, 4.0, 1.0], (24.0, 0.0))
            .mirrored(mirror)
            .under(head.clone())
    };
    let wing = |w: &Wing, x: f32, mirror: bool| {
        Part::new(w.pivot, [x, 1.0, 1.5], [10.0, 16.0, 1.0], (42.0, 0.0))
            .rot(w.rot)
            .mirrored(mirror)
            .under(vec![body])
    };
    let tip = |w: &Wing, pivot: [f32; 3], min: [f32; 3], mirror: bool| {
        Part::new(pivot, min, [8.0, 12.0, 1.0], (24.0, 16.0))
            .rot(w.tip)
            .mirrored(mirror)
            .under(vec![(w.pivot, w.rot), body])
    };
    let parts = vec![
        Part::new(head_pivot, [-3.0; 3], [6.0; 3], (0.0, 0.0)).rot(head_pose),
        ear(-4.0, false),
        ear(1.0, true),
        Part::new(at, [-3.0, 4.0, -3.0], [6.0, 12.0, 6.0], (0.0, 16.0)).rot(body_rot),
        Part::new(at, [-5.0, 16.0, 0.0], [10.0, 6.0, 1.0], (0.0, 34.0)).rot(body_rot),
        wing(&right, -12.0, false),
        tip(&right, [-12.0, 1.0, 1.5], [-8.0, 1.0, 0.0], false),
        wing(&left, 2.0, true),
        tip(&left, [12.0, 1.0, 1.5], [0.0, 1.0, 0.0], true),
    ];
    model(Skin::Bat, parts)
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{Class, Skins};
    use super::*;

    fn mob(id: u8) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(id);
        d
    }

    #[test]
    fn slime_has_body_eyes_mouth_and_a_gel_layer() {
        let d = mob(SLIME);
        let m = build(&d, SLIME).unwrap();
        assert_eq!(m.parts.len(), 4);
        assert_eq!(m.parts[1].min, [-3.25, 18.0, -3.5]);
        let gel = layers(&d);
        assert_eq!(gel.len(), 1);
        assert_eq!(gel[0].parts[0].size, [8.0; 3]);
        let mut hidden = d.clone();
        hidden.invisible = true;
        assert!(layers(&hidden).is_empty());
    }

    #[test]
    fn slime_size_scales_the_model_and_defaults_to_one() {
        let mut d = mob(SLIME);
        assert_eq!(cube_size(&d), 1.0);
        d.look.variant = 4;
        assert_eq!(cube_size(&d), 4.0);
    }

    #[test]
    fn magma_cube_has_core_and_eight_slices() {
        let m = build(&mob(MAGMA_CUBE), MAGMA_CUBE).unwrap();
        assert_eq!(m.parts.len(), 9);
        assert_eq!(m.parts[2].uv, (24.0, 10.0));
        assert_eq!(m.parts[8].min, [-2.0, 18.0, -2.0]);
    }

    #[test]
    fn blaze_rods_ring_at_the_oracle_positions() {
        // `harness/mobmodels.sh` after setupAnimation(age 10): rod 0 at (-9, -2.8010893, ~0).
        let mut d = mob(BLAZE);
        d.age = 10.0;
        let m = build(&d, BLAZE).unwrap();
        assert_eq!(m.parts.len(), 13);
        let rod = &m.parts[1];
        assert!((rod.pivot[0] + 9.0).abs() < 1e-4, "{:?}", rod.pivot);
        assert!((rod.pivot[1] + 2.801_089_3).abs() < 1e-4, "{:?}", rod.pivot);
        // `MathHelper.cos` reads a 65536 entry table, so the oracle differs from `f32::cos` by about 1e-4.
        // Rod 1 of the outer ring is a quarter turn further: (0, -2.9899907, -9) in 1.8.9.
        let next = &m.parts[2];
        assert!(next.pivot[0].abs() < 1e-4, "{:?}", next.pivot);
        assert!(
            (next.pivot[1] + 2.989_990_7).abs() < 1e-4,
            "{:?}",
            next.pivot
        );
        assert!((next.pivot[2] + 9.0).abs() < 1e-4, "{:?}", next.pivot);
        // Rod 8 of the inner ring: (2.269867, 11.00441, -4.454859).
        let inner = &m.parts[9];
        assert!(
            (inner.pivot[0] - 2.269_867).abs() < 1e-3,
            "{:?}",
            inner.pivot
        );
        assert!(
            (inner.pivot[2] + 4.454_859).abs() < 1e-3,
            "{:?}",
            inner.pivot
        );
    }

    #[test]
    fn ghast_tentacles_match_the_oracle_and_the_attack_skin_follows_metadata() {
        let mut d = mob(GHAST);
        d.age = 10.0;
        let m = build(&d, GHAST).unwrap();
        assert_eq!(m.parts.len(), 10);
        // Tentacle 1 of `GhastModel`: pivot (1.25, 15, -5) before the 0.6 block lowering, length 13,
        // x rotation 0.24864595 at age 10.
        let t = &m.parts[2];
        assert_eq!([t.pivot[0], t.pivot[2]], [1.25, -5.0]);
        assert_eq!(t.size[1], 13.0);
        assert!((t.rot[0] - 0.248_645_95).abs() < 1e-5, "{:?}", t.rot);
        assert_eq!(m.skin, Skin::Ghast);
        d.look.variant = 1;
        assert_eq!(build(&d, GHAST).unwrap().skin, Skin::GhastShooting);
    }

    #[test]
    fn bat_flaps_when_flying_and_folds_when_roosting() {
        let mut d = mob(BAT);
        d.age = 0.0;
        let flying = build(&d, BAT).unwrap();
        assert_eq!(flying.parts.len(), 9);
        // Right wing at age 0: cos(0) * PI * 0.25.
        assert!((flying.parts[5].rot[1] - PI * 0.25).abs() < 1e-5);
        assert!((flying.parts[3].rot[0] - (PI / 4.0 + 0.15)).abs() < 1e-5);
        d.look.variant = 1;
        let roost = build(&d, BAT).unwrap();
        assert_eq!(roost.parts[3].rot[0], PI);
        assert_eq!(roost.parts[5].rot[1], -1.256_637_1);
        assert_eq!(roost.parts[7].rot[1], 1.256_637_1);
        assert_eq!(bat_lift(&d), -0.1);
    }

    #[test]
    fn the_five_mobs_draw_a_model_not_a_box_at_vanilla_scale() {
        // Height of the drawn model in blocks: slime size 1 is 6 pixels tall plus the 8 pixel gel.
        let height = |id: u8, size: u8| {
            let mut d = mob(id);
            d.look.variant = size;
            let m = super::super::build(std::slice::from_ref(&d), &Skins::flat(), |_| [1.0; 2]);
            let (lo, hi) = m.vertices.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                (lo.min(v.pos[1]), hi.max(v.pos[1]))
            });
            (m.vertices.len(), hi - lo)
        };
        let (verts, h) = height(SLIME, 1);
        assert_eq!(verts, 4 * 24, "four boxes, no gel without a texture");
        assert!((h - 6.0 / 16.0).abs() < 1e-4, "{h}");
        let (_, h) = height(SLIME, 4);
        assert!((h - 6.0 / 16.0 * 4.0).abs() < 1e-4, "{h}");
        let (verts, h) = height(MAGMA_CUBE, 1);
        assert_eq!(verts, 9 * 24);
        assert!((h - 8.0 / 16.0).abs() < 1e-4, "{h}");
        assert_eq!(height(BLAZE, 0).0, 13 * 24);
        assert_eq!(height(GHAST, 0).0, 10 * 24);
        assert_eq!(height(BAT, 0).0, 9 * 24);
    }
}
