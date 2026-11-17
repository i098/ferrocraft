//! Horses, donkeys, mules, zombie horses and skeleton horses (mob 100), ported from `ModelHorse` (box layout,
//! texture offsets, `setLivingAnimations`), `RenderHorse` (the scale and the texture) and the coat logic of
//! `EntityHorse`. Every constant was read from the 1.8.9 classes with `java/oracle/HorseModels.java`.
//!
//! The animation values come from the entity: see [`HorseLook`] and `world::horse::HorseAnim`.

use super::mobs::{model, Layer};
use super::skin::horse as tex;
use super::{wrap, EntityDraw, Model, Part, Skin};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_6, PI};

pub(super) const HORSE: u8 = 100;

/// A baby is drawn at this `getSize`, scaled part by part.
const BABY_SIZE: f32 = 0.5;
/// How far the armor layer grows the boxes, in pixels, to lie over the body without z-fighting.
const ARMOR_GROW: f32 = 0.05;

/// What the horse model reads from the entity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HorseLook {
    /// Metadata byte 19: 0 horse, 1 donkey, 2 mule, 3 zombie horse, 4 skeleton horse.
    pub kind: u8,
    /// Metadata int 16: 4 saddled, 8 chested.
    pub flags: i32,
    /// Metadata int 20: the coat in the low byte, the markings in the next.
    pub variant: i32,
    /// Metadata int 22: the armor tier, 0 to 3.
    pub armor: i32,
    /// `getGrassAnimationProgress`, `getAngryAnimationProgress` (rearing) and `getEatingAnimationProgress`
    /// (the open mouth) of the frame, each 0 to 1.
    pub grass: f32,
    pub angry: f32,
    pub mouth: f32,
}

/// `RenderHorse.applyScale`: a donkey is drawn at 0.87 and a mule at 0.92.
pub(super) fn scale(h: &HorseLook) -> f32 {
    match h.kind {
        1 => 0.87,
        2 => 0.92,
        _ => 1.0,
    }
}

/// `RenderHorse.getTextureLocation`; `None` for a coat or marking the game has no texture for, which draws
/// as the missing texture in vanilla and here as plain white.
fn body_skin(h: &HorseLook) -> Option<u8> {
    let other = |kind: u8| match kind {
        1..=4 => tex::OTHER_FIRST + kind - 1,
        _ => 0,
    };
    if h.kind != 0 {
        return Some(other(h.kind));
    }
    let (coat, marking) = (h.variant & 0xFF, (h.variant >> 8) & 0xFF);
    if coat >= i32::from(tex::COATS_COUNT)
        || h.armor >= 4
        || marking > i32::from(tex::MARKINGS_COUNT)
    {
        return None;
    }
    Some(match marking {
        0 => coat as u8,
        m => tex::marked(coat as u8, m as u8),
    })
}

/// The armor texture drawn over the body of a horse that wears armor.
fn armor_skin(h: &HorseLook) -> Option<u8> {
    let tier = u8::try_from(h.armor).ok().filter(|t| (1..=3).contains(t))?;
    (h.kind == 0 && body_skin(h).is_some()).then_some(tex::ARMOR_FIRST + tier - 1)
}

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == HORSE).then(|| {
        let skin = body_skin(&d.look.horse).map_or(Skin::White, Skin::Horse);
        model(skin, parts(d))
    })
}

/// The armor of a horse: `LayeredTexture` draws it over the body, so it is a second pass here.
pub(super) fn layers(d: &EntityDraw, m: &Model) -> Vec<Layer> {
    let Some(armor) = (d.class == super::Class::Mob(HORSE) && !d.invisible)
        .then(|| armor_skin(&d.look.horse))
        .flatten()
    else {
        return Vec::new();
    };
    let mut parts = m.parts.clone();
    for p in &mut parts {
        p.inflate += ARMOR_GROW;
    }
    vec![Layer {
        skin: Skin::Horse(armor),
        tint: [1.0; 3],
        parts,
        bright: false,
        colors_when_damaged: true,
    }]
}

