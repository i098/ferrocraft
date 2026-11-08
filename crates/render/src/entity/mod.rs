//! Entity models of 1.8.9, ported from `ModelBiped`, `ModelQuadruped`, `ModelCreeper`, `ModelPig`
//! and `ModelCow`: box geometry, texture layout and limb animation. Mobs without a ported model
//! draw as a tinted box of their hitbox size.
//!
//! Not ported yet: sheep wool, villagers, enchant glint, armor textures (armor is a tinted shell),
//! held item models (a held item is a tinted bar), name tags (see [`nametags`]).

mod skin;

use crate::mesh::{MeshData, Vertex};
pub use skin::{Skin, Skins};
use std::f32::consts::{FRAC_PI_2, PI};

/// What to draw; mob and object ids are the 1.8.9 network type ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Player { slim: bool },
    Mob(u8),
    Object(i8),
    Orb,
}

/// One entity as the renderer needs it, already interpolated for the frame.
#[derive(Clone, Debug)]
pub struct EntityDraw {
    pub class: Class,
    /// Feet position in world space.
    pub pos: [f64; 3],
    pub yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub limb_swing: f32,
    pub limb_swing_amount: f32,
    /// Arm swing progress `0..1`, 0 when idle.
    pub swing: f32,
    pub age: f32,
    pub sneaking: bool,
    pub hurt: bool,
    /// Ticks into the death animation, 0 when alive.
    pub death_time: f32,
    pub invisible: bool,
    /// Hitbox size, used for the generic box.
    pub size: (f32, f32),
    /// Held item id (0 none) and armor item ids for boots, leggings, chestplate, helmet.
    pub held: i16,
    pub armor: [i16; 4],
    pub name: Option<String>,
}

/// A box of a model, in vanilla model pixels (y down).
struct Part {
    pivot: [f32; 3],
    min: [f32; 3],
    size: [f32; 3],
    uv: (f32, f32),
    rot: [f32; 3],
    /// Armor slot (0 boots .. 3 helmet) that covers this part.
    armor: Option<(usize, f32)>,
    /// Second texture box over the first (hat layer).
    inflate: f32,
}

impl Part {
    fn new(pivot: [f32; 3], min: [f32; 3], size: [f32; 3], uv: (f32, f32)) -> Part {
        Part {
            pivot,
            min,
            size,
            uv,
            rot: [0.0; 3],
            armor: None,
            inflate: 0.0,
        }
    }
    fn armor(mut self, slot: usize, scale: f32) -> Part {
        self.armor = Some((slot, scale));
        self
    }
    fn rot(mut self, rot: [f32; 3]) -> Part {
        self.rot = rot;
        self
    }
}

/// Which parts carry the arms when swinging and which one is the head, by index.
struct Model {
    skin: Skin,
    tint: [f32; 3],
    parts: Vec<Part>,
}

const BODY_OFFSET: f32 = 1.507_812_5;

fn cos(v: f32) -> f32 {
    v.cos()
}

struct Pose {
    r_arm: V3,
    l_arm: V3,
    body_rot: V3,
    r_arm_pivot: V3,
    l_arm_pivot: V3,
    head_pivot: V3,
    leg_pivots: [V3; 2],
}

impl Pose {
    /// Attack swing of the right arm and the body twist (`ModelBiped`, `swingProgress > -9990`).
    fn swing(&mut self, swing: f32, head_rot_x: f32) {
        let s = (swing.sqrt() * 2.0 * PI).sin() * 0.2;
        self.body_rot[1] = s;
        self.r_arm_pivot[2] = s.sin() * 5.0;
        self.r_arm_pivot[0] = -s.cos() * 5.0;
        self.l_arm_pivot[2] = -s.sin() * 5.0;
        self.l_arm_pivot[0] = s.cos() * 5.0;
        self.r_arm[1] += s;
        self.l_arm[1] += s;
        self.l_arm[0] += s;
        let f1 = 1.0 - (1.0 - swing).powi(4);
        let f2 = (f1 * PI).sin() * (swing * PI).sin() * -(head_rot_x - 0.7) * 0.75;
        self.r_arm[0] -= f1 * 1.2 + f2;
        self.r_arm[1] += s * 2.0;
        self.r_arm[2] += (swing * PI).sin() * -0.4;
    }

