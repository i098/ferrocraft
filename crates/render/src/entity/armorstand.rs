//! The armor stand, ported from `ArmorStandModel`, `ArmorStandArmorModel` and `ArmorStandRenderer`
//! (1.8.9, Feather names). Every constant below was read from the real jar with `javap -c -constants`:
//! the box layout and pivots in the two model constructors, the pose to rotation copy in
//! `setupAnimation`, the status flags in `ArmorStandEntity.isSmall`, `isShowArms`,
//! `isBasePlateVisible` (bit 8 hides the plate) and `isMarker`, the metadata indices 11 to 16 and
//! the default poses in its static initializer.
//!
//! Part indices follow `ModelBiped` so the armor and held item layers reuse their tables: 0 head, 1
//! hat (never drawn: its texels in `wood.png` are transparent), 2 body, 3 right arm, 4 left arm,
//! 5 right leg, 6 left leg; the stand body adds 7 and 8 the two body sticks, 9 the shoulder stick
//! and 10 the base plate.
//!
//! shortcut: a skull worn as a helmet is not drawn, and the stand yaw is the entity yaw rather than
//! `renderYawOffset`, which the server sets to the same value.

use super::{emit_armor, emit_part, Model, Part, Skin, Skins, Xf, V3};
use crate::mesh::MeshData;

/// `ArmorStandEntity` status byte (metadata 10).
const SMALL: u8 = 1;
const ARMS: u8 = 4;
const NO_BASE_PLATE: u8 = 8;

/// Model part indices.
pub(super) const RIGHT_ARM: usize = 3;
const LEFT_ARM: usize = 4;
const BASE_PLATE: usize = 10;
const HAT: usize = 1;

/// Head, body, left arm, right arm, left leg and right leg, in degrees
/// (`DEFAULT_*_ROTATION` in `ArmorStandEntity`).
const DEFAULT_POSE: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [-10.0, 0.0, -10.0],
    [-15.0, 0.0, 10.0],
    [-1.0, 0.0, -1.0],
    [1.0, 0.0, 1.0],
];

/// What the stand model reads from metadata and equipment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StandLook {
    /// Metadata 10: 1 small, 4 arms, 8 no base plate, 16 marker.
    pub flags: u8,
    /// Metadata 11 to 16 in degrees: head, body, left arm, right arm, left leg, right leg.
    pub pose: [[f32; 3]; 6],
    /// Damage value of the helmet, which picks the variant of a worn block.
    pub head_damage: i16,
}

impl Default for StandLook {
    fn default() -> StandLook {
        StandLook {
            flags: 0,
            pose: DEFAULT_POSE,
            head_damage: 0,
        }
    }
}

impl StandLook {
    /// `pose` with the entries the server sent laid over the vanilla defaults.
    pub fn with_pose(mut self, sent: [Option<[f32; 3]>; 6]) -> StandLook {
        for (pose, sent) in self.pose.iter_mut().zip(sent) {
            *pose = sent.unwrap_or(*pose);
        }
        self
    }
}

fn radians(degrees: [f32; 3]) -> V3 {
    degrees.map(|a| a * 0.017_453_292)
}

/// `ModelBiped.render`: a small stand scales its head by 3/4 and the rest by 1/2, each moved down
/// first; a sneaking one drops by 0.2 blocks.
fn placed(part: Part, d: &super::EntityDraw, head: bool) -> Part {
    let look = &d.look.stand;
    if look.flags & SMALL != 0 {
        if head {
            part.placed(0.75, [0.0, 16.0, 0.0])
        } else {
            part.placed(0.5, [0.0, 24.0, 0.0])
        }
    } else if d.sneaking {
        part.placed(1.0, [0.0, 3.2, 0.0])
    } else {
        part
    }
}