/// Pivot and angles that `setLivingAnimations` computes, shared by the part groups.
struct Rig {
    /// Rearing, its complement, grazing and the open mouth.
    angry: f32,
    calm: f32,
    grass: f32,
    mouth: f32,
    /// The head pivot (y, z) and its angles; the neck, ears, mane and the bridle follow it.
    head_y: f32,
    head_z: f32,
    head_rx: f32,
    head_ry: f32,
    /// The head pitch with the walking nod, which the reins follow.
    pitch: f32,
    body_rx: f32,
    /// `cos(swing * 0.6662 + PI)`, which drives the legs, and the walk speed.
    cos: f32,
    speed: f32,
    ridden: bool,
    kind: u8,
    saddled: bool,
    chested: bool,
    ticks: f32,
}

impl Rig {
    fn new(d: &EntityDraw) -> Rig {
        let h = &d.look.horse;
        let (angry, grass) = (h.angry, h.grass);
        let speed = d.limb_swing_amount;
        let mut pitch = d.pitch.to_radians();
        if speed > 0.2 {
            pitch += (d.limb_swing * 0.4).cos() * 0.15 * speed;
        }
        let yaw = wrap(d.head_yaw - d.yaw).clamp(-20.0, 20.0).to_radians();
        let keep = 1.0 - angry.max(grass);
        Rig {
            angry,
            calm: 1.0 - angry,
            grass,
            mouth: h.mouth,
            head_y: angry * -6.0 + grass * 11.0 + keep * 4.0,
            head_z: -angry + grass * -10.0 + keep * -10.0,
            head_rx: angry * (PI / 12.0 + pitch) + grass * 2.18166 + keep * (FRAC_PI_6 + pitch),
            head_ry: angry * yaw + keep * yaw,
            pitch,
            body_rx: angry * -45.0_f32.to_radians(),
            cos: (d.limb_swing * 0.6662 + PI).cos(),
            speed,
            ridden: d.ridden,
            kind: h.kind,
            saddled: world::horse::is_saddled(h.flags),
            chested: world::horse::is_chested(h.flags),
            ticks: d.age,
        }
    }

    /// The swing of the packs and the fenders.
    fn sway(&self) -> f32 {
        self.cos * 0.8 * self.speed
    }

    fn head_at(&self) -> [f32; 3] {
        [0.0, self.head_y, self.head_z]
    }

    fn head_rot(&self) -> [f32; 3] {
        [self.head_rx, self.head_ry, 0.0]
    }

    /// A part that follows the head pivot and angles.
    fn on_head(&self, min: [f32; 3], size: [f32; 3], uv: (f32, f32)) -> Part {
        part(self.head_at(), min, size, uv).rot(self.head_rot())
    }
}

fn part(pivot: [f32; 3], min: [f32; 3], size: [f32; 3], uv: (f32, f32)) -> Part {
    Part::new(pivot, min, size, uv)
}

/// `ModelHorse.render`: an adult draws every part as is; a baby draws the legs, the body and the head
/// each with its own scale and lift (in model pixels, 16 to the block).
fn parts(d: &EntityDraw) -> Vec<Part> {
    let r = Rig::new(d);
    let (legs, body, head) = (legs(&r), body(&r), head(&r));
    if !d.child {
        let mut out = if r.saddled { tack(&r) } else { Vec::new() };
        out.extend(legs.into_iter().chain(body).chain(head));
        if r.chested {
            out.extend(chests(&r));
        }
        return out;
    }
    let s = BABY_SIZE;
    let g = r.grass;
    let head_shift = [
        0.0,
        (0.9 * (1.0 - s) * g + 1.35 * (1.0 - s) * (1.0 - g)) * 16.0,
        0.15 * (1.0 - s) * g * 16.0,
    ];
    let mut out = place(legs, s, 0.95 * (1.0 - s) * 16.0, (0.5 + s * 0.5) / s);
    out.extend(place(body, s, 1.35 * (1.0 - s) * 16.0, 1.0));
    let hs = 0.5 + s * s * 0.5;
    out.extend(head.into_iter().map(|p| p.placed(hs, head_shift)));
    out
}