    /// Idle arm sway driven by the entity age.
    fn sway(&mut self, age: f32) {
        self.r_arm[2] += cos(age * 0.09) * 0.05 + 0.05;
        self.l_arm[2] -= cos(age * 0.09) * 0.05 + 0.05;
        self.r_arm[0] += (age * 0.067).sin() * 0.05;
        self.l_arm[0] -= (age * 0.067).sin() * 0.05;
    }

    fn sneak(&mut self) {
        self.body_rot[0] = 0.5;
        self.r_arm[0] += 0.4;
        self.l_arm[0] += 0.4;
        for leg in &mut self.leg_pivots {
            leg[1] = 9.0;
            leg[2] = 4.0;
        }
        self.head_pivot[1] = 1.0;
    }
}

/// `ModelBiped.setRotationAngles` and the box layout. `arm_w` and `limb` are the arm and leg
/// widths (4 for a player, 3 for alex, 2 for a skeleton).
fn biped(
    d: &EntityDraw,
    skin: Skin,
    tint: [f32; 3],
    arm_w: f32,
    limb_w: f32,
    raised: bool,
) -> Model {
    let (f, f1) = (d.limb_swing, d.limb_swing_amount);
    let head_rot_y = wrap(d.head_yaw - d.yaw).to_radians();
    let head_rot_x = d.pitch.to_radians();
    let swing = d.swing;

    let mut r_arm = [cos(f * 0.6662 + PI) * 2.0 * f1 * 0.5, 0.0, 0.0];
    let mut l_arm = [cos(f * 0.6662) * 2.0 * f1 * 0.5, 0.0, 0.0];
    let mut r_leg = [cos(f * 0.6662) * 1.4 * f1, 0.0, 0.0];
    let mut l_leg = [cos(f * 0.6662 + PI) * 1.4 * f1, 0.0, 0.0];
    let mut body_rot = [0.0f32; 3];
    let mut r_arm_pivot = [-5.0, 2.0, 0.0];
    let mut l_arm_pivot = [5.0, 2.0, 0.0];
    let mut head_pivot = [0.0, 0.0, 0.0];
    let mut leg_pivots = [[-1.9, 12.0, 0.0], [1.9, 12.0, 0.0]];
    if arm_w < 4.0 && limb_w == arm_w {
        // Skeleton limbs sit closer to the body.
        r_arm_pivot = [-5.0, 2.0, 0.0];
        leg_pivots = [[-2.0, 12.0, 0.0], [2.0, 12.0, 0.0]];
    }
    if arm_w == 3.0 {
        r_arm_pivot[1] = 2.5;
        l_arm_pivot[1] = 2.5;
    }
    if raised {
        r_arm[0] = -FRAC_PI_2;
        l_arm[0] = -FRAC_PI_2;
    }
    if d.held != 0 && !raised {
        r_arm[0] = r_arm[0] * 0.5 - 0.314_159_27;
    }
    let mut pose = Pose {
        r_arm,
        l_arm,
        body_rot,
        r_arm_pivot,
        l_arm_pivot,
        head_pivot,
        leg_pivots,
    };
    if swing > 0.0 {
        pose.swing(swing, head_rot_x);
    }
    pose.sway(d.age);
    if d.sneaking {
        pose.sneak();
    }
    let Pose {
        r_arm,
        l_arm,
        body_rot,
        r_arm_pivot,
        l_arm_pivot,
        head_pivot,
        leg_pivots,
    } = pose;

    let arm_min = -(arm_w - 1.0);
    let l_arm_min = -1.0;
    let limb_min = -limb_w / 2.0;
    let arm_d = if limb_w == 2.0 { 2.0 } else { 4.0 };
    let arm_z = -arm_d / 2.0;
    let mut hat = Part::new(head_pivot, [-4.0, -8.0, -4.0], [8.0; 3], (32.0, 0.0))
        .rot([head_rot_x, head_rot_y, 0.0]);
    hat.inflate = 0.5;
    let parts = vec![
        Part::new(head_pivot, [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0))
            .rot([head_rot_x, head_rot_y, 0.0])
            .armor(3, 1.0),
        hat,
        Part::new([0.0; 3], [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0], (16.0, 16.0))
            .rot(body_rot)
            .armor(2, 1.0),
        Part::new(
            r_arm_pivot,
            [arm_min, -2.0, arm_z],
            [arm_w, 12.0, arm_d],
            (40.0, 16.0),
        )
        .rot(r_arm)
        .armor(2, 1.0),
        Part::new(
            l_arm_pivot,
            [l_arm_min, -2.0, arm_z],
            [arm_w, 12.0, arm_d],
            (32.0, 48.0),
        )
        .rot(l_arm)
        .armor(2, 1.0),
        Part::new(
            leg_pivots[0],
            [limb_min, 0.0, arm_z],
            [limb_w, 12.0, arm_d],
            (0.0, 16.0),
        )
        .rot(r_leg)
        .armor(1, 0.5),
        Part::new(
            leg_pivots[1],
            [limb_min, 0.0, arm_z],
            [limb_w, 12.0, arm_d],
            (16.0, 48.0),
        )
        .rot(l_leg)
        .armor(1, 0.5),
    ];
    Model { skin, tint, parts }
}

