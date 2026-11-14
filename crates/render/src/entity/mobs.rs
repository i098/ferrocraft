//! Models of the villager, iron golem, silverfish, chicken, sheep, spider, cave spider and enderman,
//! ported from `ModelVillager`, `ModelIronGolem`, `ModelSilverfish`, `ModelChicken`, `ModelSheep1/2`,
//! `ModelSpider` and `ModelEnderman` (box layout, texture offsets and `setRotationAngles`), plus the
//! sheep wool and the spider and enderman eye layers.
//!
//! Not ported: the eat-grass head, the chicken flap (the draw has no on-ground state), the golem
//! attack and rose arms, villager head shake, and baby proportions.

use super::{cos, quad_legs, wrap, EntityDraw, Model, Part, Skin};
use std::f32::consts::{FRAC_PI_2, PI};

pub(super) const VILLAGER: u8 = 120;
pub(super) const IRON_GOLEM: u8 = 99;
pub(super) const SILVERFISH: u8 = 60;
pub(super) const CHICKEN: u8 = 93;
pub(super) const SHEEP: u8 = 91;
pub(super) const SPIDER: u8 = 52;
pub(super) const CAVE_SPIDER: u8 = 59;
pub(super) const ENDERMAN: u8 = 58;

/// A second pass over the model with another texture: sheep wool, glowing eyes.
pub(super) struct Layer {
    pub skin: Skin,
    pub tint: [f32; 3],
    pub parts: Vec<Part>,
    /// Drawn full-bright (`LayerSpiderEyes`, `LayerEndermanEyes`).
    pub bright: bool,
}

pub(super) fn head_rot(d: &EntityDraw) -> [f32; 3] {
    [
        d.pitch.to_radians(),
        wrap(d.head_yaw - d.yaw).to_radians(),
        0.0,
    ]
}

pub(super) fn model(skin: Skin, parts: Vec<Part>) -> Model {
    Model {
        skin,
        tint: [1.0; 3],
        parts,
    }
}

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    Some(match id {
        VILLAGER => villager(d),
        IRON_GOLEM => iron_golem(d),
        SILVERFISH => silverfish(d),
        CHICKEN => chicken(d),
        SHEEP => sheep(d),
        SPIDER => spider(d, Skin::Spider),
        CAVE_SPIDER => spider(d, Skin::CaveSpider),
        ENDERMAN => enderman(d),
        _ => return None,
    })
}

/// `ModelVillager`: the nose and the three arm boxes share their parent's pivot.
fn villager(d: &EntityDraw) -> Model {
    let hr = head_rot(d);
    let leg = cos(d.limb_swing * 0.6662) * 1.4 * d.limb_swing_amount * 0.5;
    let mut robe = Part::new([0.0; 3], [-4.0, 0.0, -3.0], [8.0, 12.0, 6.0], (0.0, 38.0));
    robe.size[1] = 18.0;
    robe.inflate = 0.5;
    let arms = [0.0, 3.0, -1.0];
    let crossed = [-0.75, 0.0, 0.0];
    let parts = vec![
        Part::new([0.0; 3], [-4.0, -10.0, -4.0], [8.0, 10.0, 8.0], (0.0, 0.0)).rot(hr),
        Part::new([0.0; 3], [-1.0, -3.0, -6.0], [2.0, 4.0, 2.0], (24.0, 0.0)).rot(hr),
        Part::new([0.0; 3], [-4.0, 0.0, -3.0], [8.0, 12.0, 6.0], (16.0, 20.0)),
        robe,
        Part::new(arms, [-8.0, -2.0, -2.0], [4.0, 8.0, 4.0], (44.0, 22.0)).rot(crossed),
        Part::new(arms, [4.0, -2.0, -2.0], [4.0, 8.0, 4.0], (44.0, 22.0))
            .rot(crossed)
            .mirrored(true),
        Part::new(arms, [-4.0, 2.0, -2.0], [8.0, 4.0, 4.0], (40.0, 38.0)).rot(crossed),
        Part::new(
            [-2.0, 12.0, 0.0],
            [-2.0, 0.0, -2.0],
            [4.0, 12.0, 4.0],
            (0.0, 22.0),
        )
        .rot([leg, 0.0, 0.0]),
        Part::new(
            [2.0, 12.0, 0.0],
            [-2.0, 0.0, -2.0],
            [4.0, 12.0, 4.0],
            (0.0, 22.0),
        )
        .rot([-leg, 0.0, 0.0])
        .mirrored(true),
    ];
    let skin = match d.look.variant {
        0..=4 => Skin::Villager(d.look.variant + 1),
        _ => Skin::Villager(0),
    };
    model(skin, parts)
}