/// `scalef(scale, scale * stretch, scale)` after `translatef(0, shift, 0)` on every part of a group.
fn place(parts: Vec<Part>, scale: f32, shift: f32, stretch: f32) -> Vec<Part> {
    parts
        .into_iter()
        .map(|p| p.placed(scale, [0.0, shift, 0.0]).stretched(stretch))
        .collect()
}

type Box3 = ([f32; 3], [f32; 3], (f32, f32));

/// One leg: its x, and the thigh or forearm, cannon and hoof boxes (corner, size, texture offset).
struct LegBox {
    x: f32,
    upper: Box3,
    cannon: Box3,
    hoof: Box3,
}

const CANNON: [f32; 3] = [3.0, 5.0, 3.0];
const HOOF: [f32; 3] = [4.0, 3.0, 4.0];
const THIGH: [f32; 3] = [4.0, 9.0, 5.0];
const FOREARM: [f32; 3] = [3.0, 8.0, 4.0];

/// Back left, back right, front left, front right.
const LEG_BOXES: [LegBox; 4] = [
    LegBox {
        x: 4.0,
        upper: ([-2.5, -2.0, -2.5], THIGH, (78.0, 29.0)),
        cannon: ([-2.0, 0.0, -1.5], CANNON, (78.0, 43.0)),
        hoof: ([-2.5, 5.1, -2.0], HOOF, (78.0, 51.0)),
    },
    LegBox {
        x: -4.0,
        upper: ([-1.5, -2.0, -2.5], THIGH, (96.0, 29.0)),
        cannon: ([-1.0, 0.0, -1.5], CANNON, (96.0, 43.0)),
        hoof: ([-1.5, 5.1, -2.0], HOOF, (96.0, 51.0)),
    },
    LegBox {
        x: 4.0,
        upper: ([-1.9, -1.0, -2.1], FOREARM, (44.0, 29.0)),
        cannon: ([-1.9, 0.0, -1.6], CANNON, (44.0, 41.0)),
        hoof: ([-2.4, 5.1, -2.1], HOOF, (44.0, 51.0)),
    },
    LegBox {
        x: -4.0,
        upper: ([-1.1, -1.0, -2.1], FOREARM, (60.0, 29.0)),
        cannon: ([-1.1, 0.0, -1.6], CANNON, (60.0, 41.0)),
        hoof: ([-1.6, 5.1, -2.1], HOOF, (60.0, 51.0)),
    },
];

/// Where a leg hangs: the (y, z) and angle of the thigh or forearm, then of the cannon and hoof below it.
struct Swing {
    upper: ([f32; 2], f32),
    lower: ([f32; 2], f32),
}

/// The cannon hangs 7 pixels along the angle of the leg above it.
fn hang(origin: [f32; 2], angle: f32) -> [f32; 2] {
    [
        origin[0] + (FRAC_PI_2 + angle).sin() * 7.0,
        origin[1] + (PI * 1.5 + angle).cos() * 7.0,
    ]
}