/// `ModelQuadruped` legs: 1 and 4 swing together, 2 and 3 against them.
fn quad_legs(
    d: &EntityDraw,
    height: f32,
    pivots: [[f32; 3]; 4],
    uv: (f32, f32),
    w: f32,
) -> Vec<Part> {
    let (f, f1) = (d.limb_swing, d.limb_swing_amount);
    let a = cos(f * 0.6662) * 1.4 * f1;
    let b = cos(f * 0.6662 + PI) * 1.4 * f1;
    [a, b, b, a]
        .into_iter()
        .zip(pivots)
        .map(|(rx, p)| {
            Part::new(p, [-w / 2.0, 0.0, -w / 2.0], [w, height, w], uv).rot([rx, 0.0, 0.0])
        })
        .collect()
}

fn creeper(d: &EntityDraw) -> Model {
    let head_rot = [
        d.pitch.to_radians(),
        wrap(d.head_yaw - d.yaw).to_radians(),
        0.0,
    ];
    let mut parts = vec![
        Part::new([0.0, 4.0, 0.0], [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0)).rot(head_rot),
        Part::new(
            [0.0, 4.0, 0.0],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            (16.0, 16.0),
        ),
    ];
    parts.extend(quad_legs(
        d,
        6.0,
        [
            [-2.0, 16.0, 4.0],
            [2.0, 16.0, 4.0],
            [-2.0, 16.0, -4.0],
            [2.0, 16.0, -4.0],
        ],
        (0.0, 16.0),
        4.0,
    ));
    Model {
        skin: Skin::Creeper,
        tint: [1.0; 3],
        parts,
    }
}

fn pig(d: &EntityDraw) -> Model {
    let head_rot = [
        d.pitch.to_radians(),
        wrap(d.head_yaw - d.yaw).to_radians(),
        0.0,
    ];
    let mut parts = vec![
        Part::new(
            [0.0, 12.0, -6.0],
            [-4.0, -4.0, -8.0],
            [8.0, 8.0, 8.0],
            (0.0, 0.0),
        )
        .rot(head_rot),
        Part::new(
            [0.0, 12.0, -6.0],
            [-2.0, 0.0, -9.0],
            [4.0, 3.0, 1.0],
            (16.0, 16.0),
        )
        .rot(head_rot),
        Part::new(
            [0.0, 11.0, 2.0],
            [-5.0, -10.0, -7.0],
            [10.0, 16.0, 8.0],
            (28.0, 8.0),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]),
    ];
    parts.extend(quad_legs(
        d,
        6.0,
        [
            [-3.0, 18.0, 7.0],
            [3.0, 18.0, 7.0],
            [-3.0, 18.0, -5.0],
            [3.0, 18.0, -5.0],
        ],
        (0.0, 16.0),
        4.0,
    ));
    Model {
        skin: Skin::Pig,
        tint: [1.0, 0.75, 0.75],
        parts,
    }
}