/// `ModelIronGolem.func_78172_a`: a triangle wave of period `g`, in `-1..=1`.
pub(super) fn triangle(f: f32, g: f32) -> f32 {
    ((f % g - g * 0.5).abs() - g * 0.25) / (g * 0.25)
}

fn iron_golem(d: &EntityDraw) -> Model {
    let hr = head_rot(d);
    let wave = triangle(d.limb_swing, 13.0);
    let amount = d.limb_swing_amount;
    let leg = 1.5 * wave * amount;
    let arm = 1.5 * wave;
    let at = [0.0, -7.0, 0.0];
    let mut belt = Part::new(at, [-4.5, 10.0, -3.0], [9.0, 5.0, 6.0], (0.0, 70.0));
    belt.inflate = 0.5;
    let parts = vec![
        Part::new(
            [0.0, -7.0, -2.0],
            [-4.0, -12.0, -5.5],
            [8.0, 10.0, 8.0],
            (0.0, 0.0),
        )
        .rot(hr),
        Part::new(
            [0.0, -7.0, -2.0],
            [-1.0, -5.0, -7.5],
            [2.0, 4.0, 2.0],
            (24.0, 0.0),
        )
        .rot(hr),
        Part::new(at, [-9.0, -2.0, -6.0], [18.0, 12.0, 11.0], (0.0, 40.0)),
        belt,
        Part::new(at, [-13.0, -2.5, -3.0], [4.0, 30.0, 6.0], (60.0, 21.0)).rot([
            (-0.2 + arm) * amount,
            0.0,
            0.0,
        ]),
        Part::new(at, [9.0, -2.5, -3.0], [4.0, 30.0, 6.0], (60.0, 58.0)).rot([
            (-0.2 - arm) * amount,
            0.0,
            0.0,
        ]),
        Part::new(
            [-4.0, 11.0, 0.0],
            [-3.5, -3.0, -3.0],
            [6.0, 16.0, 5.0],
            (37.0, 0.0),
        )
        .rot([-leg, 0.0, 0.0]),
        Part::new(
            [5.0, 11.0, 0.0],
            [-3.5, -3.0, -3.0],
            [6.0, 16.0, 5.0],
            (60.0, 0.0),
        )
        .rot([leg, 0.0, 0.0])
        .mirrored(true),
    ];
    model(Skin::IronGolem, parts)
}

/// Sway of the whole golem while it walks (`RenderIronGolem.rotateCorpse`), radians about the feet.
pub(super) fn golem_sway(d: &EntityDraw) -> f32 {
    if d.limb_swing_amount < 0.01 {
        return 0.0;
    }
    (6.5 * triangle(d.limb_swing + 6.0, 13.0)).to_radians()
}