impl Rig {
    /// The leg part of `setLivingAnimations`, in [`LEG_BOXES`] order.
    fn swings(&self) -> [Swing; 4] {
        let (a, n, w) = (self.angry, self.calm, self.sway());
        let rear = PI / 12.0 * a;
        let step = self.cos * 0.5 * self.speed;
        let kick = (self.ticks * 0.6 + PI).cos();
        let reared = |lead: f32| (-PI / 3.0 + lead) * a;
        let (fl, fr) = (reared(kick) + w * n, reared(-kick) + -w * n);
        let thigh = [9.0, 11.0];
        let forearm = [-2.0 * a + 9.0 * n, -2.0 * a + -8.0 * n];
        let (bl, br) = (rear + -step * n, rear + step * n);
        let (up, down) = (0.0_f32.max(step), 0.0_f32.max(-step));
        let kick_l = PI * 0.0_f32.max(0.2 + kick * 0.2);
        let kick_r = PI * 0.0_f32.max(0.2 - kick * 0.2);
        [
            Swing {
                upper: (thigh, bl),
                lower: (hang(thigh, bl), -0.087_266_46 * a + (-step - up) * n),
            },
            Swing {
                upper: (thigh, br),
                lower: (hang(thigh, br), -0.087_266_46 * a + (step - down) * n),
            },
            Swing {
                upper: (forearm, fl),
                lower: (hang(forearm, fl), (fl + kick_l) * a + (w + up) * n),
            },
            Swing {
                upper: (forearm, fr),
                lower: (hang(forearm, fr), (fr + kick_r) * a + (-w + down) * n),
            },
        ]
    }
}

/// The four legs: thigh or forearm, cannon and hoof each.
fn legs(r: &Rig) -> Vec<Part> {
    let mut out = Vec::new();
    for (leg, swing) in LEG_BOXES.iter().zip(r.swings()) {
        let ((y, z), rx) = ((swing.upper.0[0], swing.upper.0[1]), swing.upper.1);
        let (min, size, uv) = leg.upper;
        out.push(part([leg.x, y, z], min, size, uv).rot([rx, 0.0, 0.0]));
        let ([y, z], rx) = swing.lower;
        for (min, size, uv) in [leg.cannon, leg.hoof] {
            out.push(part([leg.x, y, z], min, size, uv).rot([rx, 0.0, 0.0]));
        }
    }
    out
}

/// The body, the tail and the neck and mane that follow the head.
fn body(r: &Rig) -> Vec<Part> {
    let dock_y = r.angry * 9.0 + r.calm * 3.0;
    // Only the server swishes the tail (`EntityHorse.type`), so the client tail follows the walk alone.
    let tail_rx = (-1.3089 + r.speed * 1.5).min(0.0);
    let tail = |min, size, uv, rx| part([0.0, dock_y, 14.0], min, size, uv).rot([rx, 0.0, 0.0]);
    vec![
        part(
            [0.0, 11.0, 9.0],
            [-5.0, -8.0, -19.0],
            [10.0, 10.0, 24.0],
            (0.0, 34.0),
        )
        .rot([r.body_rx, 0.0, 0.0]),
        tail([-1.0, -1.0, 0.0], [2.0, 2.0, 3.0], (44.0, 0.0), tail_rx),
        tail([-1.5, -2.0, 3.0], [3.0, 4.0, 7.0], (38.0, 7.0), tail_rx),
        tail(
            [-1.5, -4.5, 9.0],
            [3.0, 4.0, 7.0],
            (24.0, 3.0),
            -0.2618 + tail_rx,
        ),
        r.on_head([-2.05, -9.8, -2.0], [4.0, 14.0, 8.0], (0.0, 12.0)),
        r.on_head([-1.0, -11.5, 5.0], [2.0, 16.0, 4.0], (58.0, 0.0)),
    ]
}

/// The ears, the head and the jaws; the mouth opens the jaws.
fn head(r: &Rig) -> Vec<Part> {
    let mut out = if r.kind == 1 || r.kind == 2 {
        // The long ears of a donkey and a mule, tilted outward.
        let ear = |min, tilt| {
            let mut rot = r.head_rot();
            rot[2] = tilt;
            part(r.head_at(), min, [2.0, 7.0, 1.0], (0.0, 12.0)).rot(rot)
        };
        vec![
            ear([-2.0, -16.0, 4.0], PI / 12.0),
            ear([0.0, -16.0, 4.0], -PI / 12.0),
        ]
    } else {
        vec![
            r.on_head([0.45, -12.0, 4.0], [2.0, 3.0, 1.0], (0.0, 0.0)),
            r.on_head([-2.45, -12.0, 4.0], [2.0, 3.0, 1.0], (0.0, 0.0)),
        ]
    };
    out.push(r.on_head([-2.5, -10.0, -1.5], [5.0, 5.0, 7.0], (0.0, 0.0)));
    let parent = vec![(r.head_at(), r.head_rot())];
    let m = r.mouth;
    out.push(
        part(
            [0.0, 0.02, 0.02 - m],
            [-2.0, -10.0, -7.0],
            [4.0, 3.0, 6.0],
            (24.0, 18.0),
        )
        .rot([-0.094_247_78 * m, 0.0, 0.0])
        .under(parent.clone()),
    );
    out.push(
        part(
            [0.0, 0.0, m],
            [-2.0, -7.0, -6.5],
            [4.0, 2.0, 5.0],
            (24.0, 27.0),
        )
        .rot([PI / 20.0 * m, 0.0, 0.0])
        .under(parent),
    );
    out
}