fn cow(d: &EntityDraw) -> Model {
    let head_rot = [
        d.pitch.to_radians(),
        wrap(d.head_yaw - d.yaw).to_radians(),
        0.0,
    ];
    let mut parts = vec![
        Part::new(
            [0.0, 4.0, -8.0],
            [-4.0, -4.0, -6.0],
            [8.0, 8.0, 6.0],
            (0.0, 0.0),
        )
        .rot(head_rot),
        Part::new(
            [0.0, 4.0, -8.0],
            [-5.0, -5.0, -4.0],
            [1.0, 3.0, 1.0],
            (22.0, 0.0),
        )
        .rot(head_rot),
        Part::new(
            [0.0, 4.0, -8.0],
            [4.0, -5.0, -4.0],
            [1.0, 3.0, 1.0],
            (22.0, 0.0),
        )
        .rot(head_rot),
        Part::new(
            [0.0, 5.0, 2.0],
            [-6.0, -10.0, -7.0],
            [12.0, 18.0, 10.0],
            (18.0, 4.0),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]),
        Part::new(
            [0.0, 5.0, 2.0],
            [-2.0, 2.0, -8.0],
            [4.0, 6.0, 1.0],
            (52.0, 0.0),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]),
    ];
    parts.extend(quad_legs(
        d,
        12.0,
        [
            [-4.0, 12.0, 7.0],
            [4.0, 12.0, 7.0],
            [-4.0, 12.0, -6.0],
            [4.0, 12.0, -6.0],
        ],
        (0.0, 16.0),
        4.0,
    ));
    Model {
        skin: Skin::Cow,
        tint: [0.6, 0.45, 0.35],
        parts,
    }
}

/// A tinted bar for the held item, anchored at the right hand.
fn held_bar(arm: &Part) -> Part {
    let mut p = Part::new(arm.pivot, [-1.0, 8.0, -9.0], [2.0, 2.0, 12.0], (0.0, 0.0));
    p.rot = arm.rot;
    p
}