/// `ModelSilverfish`: seven body segments and three wing plates that wriggle with the age.
fn silverfish(d: &EntityDraw) -> Model {
    const SIZES: [[f32; 3]; 7] = [
        [3.0, 2.0, 2.0],
        [4.0, 3.0, 2.0],
        [6.0, 4.0, 3.0],
        [3.0, 3.0, 3.0],
        [2.0, 2.0, 3.0],
        [2.0, 1.0, 2.0],
        [1.0, 1.0, 2.0],
    ];
    const UV: [(f32, f32); 7] = [
        (0.0, 0.0),
        (0.0, 4.0),
        (0.0, 9.0),
        (0.0, 16.0),
        (0.0, 22.0),
        (11.0, 0.0),
        (13.0, 4.0),
    ];
    // (box size, texture offset, body segment it follows)
    const WINGS: [([f32; 3], (f32, f32), usize); 3] = [
        ([10.0, 8.0, 3.0], (20.0, 0.0), 2),
        ([6.0, 4.0, 3.0], (20.0, 11.0), 4),
        ([6.0, 5.0, 2.0], (20.0, 18.0), 1),
    ];
    let mut zs = [0.0f32; 7];
    let mut z = -3.5;
    for i in 0..7 {
        zs[i] = z;
        if i < 6 {
            z += (SIZES[i][2] + SIZES[i + 1][2]) * 0.5;
        }
    }
    // Pivot x and y rotation of one body segment.
    let seg = |i: usize| {
        let phase = d.age * 0.9 + i as f32 * 0.15 * PI;
        let dist = (i as f32 - 2.0).abs();
        (
            phase.sin() * PI * 0.2 * dist,
            cos(phase) * PI * 0.05 * (1.0 + dist),
        )
    };
    let mut parts = Vec::new();
    for i in 0..7 {
        let (x, rot_y) = seg(i);
        parts.push(
            Part::new(
                [x, 24.0 - SIZES[i][1], zs[i]],
                [SIZES[i][0] * -0.5, 0.0, SIZES[i][2] * -0.5],
                SIZES[i],
                UV[i],
            )
            .rot([0.0, rot_y, 0.0]),
        );
    }
    for (n, (size, uv, follows)) in WINGS.into_iter().enumerate() {
        let (x, rot_y) = seg(follows);
        // The first plate keeps x 0; the other two take the x of their segment.
        let x = if n == 0 { 0.0 } else { x };
        let y = [16.0, 20.0, 19.0][n];
        parts.push(
            Part::new([x, y, zs[follows]], [size[0] * -0.5, 0.0, -1.5], size, uv)
                .rot([0.0, rot_y, 0.0]),
        );
    }
    model(Skin::Silverfish, parts)
}

/// `ModelChicken`: the wings hang unflapped, as an on-ground chicken holds them.
fn chicken(d: &EntityDraw) -> Model {
    let hr = head_rot(d);
    let (f, f1) = (d.limb_swing, d.limb_swing_amount);
    let head = [0.0, 15.0, -4.0];
    let wing = 0.0;
    let parts = vec![
        Part::new(head, [-2.0, -6.0, -2.0], [4.0, 6.0, 3.0], (0.0, 0.0)).rot(hr),
        Part::new(head, [-2.0, -4.0, -4.0], [4.0, 2.0, 2.0], (14.0, 0.0)).rot(hr),
        Part::new(head, [-1.0, -2.0, -3.0], [2.0, 2.0, 2.0], (14.0, 4.0)).rot(hr),
        Part::new(
            [0.0, 16.0, 0.0],
            [-3.0, -4.0, -3.0],
            [6.0, 8.0, 6.0],
            (0.0, 9.0),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]),
        Part::new(
            [-2.0, 19.0, 1.0],
            [-1.0, 0.0, -3.0],
            [3.0, 5.0, 3.0],
            (26.0, 0.0),
        )
        .rot([cos(f * 0.6662) * 1.4 * f1, 0.0, 0.0]),
        Part::new(
            [1.0, 19.0, 1.0],
            [-1.0, 0.0, -3.0],
            [3.0, 5.0, 3.0],
            (26.0, 0.0),
        )
        .rot([cos(f * 0.6662 + PI) * 1.4 * f1, 0.0, 0.0]),
        Part::new(
            [-4.0, 13.0, 0.0],
            [0.0, 0.0, -3.0],
            [1.0, 4.0, 6.0],
            (24.0, 13.0),
        )
        .rot([0.0, 0.0, wing]),
        Part::new(
            [4.0, 13.0, 0.0],
            [-1.0, 0.0, -3.0],
            [1.0, 4.0, 6.0],
            (24.0, 13.0),
        )
        .rot([0.0, 0.0, -wing]),
    ];
    model(Skin::Chicken, parts)
}

/// `ModelSheep2`: the shorn body; the wool is a [`Layer`].
fn sheep(d: &EntityDraw) -> Model {
    let mut parts = vec![
        Part::new(
            [0.0, 6.0, -8.0],
            [-3.0, -4.0, -6.0],
            [6.0, 6.0, 8.0],
            (0.0, 0.0),
        )
        .rot(head_rot(d)),
        Part::new(
            [0.0, 5.0, 2.0],
            [-4.0, -10.0, -7.0],
            [8.0, 16.0, 6.0],
            (28.0, 8.0),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]),
    ];
    parts.extend(quad_legs(d, 12.0, SHEEP_LEGS, (0.0, 16.0), 4.0));
    model(Skin::Sheep, parts)
}