/// `ArmorStandModel`: the thin wooden body.
pub(super) fn model(d: &super::EntityDraw) -> Model {
    let [head, body, l_arm, r_arm, l_leg, r_leg] = d.look.stand.pose.map(radians);
    let head_part = Part::new(
        [0.0, 1.0, 0.0],
        [-1.0, -7.0, -1.0],
        [2.0, 7.0, 2.0],
        (0.0, 0.0),
    )
    .rot(head);
    let mut parts = vec![
        head_part.clone(),
        head_part,
        Part::new([0.0; 3], [-6.0, 0.0, -1.5], [12.0, 3.0, 3.0], (0.0, 26.0)).rot(body),
        Part::new(
            [-5.0, 2.0, 0.0],
            [-2.0, -2.0, -1.0],
            [2.0, 12.0, 2.0],
            (24.0, 0.0),
        )
        .rot(r_arm),
        Part::new(
            [5.0, 2.0, 0.0],
            [0.0, -2.0, -1.0],
            [2.0, 12.0, 2.0],
            (32.0, 16.0),
        )
        .rot(l_arm)
        .mirrored(true),
        Part::new(
            [-1.9, 12.0, 0.0],
            [-1.0, 0.0, -1.0],
            [2.0, 11.0, 2.0],
            (8.0, 0.0),
        )
        .rot(r_leg),
        Part::new(
            [1.9, 12.0, 0.0],
            [-1.0, 0.0, -1.0],
            [2.0, 11.0, 2.0],
            (40.0, 16.0),
        )
        .rot(l_leg)
        .mirrored(true),
    ];
    let stick = |min: V3, size: V3, uv| Part::new([0.0; 3], min, size, uv).rot(body);
    parts.extend([
        stick([-3.0, 3.0, -1.0], [2.0, 7.0, 2.0], (16.0, 0.0)),
        stick([1.0, 3.0, -1.0], [2.0, 7.0, 2.0], (48.0, 16.0)),
        stick([-4.0, 10.0, -1.0], [8.0, 2.0, 2.0], (0.0, 48.0)),
        // The plate turns against the stand so that it stays aligned with the world.
        Part::new(
            [0.0, 12.0, 0.0],
            [-6.0, 11.0, -6.0],
            [12.0, 1.0, 12.0],
            (0.0, 32.0),
        )
        .rot([0.0, -d.yaw.to_radians(), 0.0]),
    ]);
    Model {
        skin: Skin::ArmorStand,
        tint: [1.0; 3],
        parts,
    }
}

/// `ArmorStandArmorModel`: a `ModelBiped` on the stand's pose, with its legs one pixel higher.
pub(super) fn armor_model(d: &super::EntityDraw) -> Model {
    let [head, body, l_arm, r_arm, l_leg, r_leg] = d.look.stand.pose.map(radians);
    let head_part = Part::new([0.0, 1.0, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0)).rot(head);
    let parts = vec![
        head_part.clone(),
        head_part,
        Part::new([0.0; 3], [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0], (16.0, 16.0)).rot(body),
        Part::new(
            [-5.0, 2.0, 0.0],
            [-3.0, -2.0, -2.0],
            [4.0, 12.0, 4.0],
            (40.0, 16.0),
        )
        .rot(r_arm),
        Part::new(
            [5.0, 2.0, 0.0],
            [-1.0, -2.0, -2.0],
            [4.0, 12.0, 4.0],
            (40.0, 16.0),
        )
        .rot(l_arm)
        .mirrored(true),
        Part::new(
            [-1.9, 11.0, 0.0],
            [-2.0, 0.0, -2.0],
            [4.0, 12.0, 4.0],
            (0.0, 16.0),
        )
        .rot(r_leg),
        Part::new(
            [1.9, 11.0, 0.0],
            [-2.0, 0.0, -2.0],
            [4.0, 12.0, 4.0],
            (0.0, 16.0),
        )
        .rot(l_leg)
        .mirrored(true),
    ];
    Model {
        skin: Skin::ArmorStand,
        tint: [1.0; 3],
        parts: parts
            .into_iter()
            .enumerate()
            .map(|(i, p)| placed(p, d, i == 0))
            .collect(),
    }
}

/// Which parts of the stand body are drawn: the arms need the arms flag, the plate its absence.
fn drawn(index: usize, flags: u8) -> bool {
    match index {
        HAT => false,
        LEFT_ARM | RIGHT_ARM => flags & ARMS != 0,
        BASE_PLATE => flags & NO_BASE_PLATE == 0,
        _ => true,
    }
}