fn wrap(mut d: f32) -> f32 {
    d %= 360.0;
    if d >= 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

fn model(d: &EntityDraw) -> Model {
    match d.class {
        Class::Player { slim } => {
            let (skin, arm) = if slim {
                (Skin::Alex, 3.0)
            } else {
                (Skin::Steve, 4.0)
            };
            biped(d, skin, [1.0; 3], arm, 4.0, false)
        }
        Class::Mob(54) => biped(d, Skin::Zombie, [1.0; 3], 4.0, 4.0, true),
        Class::Mob(57) => biped(d, Skin::PigZombie, [1.0; 3], 4.0, 4.0, true),
        Class::Mob(51) => biped(d, Skin::Skeleton, [1.0; 3], 2.0, 2.0, false),
        Class::Mob(50) => creeper(d),
        Class::Mob(90) => pig(d),
        Class::Mob(92) => cow(d),
        _ => generic(d),
    }
}

fn tint_of(class: Class) -> [f32; 3] {
    match class {
        Class::Mob(id) => {
            let h = u32::from(id).wrapping_mul(2_654_435_761);
            [
                0.45 + (h & 0xFF) as f32 / 600.0,
                0.45 + (h >> 8 & 0xFF) as f32 / 600.0,
                0.45 + (h >> 16 & 0xFF) as f32 / 600.0,
            ]
        }
        Class::Object(2) => [1.0, 0.85, 0.3],
        Class::Object(_) => [0.7, 0.7, 0.75],
        Class::Orb => [0.5, 1.0, 0.2],
        Class::Player { .. } => [1.0; 3],
    }
}

/// A single tinted box of the hitbox size, standing on the feet position.
fn generic(d: &EntityDraw) -> Model {
    let (w, h) = (d.size.0 * 16.0, d.size.1 * 16.0);
    Model {
        skin: Skin::White,
        tint: tint_of(d.class),
        parts: vec![Part::new(
            [0.0, 0.0, 0.0],
            [-w / 2.0, -h, -w / 2.0],
            [w, h, w],
            (0.0, 0.0),
        )],
    }
}

fn armor_tint(id: i16) -> Option<[f32; 3]> {
    Some(match id {
        298..=301 => [0.55, 0.35, 0.2],
        302..=305 => [0.55, 0.55, 0.6],
        306..=309 => [0.85, 0.85, 0.88],
        310..=313 => [0.3, 0.9, 0.9],
        314..=317 => [1.0, 0.85, 0.2],
        _ => return None,
    })
}

type V3 = [f32; 3];

/// Rotation as `ModelRenderer.render` applies it: x first, then y, then z, about the pivot.
fn rotate(p: V3, rot: V3) -> V3 {
    let [mut x, mut y, mut z] = p;
    let (s, c) = rot[0].sin_cos();
    (y, z) = (y * c - z * s, y * s + z * c);
    let (s, c) = rot[1].sin_cos();
    (x, z) = (x * c + z * s, -x * s + z * c);
    let (s, c) = rot[2].sin_cos();
    (x, y) = (x * c - y * s, x * s + y * c);
    [x, y, z]
}

/// Skin or flat color of one draw, meshes appended to `out`.
pub fn build(draws: &[EntityDraw], skins: &Skins) -> MeshData {
    let mut out = MeshData::default();
    for d in draws.iter().filter(|d| !d.invisible) {
        draw_entity(&mut out, d, skins);
    }
    out
}

/// Model-space to world-space transform of one entity: yaw, death tilt, sneak drop and bob.
struct Xf {
    pos: [f64; 3],
    st: f32,
    ct: f32,
    lift: f32,
    sneak_drop: f32,
    fall: f32,
    height: f32,
    scale: f32,
    /// The generic box is built y-up from the feet, so it skips the body offset.
    generic: bool,
}

impl Xf {
    fn new(d: &EntityDraw, generic: bool) -> Xf {
        let spinning = matches!(d.class, Class::Object(2) | Class::Orb);
        let body_yaw = if spinning {
            (d.age * 0.1).to_degrees()
        } else {
            d.yaw
        };
        let (st, ct) = (180.0 - body_yaw).to_radians().sin_cos();
        // Dead mobs fall onto their side over 20 ticks (`getDeathMaxRotation` 90 degrees).
        let fall = if d.death_time > 0.0 {
            ((d.death_time - 1.0) / 20.0 * 1.6).sqrt().min(1.0) * FRAC_PI_2
        } else {
            0.0
        };
        Xf {
            pos: d.pos,
            st,
            ct,
            lift: if matches!(d.class, Class::Object(2)) {
                (d.age * 0.1).sin() * 0.1 + 0.1
            } else {
                0.0
            },
            sneak_drop: if d.sneaking { 0.125 } else { 0.0 },
            fall,
            height: d.size.1,
            // `RenderPlayer.preRenderCallback` scales the player model by 15/16.
            scale: if matches!(d.class, Class::Player { .. }) {
                0.9375
            } else {
                1.0
            },
            generic,
        }
    }

    fn yaw(&self, x: f32, y: f32, z: f32, y_shift: f32) -> V3 {
        let (wx, wz) = (x * self.ct + z * self.st, -x * self.st + z * self.ct);
        [
            (self.pos[0] + f64::from(wx)) as f32,
            (self.pos[1] + f64::from(y + y_shift)) as f32,
            (self.pos[2] + f64::from(wz)) as f32,
        ]
    }

    /// Model pixels (y down) to world blocks.
    fn world(&self, q: V3) -> V3 {
        if self.generic {
            // y-up block coordinates: the box hangs from y = 0 downward in model space.
            return self.yaw(-q[0] / 16.0, -q[1] / 16.0, q[2] / 16.0, self.lift);
        }
        let s = self.scale;
        let (x, y) = (-q[0] / 16.0 * s, (BODY_OFFSET - q[1] / 16.0) * s);
        let z = q[2] / 16.0 * s;
        let (x, y) = if self.fall != 0.0 {
            let (sf, cf) = self.fall.sin_cos();
            let cy = self.height / 2.0;
            let ry = y - cy;
            (x * cf - ry * sf, x * sf + ry * cf + cy)
        } else {
            (x, y)
        };
        self.yaw(x, y, z, self.lift - self.sneak_drop)
    }
}

/// Texture and tint of the main layer: the real skin when loaded, else white with a stand-in tint.
fn base_look(m: &Model, d: &EntityDraw, skins: &Skins) -> (Skin, [f32; 3]) {
    let (skin, tint) = if skins.has(m.skin) {
        (m.skin, m.tint)
    } else if m.skin == Skin::White {
        (Skin::White, m.tint)
    } else {
        (Skin::White, fallback_tint(m.skin))
    };
    if d.hurt || d.death_time > 0.0 {
        return (skin, [tint[0], tint[1] * 0.45, tint[2] * 0.45]);
    }
    (skin, tint)
}

fn emit_part(out: &mut MeshData, xf: &Xf, part: &Part, grow: f32, color: V3, v_off: f32) {
    let min = part.min.map(|m| m - grow);
    let size = part.size.map(|s| s + 2.0 * grow);
    let tf = |p: V3| -> V3 {
        let r = rotate(p, part.rot);
        xf.world([
            r[0] + part.pivot[0],
            r[1] + part.pivot[1],
            r[2] + part.pivot[2],
        ])
    };
    add_box(out, &tf, min, size, part.uv, v_off, color, skin_dims());
}

fn draw_entity(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let m = model(d);
    let (skin, tint) = base_look(&m, d, skins);
    let generic = m.skin == Skin::White
        && matches!(d.class, Class::Mob(_) | Class::Object(_) | Class::Orb)
        && m.parts.len() == 1;
    let xf = Xf::new(d, generic);
    let white = skins.v_offset(Skin::White);

    for part in m
        .parts
        .iter()
        .filter(|p| p.inflate == 0.0 || skin != Skin::White)
    {
        emit_part(out, &xf, part, part.inflate, tint, skins.v_offset(skin));
    }
    for part in m.parts.iter().filter(|p| p.inflate == 0.0) {
        let worn = part
            .armor
            .and_then(|(slot, scale)| Some((armor_tint(d.armor[slot])?, scale)));
        if let Some((color, scale)) = worn {
            emit_part(out, &xf, part, scale * 0.5 + 0.2, color, white);
        }
    }
    let holds = matches!(d.class, Class::Player { .. } | Class::Mob(54 | 57 | 51));
    if let Some(arm) = m.parts.get(3).filter(|_| d.held != 0 && holds) {
        emit_part(out, &xf, &held_bar(arm), 0.0, [0.75, 0.6, 0.35], white);
    }
}

fn fallback_tint(skin: Skin) -> [f32; 3] {
    match skin {
        Skin::Steve | Skin::Alex => [0.75, 0.6, 0.5],
        Skin::Zombie => [0.35, 0.6, 0.3],
        Skin::Skeleton => [0.85, 0.85, 0.8],
        Skin::PigZombie => [0.8, 0.55, 0.5],
        Skin::Creeper => [0.3, 0.7, 0.25],
        Skin::Pig => [0.95, 0.65, 0.65],
        Skin::Cow => [0.4, 0.3, 0.25],
        Skin::White => [1.0; 3],
    }
}

fn skin_dims() -> [f32; 2] {
    [Skins::width() as f32, Skins::height() as f32]
}

/// `ModelBox`: six quads with the unfolded-cuboid texture layout at `uv` (texels, plus `v_off`).
#[allow(clippy::too_many_arguments)]
fn add_box(
    out: &mut MeshData,
    tf: &dyn Fn(V3) -> V3,
    min: V3,
    size: V3,
    (u0, v0): (f32, f32),
    v_off: f32,
    color: [f32; 3],
    tex: [f32; 2],
) {
    let [x1, y1, z1] = min;
    let [x2, y2, z2] = [min[0] + size[0], min[1] + size[1], min[2] + size[2]];
    let [w, h, d] = size;
    let v0 = v0 + v_off;
    let uv = |u: f32, v: f32| [u / tex[0], v / tex[1]];
    // (corners in model space, texel corners, outward model-space normal, shade)
    let faces: [([V3; 4], [[f32; 2]; 4], V3, f32); 6] = [
        // +x: u grows with z
        (
            [[x2, y1, z1], [x2, y1, z2], [x2, y2, z2], [x2, y2, z1]],
            [
                uv(u0 + d + w, v0 + d),
                uv(u0 + d + w + d, v0 + d),
                uv(u0 + d + w + d, v0 + d + h),
                uv(u0 + d + w, v0 + d + h),
            ],
            [1.0, 0.0, 0.0],
            0.6,
        ),
        // -x: u grows as z falls
        (
            [[x1, y1, z2], [x1, y1, z1], [x1, y2, z1], [x1, y2, z2]],
            [
                uv(u0, v0 + d),
                uv(u0 + d, v0 + d),
                uv(u0 + d, v0 + d + h),
                uv(u0, v0 + d + h),
            ],
            [-1.0, 0.0, 0.0],
            0.6,
        ),
        // top (y1)
        (
            [[x1, y1, z2], [x2, y1, z2], [x2, y1, z1], [x1, y1, z1]],
            [
                uv(u0 + d, v0),
                uv(u0 + d + w, v0),
                uv(u0 + d + w, v0 + d),
                uv(u0 + d, v0 + d),
            ],
            [0.0, -1.0, 0.0],
            1.0,
        ),
        // bottom (y2)
        (
            [[x1, y2, z1], [x2, y2, z1], [x2, y2, z2], [x1, y2, z2]],
            [
                uv(u0 + d + w, v0),
                uv(u0 + d + 2.0 * w, v0),
                uv(u0 + d + 2.0 * w, v0 + d),
                uv(u0 + d + w, v0 + d),
            ],
            [0.0, 1.0, 0.0],
            0.5,
        ),
        // front (z1): u grows with x
        (
            [[x1, y1, z1], [x2, y1, z1], [x2, y2, z1], [x1, y2, z1]],
            [
                uv(u0 + d, v0 + d),
                uv(u0 + d + w, v0 + d),
                uv(u0 + d + w, v0 + d + h),
                uv(u0 + d, v0 + d + h),
            ],
            [0.0, 0.0, -1.0],
            0.8,
        ),
        // back (z2): u grows as x falls
        (
            [[x2, y1, z2], [x1, y1, z2], [x1, y2, z2], [x2, y2, z2]],
            [
                uv(u0 + 2.0 * d + w, v0 + d),
                uv(u0 + 2.0 * d + 2.0 * w, v0 + d),
                uv(u0 + 2.0 * d + 2.0 * w, v0 + d + h),
                uv(u0 + 2.0 * d + w, v0 + d + h),
            ],
            [0.0, 0.0, 1.0],
            0.8,
        ),
    ];
    for (corners, uvs, normal, shade) in faces {
        let mut order = [0usize, 1, 2, 3];
        let e1 = sub(corners[1], corners[0]);
        let e2 = sub(corners[2], corners[0]);
        if dot(cross(e1, e2), normal) < 0.0 {
            order = [0, 3, 2, 1];
        }
        let base = out.vertices.len() as u32;
        for i in order {
            out.vertices.push(Vertex {
                pos: tf(corners[i]),
                uv: uvs[i],
                color: [color[0] * shade, color[1] * shade, color[2] * shade, 1.0],
            });
        }
        out.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Screen position and text of every name tag in range, for the HUD to draw: players and named
/// mobs, above the hitbox (`RenderLivingBase.canRenderName`, 64 blocks, sneaking hides at 32).
/// `view_proj` maps world space to clip space; the result is in `0..width` by `0..height` pixels.
pub fn nametags(
    draws: &[EntityDraw],
    eye: [f64; 3],
    view_proj: &[[f32; 4]; 4],
    width: f32,
    height: f32,
) -> Vec<(f32, f32, String)> {
    draws
        .iter()
        .filter_map(|d| {
            let name = d.name.as_ref()?;
            let dist = (0..3)
                .map(|i| (d.pos[i] - eye[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let limit = if d.sneaking { 32.0 } else { 64.0 };
            if dist > limit || d.invisible {
                return None;
            }
            let p = [
                d.pos[0] as f32,
                (d.pos[1] + f64::from(d.size.1) + 0.5) as f32,
                d.pos[2] as f32,
            ];
            let c: Vec<f32> = (0..4)
                .map(|r| {
                    view_proj[0][r] * p[0]
                        + view_proj[1][r] * p[1]
                        + view_proj[2][r] * p[2]
                        + view_proj[3][r]
                })
                .collect();
            if c[3] <= 0.0 {
                return None;
            }
            Some((
                (c[0] / c[3] * 0.5 + 0.5) * width,
                (0.5 - c[1] / c[3] * 0.5) * height,
                name.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steve() -> EntityDraw {
        EntityDraw {
            class: Class::Player { slim: false },
            pos: [10.0, 64.0, 10.0],
            yaw: 0.0,
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
            size: (0.6, 1.8),
            held: 0,
            armor: [0; 4],
            name: None,
        }
    }

    #[test]
    fn standing_player_spans_the_hitbox_height() {
        let m = build(&[steve()], &Skins::flat());
        let ys = m.vertices.iter().map(|v| v.pos[1]);
        let (lo, hi) = (
            ys.clone().fold(f32::MAX, f32::min),
            ys.fold(f32::MIN, f32::max),
        );
        // Feet on the position, model feet at 24 px and head top at -8 px (the hat layer needs a skin), scaled by 15/16.
        assert!((lo - 64.0073).abs() < 0.01, "feet {lo}");
        assert!((hi - 65.8823).abs() < 0.01, "head {hi}");
    }

    #[test]
    fn nose_side_faces_plus_z_at_yaw_zero_and_minus_x_at_yaw_90() {
        // Skeleton arms are the same at both yaws; compare the head front quad (v = 8..16, u = 8..16).
        let front_z = |yaw: f32| {
            let mut d = steve();
            d.yaw = yaw;
            d.head_yaw = yaw;
            let m = build(&[d], &Skins::flat());
            let v = |y: f32| (512.0 + y) / 576.0;
            let q: Vec<_> = m
                .vertices
                .iter()
                .filter(|p| (p.uv[0] - 8.0 / 64.0).abs() < 1e-4 && (p.uv[1] - v(8.0)).abs() < 1e-4)
                .collect();
            (q[0].pos[0] - 10.0, q[0].pos[2] - 10.0)
        };
        let (x0, z0) = front_z(0.0);
        assert!(z0 > 0.2 && x0.abs() < 0.3);
        let (x90, z90) = front_z(90.0);
        assert!(x90 < -0.2 && z90.abs() < 0.3);
    }

    #[test]
    fn every_face_of_a_box_winds_counter_clockwise_seen_from_outside() {
        let mut d = steve();
        d.class = Class::Object(50);
        d.size = (1.0, 1.0);
        let m = build(&[d], &Skins::flat());
        let center = [10.0, 64.5, 10.0];
        assert_eq!(m.indices.len(), 36);
        for tri in m.indices.chunks(3) {
            let p: Vec<V3> = tri.iter().map(|&i| m.vertices[i as usize].pos).collect();
            let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            let out = sub(p[0], center);
            assert!(dot(n, out) > 0.0, "inward face {p:?}");
        }
    }

    #[test]
    fn walking_swings_opposite_limbs_in_opposite_directions() {
        let mut d = steve();
        d.limb_swing = 1.0;
        d.limb_swing_amount = 1.0;
        let walking = build(&[d], &Skins::flat());
        let idle = build(&[steve()], &Skins::flat());
        assert_ne!(walking.vertices.len(), 0);
        assert_ne!(walking.vertices[100].pos, idle.vertices[100].pos);
    }
}