const SHEEP_LEGS: [[f32; 3]; 4] = [
    [-3.0, 12.0, 7.0],
    [3.0, 12.0, 7.0],
    [-3.0, 12.0, -5.0],
    [3.0, 12.0, -5.0],
];

/// `EntitySheep.fleeceColorTable`, by dye meta.
const FLEECE: [[f32; 3]; 16] = [
    [1.0, 1.0, 1.0],
    [0.85, 0.5, 0.2],
    [0.7, 0.3, 0.85],
    [0.4, 0.6, 0.85],
    [0.9, 0.9, 0.2],
    [0.5, 0.8, 0.1],
    [0.95, 0.5, 0.65],
    [0.3, 0.3, 0.3],
    [0.6, 0.6, 0.6],
    [0.3, 0.5, 0.6],
    [0.5, 0.25, 0.7],
    [0.2, 0.3, 0.7],
    [0.4, 0.3, 0.2],
    [0.4, 0.5, 0.2],
    [0.6, 0.2, 0.2],
    [0.1, 0.1, 0.1],
];

/// `LayerSheepWool`: the dye color, or the jeb_ rainbow that blends one color into the next
/// every 25 ticks (the entity id offset is 0 here).
fn fleece(d: &EntityDraw) -> [f32; 3] {
    if d.name.as_deref() != Some("jeb_") {
        return FLEECE[usize::from(d.look.variant & 15)];
    }
    let step = (d.age / 25.0).floor() as usize;
    let (a, b) = (FLEECE[step % 16], FLEECE[(step + 1) % 16]);
    let t = (d.age % 25.0) / 25.0;
    [0, 1, 2].map(|i| a[i] * (1.0 - t) + b[i] * t)
}

/// `ModelSheep1`, drawn over the sheep unless it is sheared or invisible.
fn sheep_wool(d: &EntityDraw) -> Layer {
    let mut head = Part::new(
        [0.0, 6.0, -8.0],
        [-3.0, -4.0, -4.0],
        [6.0, 6.0, 6.0],
        (0.0, 0.0),
    )
    .rot(head_rot(d));
    head.inflate = 0.6;
    let mut body = Part::new(
        [0.0, 5.0, 2.0],
        [-4.0, -10.0, -7.0],
        [8.0, 16.0, 6.0],
        (28.0, 8.0),
    )
    .rot([FRAC_PI_2, 0.0, 0.0]);
    body.inflate = 1.75;
    let mut parts = vec![head, body];
    for mut leg in quad_legs(d, 6.0, SHEEP_LEGS, (0.0, 16.0), 4.0) {
        leg.inflate = 0.5;
        parts.push(leg);
    }
    Layer {
        skin: Skin::SheepFur,
        tint: fleece(d),
        parts,
        bright: false,
    }
}

/// `ModelSpider`: eight legs fanned out and walking in pairs.
fn spider(d: &EntityDraw, skin: Skin) -> Model {
    let hr = head_rot(d);
    let (f, amount) = (d.limb_swing, d.limb_swing_amount);
    let y0 = 15.0;
    let mut parts = vec![
        Part::new([0.0, y0, -3.0], [-4.0, -4.0, -8.0], [8.0; 3], (32.0, 4.0)).rot(hr),
        Part::new([0.0, y0, 0.0], [-3.0, -3.0, -3.0], [6.0; 3], (0.0, 0.0)),
        Part::new(
            [0.0, y0, 9.0],
            [-5.0, -4.0, -6.0],
            [10.0, 8.0, 12.0],
            (0.0, 12.0),
        ),
    ];
    let (base_z, base_y) = (PI / 4.0, PI / 8.0);
    // Leg pairs front to back as `ModelSpider` lists them: pivot z, fan factors and walk phase.
    let fan_z = [1.0, 0.74, 0.74, 1.0];
    let fan_y = [2.0, 1.0, -1.0, -2.0];
    let phase = [0.0, PI, FRAC_PI_2, 3.0 * FRAC_PI_2];
    let pivot_z = [2.0, 1.0, 0.0, -1.0];
    for pair in 0..4 {
        let yaw = -(cos(f * 0.6662 * 2.0 + phase[pair]) * 0.4) * amount;
        let lift = (f * 0.6662 + phase[pair]).sin().abs() * 0.4 * amount;
        // The right leg (x < 0) fans out by `+y, -z`, the left one mirrors both; the walk adds to the right.
        let right = [
            0.0,
            base_y * fan_y[pair] + yaw,
            -base_z * fan_z[pair] + lift,
        ];
        let left = [0.0, -right[1], -right[2]];
        parts.push(
            Part::new(
                [-4.0, y0, pivot_z[pair]],
                [-15.0, -1.0, -1.0],
                [16.0, 2.0, 2.0],
                (18.0, 0.0),
            )
            .rot(right),
        );
        parts.push(
            Part::new(
                [4.0, y0, pivot_z[pair]],
                [-1.0, -1.0, -1.0],
                [16.0, 2.0, 2.0],
                (18.0, 0.0),
            )
            .rot(left),
        );
    }
    model(skin, parts)
}