/// The saddle, the bridle and, under a rider, the reins.
fn tack(r: &Rig) -> Vec<Part> {
    let y = r.angry * 0.5 + r.calm * 2.0;
    let z = r.angry * 11.0 + r.calm * 2.0;
    let sway = r.sway();
    let (fx, fz) = if r.ridden {
        (-PI / 3.0, 0.0)
    } else {
        (sway / 3.0, sway / 5.0)
    };
    let seat = |min, size, uv| part([0.0, y, z], min, size, uv).rot([r.body_rx, 0.0, 0.0]);
    let fender =
        |x: f32, min, size, uv, side: f32| part([x, y, z], min, size, uv).rot([fx, 0.0, fz * side]);
    let mut halster = r.on_head([-2.5, -10.1, -7.0], [5.0, 5.0, 12.0], (80.0, 12.0));
    halster.inflate = 0.2;
    let mut out = vec![
        halster,
        seat([-5.0, 0.0, -3.0], [10.0, 1.0, 8.0], (80.0, 0.0)),
        seat([-1.5, -1.0, -3.0], [3.0, 1.0, 2.0], (106.0, 9.0)),
        seat([-4.0, -1.0, 3.0], [8.0, 1.0, 2.0], (80.0, 9.0)),
        fender(5.0, [-0.5, 0.0, -0.5], [1.0, 6.0, 1.0], (70.0, 0.0), 1.0),
        fender(5.0, [-0.5, 6.0, -1.0], [1.0, 2.0, 2.0], (74.0, 0.0), 1.0),
        fender(-5.0, [-0.5, 0.0, -0.5], [1.0, 6.0, 1.0], (80.0, 0.0), -1.0),
        fender(-5.0, [-0.5, 6.0, -1.0], [1.0, 2.0, 2.0], (74.0, 4.0), -1.0),
        r.on_head([1.5, -8.0, -4.0], [1.0, 2.0, 2.0], (74.0, 13.0)),
        r.on_head([-2.5, -8.0, -4.0], [1.0, 2.0, 2.0], (74.0, 13.0)),
    ];
    if r.ridden {
        let rein =
            |min, uv| part(r.head_at(), min, [0.0, 3.0, 16.0], uv).rot([r.pitch, r.head_ry, 0.0]);
        out.push(rein([2.6, -6.0, -6.0], (44.0, 10.0)));
        out.push(rein([-2.6, -6.0, -6.0], (44.0, 5.0)));
    }
    out
}