/// The body unless the stand is invisible, then its armor; returns where full-bright vertices
/// start, which is the end since a stand has none.
pub(super) fn draw(out: &mut MeshData, d: &super::EntityDraw, skins: &Skins) -> usize {
    let xf = Xf::new(d, false);
    if !d.invisible {
        let body = model(d);
        let (skin, tint) = super::base_look(&body, d, skins);
        let v_off = skins.v_offset(skin);
        for (i, part) in body.parts.iter().enumerate() {
            if drawn(i, d.look.stand.flags) {
                // The sticks and the plate scale with the body group of a small stand.
                let part = placed(part.clone(), d, i == 0);
                emit_part(out, &xf, &part, 0.0, tint, v_off);
            }
        }
    }
    emit_armor(out, &xf, &armor_model(d), d, skins);
    out.vertices.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Class, EntityDraw};

    fn stand() -> EntityDraw {
        EntityDraw {
            class: Class::Object(78),
            pos: [0.0; 3],
            yaw: 90.0,
            head_yaw: 0.0,
            pitch: 0.0,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            swing: 0.0,
            age: 0.0,
            sneaking: false,
            hurt: false,
            death_time: 0.0,
            invisible: false,
            size: (0.5, 1.975),
            held: 0,
            armor: [0; 4],
            armor_dye: [None; 4],
            armor_glint: [false; 4],
            held_glint: false,
            held_damage: 0,
            stack: None,
            hover_start: 0.0,
            hang: Default::default(),
            name: None,
            skin: None,
            burning: false,
            eye_height: 1.7,
            always_name: false,
            team_tag: Default::default(),
            pointed: false,
            ridden: false,
            child: false,
            using: None,
            view: [0.0; 2],
            line: None,
            look: crate::entity::Look::default(),
        }
    }

    fn close(a: V3, b: V3) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6)
    }

    #[test]
    fn a_pose_sets_the_part_rotations_in_radians() {
        let mut d = stand();
        d.look.stand = StandLook::default().with_pose([
            Some([10.0, 20.0, 30.0]),
            Some([0.0, 90.0, 0.0]),
            None,
            None,
            None,
            Some([180.0, 0.0, -90.0]),
        ]);
        let m = model(&d);
        assert!(close(m.parts[0].rot, radians([10.0, 20.0, 30.0])));
        // The body and the three sticks share one rotation.
        for i in [2, 7, 8, 9] {
            assert!(close(m.parts[i].rot, radians([0.0, 90.0, 0.0])), "part {i}");
        }
        // A pose the server left out keeps its default.
        assert!(close(m.parts[LEFT_ARM].rot, radians([-10.0, 0.0, -10.0])));
        assert!(close(m.parts[RIGHT_ARM].rot, radians([-15.0, 0.0, 10.0])));
        assert!(close(m.parts[5].rot, radians([180.0, 0.0, -90.0])));
        // The plate cancels the entity yaw.
        assert!(close(
            m.parts[BASE_PLATE].rot,
            [0.0, -90f32.to_radians(), 0.0]
        ));
        // The armor model takes the same pose.
        assert!(close(armor_model(&d).parts[0].rot, m.parts[0].rot));
    }

    #[test]
    fn the_flags_pick_the_parts_and_the_small_flag_halves_the_body() {
        let shown = |flags| (0..=BASE_PLATE).filter(|&i| drawn(i, flags)).count();
        assert_eq!(shown(0), 8, "no arms, plate on, no hat");
        assert_eq!(shown(ARMS), 10);
        assert_eq!(shown(NO_BASE_PLATE), 7);
        let mut d = stand();
        d.look.stand.flags = SMALL;
        let m = armor_model(&d);
        assert_eq!(
            (m.parts[0].scale, m.parts[0].shift),
            (0.75, [0.0, 16.0, 0.0])
        );
        assert_eq!(
            (m.parts[2].scale, m.parts[2].shift),
            (0.5, [0.0, 24.0, 0.0])
        );
    }
}