/// `ModelEnderman`: a `ModelBiped` with 30 pixel limbs. Both arms and legs swing at half the biped
/// angle (limited to 0.4 radians); carrying a block holds the arms forward; an angry enderman
/// lifts its head 5 pixels, which opens the mouth drawn by the hat box.
fn enderman(d: &EntityDraw) -> Model {
    let hr = head_rot(d);
    let (f, f1) = (d.limb_swing, d.limb_swing_amount);
    let limit = |v: f32| (v * 0.5).clamp(-0.4, 0.4);
    let sway_z = cos(d.age * 0.09) * 0.05 + 0.05;
    let sway_x = (d.age * 0.067).sin() * 0.05;
    let mut r_arm = [
        limit(cos(f * 0.6662 + PI) * 2.0 * f1 * 0.5 + sway_x),
        0.0,
        sway_z,
    ];
    let mut l_arm = [
        limit(cos(f * 0.6662) * 2.0 * f1 * 0.5 - sway_x),
        0.0,
        -sway_z,
    ];
    if d.look.carried != 0 {
        r_arm = [-0.5, 0.0, 0.05];
        l_arm = [-0.5, 0.0, -0.05];
    }
    let r_leg = limit(cos(f * 0.6662) * 1.4 * f1);
    let l_leg = limit(cos(f * 0.6662 + PI) * 1.4 * f1);
    let head_y = if d.look.screaming { -18.0 } else { -13.0 };
    let mut jaw = Part::new([0.0, -13.0, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 16.0)).rot(hr);
    jaw.inflate = -0.5;
    let parts = vec![
        Part::new([0.0, head_y, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0)).rot(hr),
        jaw,
        Part::new(
            [0.0, -14.0, 0.0],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            (32.0, 16.0),
        ),
        Part::new(
            [-5.0, -12.0, 0.0],
            [-1.0, -2.0, -1.0],
            [2.0, 30.0, 2.0],
            (56.0, 0.0),
        )
        .rot(r_arm),
        Part::new(
            [5.0, -12.0, 0.0],
            [-1.0, -2.0, -1.0],
            [2.0, 30.0, 2.0],
            (56.0, 0.0),
        )
        .rot(l_arm)
        .mirrored(true),
        Part::new(
            [-2.0, -5.0, 0.0],
            [-1.0, 0.0, -1.0],
            [2.0, 30.0, 2.0],
            (56.0, 0.0),
        )
        .rot([r_leg, 0.0, 0.0]),
        Part::new(
            [2.0, -5.0, 0.0],
            [-1.0, 0.0, -1.0],
            [2.0, 30.0, 2.0],
            (56.0, 0.0),
        )
        .rot([l_leg, 0.0, 0.0])
        .mirrored(true),
    ];
    model(Skin::Enderman, parts)
}

/// Second passes of the mob drawn by `m`: wool before, eyes after (they draw full-bright).
pub(super) fn layers(d: &EntityDraw, m: &Model) -> Vec<Layer> {
    let super::Class::Mob(id) = d.class else {
        return Vec::new();
    };
    let eyes = |skin| Layer {
        skin,
        tint: [1.0; 3],
        parts: m.parts.clone(),
        bright: true,
    };
    match id {
        SHEEP if d.look.variant & 16 == 0 && !d.invisible => vec![sheep_wool(d)],
        SPIDER | CAVE_SPIDER => vec![eyes(Skin::SpiderEyes)],
        ENDERMAN => vec![eyes(Skin::EndermanEyes)],
        _ => super::mobs2::layers(d),
    }
}