/// The two packs of a donkey or a mule with a chest; the left one only follows the right one under a saddle.
fn chests(r: &Rig) -> Vec<Part> {
    let right = [
        4.5,
        r.angry * 5.5 + r.calm * 3.0,
        r.angry * 15.0 + r.calm * 10.0,
    ];
    let left = if r.saddled {
        [-7.5, right[1], right[2]]
    } else {
        [-7.5, 3.0, 10.0]
    };
    let sway = r.sway() / 5.0;
    vec![
        part(left, [-3.0, 0.0, 0.0], [8.0, 8.0, 3.0], (0.0, 34.0)).rot([sway, FRAC_PI_2, 0.0]),
        part(right, [-3.0, 0.0, 0.0], [8.0, 8.0, 3.0], (0.0, 47.0)).rot([-sway, FRAC_PI_2, 0.0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::*;
    use crate::entity::Class;

    /// The pose of the oracle run (`java/oracle/HorseModels.java`): head yaw 30, pitch 15, ticks 7.
    fn horse(look: HorseLook, ridden: bool, swing: f32, amount: f32) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(HORSE);
        (d.yaw, d.head_yaw, d.pitch, d.age) = (0.0, 30.0, 15.0, 7.0);
        (d.limb_swing, d.limb_swing_amount, d.ridden) = (swing, amount, ridden);
        d.look.horse = look;
        d
    }

    fn near(a: &[f32], b: &[f32]) {
        assert!(
            a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-3),
            "{a:?} vs {b:?}"
        );
    }

    /// The part with the box at texture offset `uv` and corner `min`.
    fn find(parts: &[Part], uv: (f32, f32), min: [f32; 3]) -> &Part {
        parts.iter().find(|p| p.uv == uv && p.min == min).unwrap()
    }

    #[test]
    fn walking_horse_matches_the_oracle() {
        let p = parts(&horse(HorseLook::default(), false, 1.3, 0.6));
        assert_eq!(p.len(), 23);
        let head = find(&p, (0.0, 12.0), [-2.05, -9.8, -2.0]);
        near(&head.pivot, &[0.0, 4.0, -10.0]);
        near(&head.rot, &[0.863_505_36, 0.349_065_87, 0.0]);
        let tail = find(&p, (44.0, 0.0), [-1.0, -1.0, 0.0]);
        near(&tail.rot, &[-0.408_899_96, 0.0, 0.0]);
        let leg = find(&p, (44.0, 29.0), [-1.9, -1.0, -2.1]);
        near(&leg.pivot, &[4.0, 9.0, -8.0]);
    }

    #[test]
    fn saddled_rider_rears_with_reins_and_the_donkey_carries_chests() {
        let look = HorseLook {
            flags: 4,
            angry: 0.6,
            mouth: 0.5,
            ..HorseLook::default()
        };
        let p = parts(&horse(look, true, 0.4, 0.3));
        assert_eq!(p.len(), 35);
        near(
            &find(&p, (0.0, 34.0), [-5.0, -8.0, -19.0]).rot,
            &[-0.471_238_9, 0.0, 0.0],
        );
        let reins = HorseLook {
            kind: 1,
            flags: 8,
            ..HorseLook::default()
        };
        assert_eq!(parts(&horse(reins, false, 1.3, 0.6)).len(), 25);
    }

    #[test]
    fn the_coat_picks_the_texture_and_armor_a_second_pass() {
        let look = HorseLook {
            variant: 3 | (2 << 8),
            armor: 2,
            ..HorseLook::default()
        };
        assert_eq!(body_skin(&look), Some(tex::marked(3, 2)));
        assert_eq!(armor_skin(&look), Some(tex::ARMOR_FIRST + 1));
        assert_eq!(
            armor_skin(&HorseLook {
                kind: 1,
                armor: 2,
                ..look
            }),
            None
        );
        let d = horse(look, false, 0.0, 0.0);
        let m = build(&d, HORSE).unwrap();
        assert_eq!(layers(&d, &m).len(), 1);
    }

    #[test]
    fn a_baby_horse_scales_legs_body_and_head_apart() {
        let mut d = horse(HorseLook::default(), false, 0.0, 0.0);
        d.child = true;
        let p = parts(&d);
        let body = find(&p, (0.0, 34.0), [-5.0, -8.0, -19.0]);
        assert_eq!((body.scale, body.shift[1]), (0.5, 1.35 * 0.5 * 16.0));
        let leg = find(&p, (78.0, 29.0), [-2.5, -2.0, -2.5]);
        assert!((leg.stretch_y - 1.5).abs() < 1e-6);
    }
}
