//! Entity models of 1.8.9, ported from `ModelBiped`, `ModelQuadruped`, `ModelCreeper`, `ModelPig`
//! and `ModelCow` here and from the villager, golem, silverfish, chicken, sheep, spider and
//! enderman models in [`mobs`]: box geometry, texture layout and limb animation. Mobs without a
//! ported model draw as a tinted box of their hitbox size.
//!
//! Not ported yet: sheep wool, villagers. Held items and dropped items are in [`item`]. Name tags
//! are laid out by [`nametags`].

mod block;
pub(crate) mod fire;
pub(crate) mod guardian;
pub mod hanging;
pub mod item;
mod mobs;
mod projectile;
mod skin;

// Indices of the player model's second skin layer in `Model::parts`, after the 7 base boxes.
const PLAYER_JACKET: usize = 7;
const PLAYER_RIGHT_SLEEVE: usize = 8;
const PLAYER_LEFT_SLEEVE: usize = 9;
const PLAYER_RIGHT_PANTS: usize = 10;
const PLAYER_LEFT_PANTS: usize = 11;

use crate::hand::UseAction;
use crate::mesh::{MeshData, Vertex};
pub use projectile::Thrower;
pub use skin::{Skin, Skins, SLOT};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_6, PI};

/// What to draw; mob and object ids are the 1.8.9 network type ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Player {
        slim: bool,
    },
    Mob(u8),
    Object(i8),
    Orb,
    /// `EntityPainting`; item frames are `Object(71)`.
    Painting,
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
    /// `display.color` of each worn leather piece.
    pub armor_dye: [Option<i32>; 4],
    /// `ItemStack.hasEffect` of each armor piece (boots first) and of the held item.
    pub armor_glint: [bool; 4],
    pub held_glint: bool,
    /// Damage value of the held item, which picks the model of block and variant items.
    pub held_damage: i16,
    /// `display.color` of held leather armor.
    pub held_dye: Option<i32>,
    /// Dropped items (object 2): the stack shown.
    pub stack: Option<item::Stack>,
    /// `EntityItem.hoverStart`: phase of the bob and the spin.
    pub hover_start: f32,
    /// Paintings and item frames: facing, motive and item rotation.
    pub hang: hanging::Hang,
    pub name: Option<String>,
    /// Player whose downloaded skin to draw; the default skin when none is stored.
    pub skin: Option<u128>,
    /// Metadata 0 bit 0: drawn full-bright (`RenderManager.renderEntityStatic`).
    pub burning: bool,
    /// `Entity.getEyeHeight`: the light is read at feet plus this.
    pub eye_height: f32,
    /// Metadata byte 3 (`getAlwaysRenderNameTag`).
    pub always_name: bool,
    /// The entity under the crosshair (`RenderManager.pointedEntity`).
    pub pointed: bool,
    /// Scoreboard data of a player's tag; see [`TeamTag`].
    pub team_tag: TeamTag,
    /// Another entity rides this one (`riddenByEntity != null`), which hides its tag.
    pub ridden: bool,
    /// `isChild`: the tag hangs half a height lower (`RendererLivingEntity.renderName`).
    pub child: bool,
    /// Action of the timed item use in progress (`getItemUseTimer() > 0`), players only.
    pub using: Option<UseAction>,
    /// Camera yaw and pitch in degrees, for sprites that face the viewer.
    pub view: [f32; 2],
    /// Fishing bobber: world point where the line leaves the rod and the vertical eye offset.
    pub line: Option<([f64; 3], f32)>,
    /// Metadata the models read; see [`Look`].
    pub look: Look,
}

/// What the scoreboard says about a player's name tag.
#[derive(Clone, Debug, Default)]
pub struct TeamTag {
    /// The team's `nameTagVisibility` decision for this viewer; `None` for a player with no team.
    pub shown: Option<bool>,
    /// The `"<score> <objective>"` line of display slot 2 (`BELOW_NAME`).
    pub below: Option<String>,
}

/// Per-entity appearance data that comes from metadata and the profile.
#[derive(Clone, Debug)]
pub struct Look {
    /// Players: the skin parts byte (metadata 10): 1 cape, 2 jacket, 4 left sleeve, 8 right sleeve,
    /// 16 left pants, 32 right pants, 64 hat. A player with no byte shows every part.
    pub skin_parts: u8,
    /// Villager: profession (metadata 16), 0 farmer to 4 butcher. Sheep: metadata 16, dye in the low
    /// four bits and 16 when sheared.
    pub variant: u8,
    /// Enderman: id of the carried block (metadata 16), 0 for none.
    pub carried: u16,
    /// Enderman: metadata 18, open mouth.
    pub screaming: bool,
    /// Players with a cape: how it sways.
    pub cape: Option<CapeSway>,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            skin_parts: 0x7F,
            variant: 0,
            carried: 0,
            screaming: false,
            cape: None,
        }
    }
}

/// Inputs of `LayerCape`, already interpolated for the frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct CapeSway {
    /// `chasingPos - pos` of the player (`EntityPlayer.updateCape` keeps `chasingPos` trailing).
    pub chase: [f32; 3],
    /// `renderYawOffset`, degrees.
    pub body_yaw: f32,
    /// `cameraYaw`, the walking bob.
    pub camera_yaw: f32,
    /// `distanceWalkedModified`.
    pub walked: f32,
}

impl CapeSway {
    /// `LayerCape.doRenderLayer`: the cape pitch about x, and the twist about z and y, in degrees.
    fn angles(&self, sneaking: bool) -> (f32, f32) {
        let [dx, dy, dz] = self.chase;
        let (sin, cos) = self.body_yaw.to_radians().sin_cos();
        let (s, c) = (sin, -cos);
        let mut pitch = (dy * 10.0).clamp(-6.0, 32.0);
        let swing = (dx * s + dz * c) * 100.0;
        let twist = (dx * c - dz * s) * 100.0;
        pitch += (self.walked * 6.0).sin() * 32.0 * self.camera_yaw;
        if sneaking {
            pitch += 25.0;
        }
        (6.0 + swing.max(0.0) / 2.0 + pitch, twist)
    }
}

/// A box of a model, in vanilla model pixels (y down).
#[derive(Clone)]
struct Part {
    pivot: [f32; 3],
    min: [f32; 3],
    size: [f32; 3],
    uv: (f32, f32),
    rot: [f32; 3],
    /// Second texture box over the first (hat layer).
    inflate: f32,
    /// `ModelRenderer.mirror`: the box samples the opposite-hand texture layout, flipped.
    mirror: bool,
}

impl Part {
    fn new(pivot: [f32; 3], min: [f32; 3], size: [f32; 3], uv: (f32, f32)) -> Part {
        Part {
            pivot,
            min,
            size,
            uv,
            rot: [0.0; 3],
            inflate: 0.0,
            mirror: false,
        }
    }
    fn rot(mut self, rot: [f32; 3]) -> Part {
        self.rot = rot;
        self
    }
    fn mirrored(mut self, mirror: bool) -> Part {
        self.mirror = mirror;
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
        let f1 = ((1.0 - (1.0 - swing).powi(4)) * PI).sin();
        let f2 = (swing * PI).sin() * -(head_rot_x - 0.7) * 0.75;
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

    /// `ModelZombie.setRotationAngles`, which skeletons inherit: both arms point forward and replace
    /// every arm angle set so far (walk, sneak, held item); only the swing pivots stay.
    fn raise(&mut self, swing: f32, age: f32) {
        let f = (swing * PI).sin();
        let f1 = ((1.0 - (1.0 - swing) * (1.0 - swing)) * PI).sin();
        let arm_x = -FRAC_PI_2 - (f * 1.2 - f1 * 0.4);
        let sway_z = cos(age * 0.09) * 0.05 + 0.05;
        let sway_x = (age * 0.067).sin() * 0.05;
        self.r_arm = [arm_x + sway_x, -(0.1 - f * 0.6), sway_z];
        self.l_arm = [arm_x - sway_x, 0.1 - f * 0.6, -sway_z];
    }

    /// `ModelBiped.aimingBow`: both arms point where the head looks; the swing is ignored.
    fn aim_bow(&mut self, head_rot_x: f32, head_rot_y: f32, age: f32) {
        let sway_z = cos(age * 0.09) * 0.05 + 0.05;
        let sway_x = (age * 0.067).sin() * 0.05;
        let arm_x = -FRAC_PI_2 + head_rot_x;
        self.r_arm = [arm_x + sway_x, -0.1 + head_rot_y, sway_z];
        self.l_arm = [arm_x - sway_x, 0.1 + head_rot_y + 0.4, -sway_z];
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

/// `RenderPlayer.setModelVisibilities`: `itemInRightHand` is 3 while blocking, else 1.
fn hold_item(r_arm: &mut [f32; 3], blocking: bool) {
    r_arm[0] = r_arm[0] * 0.5 - 0.314_159_27 * if blocking { 3.0 } else { 1.0 };
    if blocking {
        r_arm[1] = -FRAC_PI_6;
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
    let l_arm = [cos(f * 0.6662) * 2.0 * f1 * 0.5, 0.0, 0.0];
    let r_leg = [cos(f * 0.6662) * 1.4 * f1, 0.0, 0.0];
    let l_leg = [cos(f * 0.6662 + PI) * 1.4 * f1, 0.0, 0.0];
    let body_rot = [0.0f32; 3];
    let mut r_arm_pivot = [-5.0, 2.0, 0.0];
    let mut l_arm_pivot = [5.0, 2.0, 0.0];
    let head_pivot = [0.0, 0.0, 0.0];
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
    if d.held != 0 && !raised {
        hold_item(&mut r_arm, d.using == Some(UseAction::Block));
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
    if raised {
        pose.raise(swing, d.age);
    }
    if d.using == Some(UseAction::Bow) {
        pose.aim_bow(head_rot_x, head_rot_y, d.age);
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
    // `ModelBiped` mirrors the right limb for the left one; only `ModelPlayer` has its own 64x64 regions.
    let mirrored = !matches!(d.class, Class::Player { .. });
    let (l_arm_uv, l_leg_uv) = if mirrored {
        ((40.0, 16.0), (0.0, 16.0))
    } else {
        ((32.0, 48.0), (16.0, 48.0))
    };
    let mut parts = vec![
        Part::new(head_pivot, [-4.0, -8.0, -4.0], [8.0; 3], (0.0, 0.0))
            .rot([head_rot_x, head_rot_y, 0.0]),
        hat,
        Part::new([0.0; 3], [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0], (16.0, 16.0)).rot(body_rot),
        Part::new(
            r_arm_pivot,
            [arm_min, -2.0, arm_z],
            [arm_w, 12.0, arm_d],
            (40.0, 16.0),
        )
        .rot(r_arm),
        Part::new(
            l_arm_pivot,
            [l_arm_min, -2.0, arm_z],
            [arm_w, 12.0, arm_d],
            l_arm_uv,
        )
        .rot(l_arm)
        .mirrored(mirrored),
        Part::new(
            leg_pivots[0],
            [limb_min, 0.0, arm_z],
            [limb_w, 12.0, arm_d],
            (0.0, 16.0),
        )
        .rot(r_leg),
        Part::new(
            leg_pivots[1],
            [limb_min, 0.0, arm_z],
            [limb_w, 12.0, arm_d],
            l_leg_uv,
        )
        .rot(l_leg)
        .mirrored(mirrored),
    ];
    if matches!(d.class, Class::Player { .. }) {
        // `ModelPlayer`'s second skin layer: same boxes and poses, grown by a quarter pixel.
        let layer = |base: usize, uv: (f32, f32)| {
            let mut p = parts[base].clone();
            p.uv = uv;
            p.inflate = 0.25;
            p
        };
        let wear = [
            layer(2, (16.0, 32.0)),
            layer(3, (40.0, 32.0)),
            layer(4, (48.0, 48.0)),
            layer(5, (0.0, 32.0)),
            layer(6, (0.0, 48.0)),
        ];
        parts.extend(wear);
    }
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
        tint: [1.0; 3],
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
        tint: [1.0; 3],
        parts,
    }
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
        Class::Mob(51) => biped(d, Skin::Skeleton, [1.0; 3], 2.0, 2.0, true),
        Class::Mob(50) => creeper(d),
        Class::Mob(90) => pig(d),
        Class::Mob(92) => cow(d),
        Class::Mob(id) => mobs::build(d, id).unwrap_or_else(|| generic(d)),
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
        Class::Object(_) => [0.7, 0.7, 0.75],
        Class::Orb => [0.5, 1.0, 0.2],
        Class::Painting => [1.0; 3],
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

pub(crate) type V3 = [f32; 3];
/// Corners in model space, texel corners, outward normal, shade.
type Face = ([V3; 4], [[f32; 2]; 4], V3, f32);

/// Rotation as `ModelRenderer.render` applies it: x first, then y, then z, about the pivot.
pub(crate) fn rotate(p: V3, rot: V3) -> V3 {
    let [mut x, mut y, mut z] = p;
    let (s, c) = rot[0].sin_cos();
    (y, z) = (y * c - z * s, y * s + z * c);
    let (s, c) = rot[1].sin_cos();
    (x, z) = (x * c + z * s, -x * s + z * c);
    let (s, c) = rot[2].sin_cos();
    (x, y) = (x * c - y * s, x * s + y * c);
    [x, y, z]
}

/// Texture of the enchantment glint, `RenderItem.renderEffect` and `LayerArmorBase.renderGlint`.
pub const GLINT_TEXTURE: &str = "textures/misc/enchanted_item_glint.png";

/// `LayerArmorBase.renderGlint`, pass `pass` (0 or 1): the texture matrix is scale 1/3, rotate
/// `30 - 60 * pass` degrees, translate `(0, ticks * (0.001 + 0.003 * pass) * 20)`, applied to the
/// model's own texture coordinates (`uv` in `0..1` of the 64x32 armor texture).
pub fn glint_uv_armor(uv: [f32; 2], age: f32, pass: usize) -> [f32; 2] {
    let (s, c) = (30.0 - 60.0 * pass as f32).to_radians().sin_cos();
    let (x, y) = (uv[0], uv[1] + age * (0.001 + 0.003 * pass as f32) * 20.0);
    [
        (x * c - y * s) * 0.333_333_34,
        (x * s + y * c) * 0.333_333_34,
    ]
}

/// `RenderItem.renderEffect`, pass `pass`: scale 8, translate `(+-t / 8, 0)`, rotate -50 or 10
/// degrees. `t` is `(ms % 3000) / 3000` or `(ms % 4873) / 4873`. `uv` are the item sprite's
/// coordinates in the block/item atlas; `ms` is the game clock in milliseconds.
pub fn glint_uv_item(uv: [f32; 2], ms: f32, pass: usize) -> [f32; 2] {
    let (period, angle, sign) = if pass == 0 {
        (3000.0, -50.0f32, 1.0)
    } else {
        (4873.0, 10.0f32, -1.0)
    };
    let shift = sign * (ms % period) / period / 8.0;
    let (s, c) = angle.to_radians().sin_cos();
    [
        (uv[0] * c - uv[1] * s + shift) * 8.0,
        (uv[0] * s + uv[1] * c) * 8.0,
    ]
}

/// Item sprites are 16 px of the atlas; vanilla 1.8.9 stitches a 512 px wide atlas.
pub const ITEM_UV_SCALE: f32 = 16.0 / 512.0;

/// Glint tint, 0.5 * 0.76, 0.25 * 0.76, 0.8 * 0.76 as `renderGlint` sets it.
pub const ARMOR_GLINT_COLOR: [f32; 4] = [0.38, 0.19, 0.608, 1.0];
/// Item glint tint, `0xFF8040CC` as `RenderItem.renderEffect` passes it.
pub const ITEM_GLINT_COLOR: [f32; 4] = [0.502, 0.251, 0.8, 1.0];

/// The glint layer: the glowing armor pieces and held items again, once per vanilla pass, with
/// texture coordinates that scroll over the surface. Blended `SRC_COLOR, ONE` at the same depth as
/// the model.
pub fn build_glint(draws: &[EntityDraw]) -> MeshData {
    let mut out = MeshData::default();
    for d in draws {
        let m = model(d);
        let holds = matches!(d.class, Class::Player { .. } | Class::Mob(54 | 57 | 51));
        if !holds {
            continue;
        }
        let xf = Xf::new(d, false);
        let [sw, sh] = skin_dims();
        for pass in 0..2 {
            let start = out.vertices.len();
            for (slot, parts, grow, layer) in ARMOR_PIECES {
                if d.armor_glint[slot] && Skin::armor(d.armor[slot], layer, false).is_some() {
                    for &i in parts {
                        emit_part(&mut out, &xf, &m.parts[i], grow, [1.0; 3], 0.0);
                    }
                }
            }
            for v in &mut out.vertices[start..] {
                let uv = [v.uv[0] * sw / 64.0, v.uv[1] * sh / 32.0];
                (v.uv, v.color) = (glint_uv_armor(uv, d.age, pass), ARMOR_GLINT_COLOR);
                v.light = [1.0; 2];
            }
        }
    }
    out
}

/// Skin or flat color of one draw, meshes appended to `out`. `light` gives the `[sky, block]` level
/// (`0..=1`) at a world position; burning entities skip it and stay full-bright.
pub fn build(
    draws: &[EntityDraw],
    skins: &Skins,
    light: impl Fn([f64; 3]) -> [f32; 2],
) -> MeshData {
    let mut out = MeshData::default();
    for d in draws {
        let start = out.vertices.len();
        let bright = draw_entity(&mut out, d, skins);
        let level = light_level(d, &light, true);
        let bright = bright.max(start);
        for v in &mut out.vertices[start..bright] {
            v.light = level;
        }
        for v in &mut out.vertices[bright..] {
            v.light = [1.0, 1.0];
        }
    }
    out
}

/// `[sky, block]` light of one entity; `flash` adds the hurt flag of a flashing body.
fn light_level(d: &EntityDraw, light: &impl Fn([f64; 3]) -> [f32; 2], flash: bool) -> [f32; 2] {
    let level = if d.burning {
        [1.0, 1.0]
    } else {
        light([d.pos[0], d.pos[1] + f64::from(d.eye_height), d.pos[2]])
    };
    [
        level[0],
        level[1]
            + if flash && hurt_flash(d) {
                HURT_FLAG
            } else {
                0.0
            },
    ]
}

/// Added to the block light of every vertex of a flashing entity; `fs_world` strips it and mixes
/// in `HURT_RED` (`RenderLivingBase.setBrightness`: `(1, 0, 0, 0.3)` interpolated over the lit texture).
pub const HURT_FLAG: f32 = 2.0;

/// `RenderLivingBase.setBrightness` runs for `hurtTime > 0 || deathTime > 0`.
fn hurt_flash(d: &EntityDraw) -> bool {
    d.hurt || d.death_time > 0.0
}

/// Model-space to world-space transform of one entity: yaw, death tilt, sneak drop and bob.
struct Xf {
    pos: [f64; 3],
    st: f32,
    ct: f32,
    sneak_drop: f32,
    fall: f32,
    scale: f32,
    /// The generic box is built y-up from the feet, so it skips the body offset.
    generic: bool,
}

impl Xf {
    fn new(d: &EntityDraw, generic: bool) -> Xf {
        let spinning = matches!(d.class, Class::Orb);
        let body_yaw = if spinning {
            (d.age * 0.1).to_degrees()
        } else {
            d.yaw
        };
        let (st, ct) = (180.0 - body_yaw).to_radians().sin_cos();
        // Dead mobs fall onto their side over 20 ticks (`getDeathMaxRotation` 90 degrees).
        let fall = if d.death_time > 0.0 {
            ((d.death_time - 1.0).max(0.0) / 20.0 * 1.6).sqrt().min(1.0) * FRAC_PI_2
        } else {
            0.0
        };
        // `RenderIronGolem.rotateCorpse` rolls the walking golem about the same axis.
        let fall = fall
            + if d.class == Class::Mob(mobs::IRON_GOLEM) {
                mobs::golem_sway(d)
            } else {
                0.0
            };
        Xf {
            pos: d.pos,
            st,
            ct,
            sneak_drop: if d.sneaking { 0.125 } else { 0.0 },
            fall,
            // `RenderPlayer.preRenderCallback` scales the player by 15/16, `RenderCaveSpider` the spider by 0.7.
            scale: match d.class {
                Class::Player { .. } => 0.9375,
                Class::Mob(mobs::CAVE_SPIDER) => 0.7,
                // `RenderVillager.preRenderCallback`: 15/16, halved for a child.
                Class::Mob(mobs::VILLAGER) if d.child => 0.46875,
                Class::Mob(mobs::VILLAGER) => 0.9375,
                _ => 1.0,
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
            return self.yaw(-q[0] / 16.0, -q[1] / 16.0, q[2] / 16.0, 0.0);
        }
        let s = self.scale;
        let (x, y) = (-q[0] / 16.0 * s, (BODY_OFFSET - q[1] / 16.0) * s);
        let z = q[2] / 16.0 * s;
        let (x, y) = if self.fall != 0.0 {
            // `LivingRenderer.applyRotation`: a z rotation about the feet origin.
            let (sf, cf) = self.fall.sin_cos();
            (x * cf - y * sf, x * sf + y * cf)
        } else {
            (x, y)
        };
        self.yaw(x, y, z, -self.sneak_drop)
    }
}

/// Texture and tint of the main layer: the real skin when loaded, else white with a stand-in tint.
fn base_look(m: &Model, d: &EntityDraw, skins: &Skins) -> (Skin, [f32; 3]) {
    let own = match (d.class, d.skin) {
        (Class::Player { .. }, Some(key)) => skins.player(key),
        _ => None,
    };
    let (skin, tint) = if let Some(own) = own {
        (own, m.tint)
    } else if skins.has(m.skin) {
        (m.skin, m.tint)
    } else if m.skin == Skin::White {
        (Skin::White, m.tint)
    } else {
        (Skin::White, fallback_tint(m.skin))
    };
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
    add_box(
        out,
        &tf,
        min,
        size,
        part.size,
        part.uv,
        part.mirror,
        v_off,
        color,
        skin_dims(),
    );
}

/// Mask bit of the skin parts byte that shows model part `i` of a player.
fn player_part_bit(i: usize) -> Option<u8> {
    match i {
        1 => Some(0x40),
        PLAYER_JACKET => Some(0x02),
        PLAYER_RIGHT_SLEEVE => Some(0x08),
        PLAYER_LEFT_SLEEVE => Some(0x04),
        PLAYER_RIGHT_PANTS => Some(0x20),
        PLAYER_LEFT_PANTS => Some(0x10),
        _ => None,
    }
}

/// `LayerCape.shouldCombineTextures`/`doRenderLayer`: drawn when the cape bit is on, the player is
/// visible and wears no chestplate.
fn cape_shown(d: &EntityDraw) -> bool {
    matches!(d.class, Class::Player { .. })
        && d.look.skin_parts & 0x01 != 0
        && !d.invisible
        && d.armor[2] == 0
}

/// `LayerCape`: the 10x16x1 cape box on the back, tilted by the sway.
fn emit_cape(out: &mut MeshData, xf: &Xf, d: &EntityDraw, sway: CapeSway, v_off: f32) {
    let (pitch, twist) = sway.angles(d.sneaking);
    let half = (twist / 2.0).to_radians();
    let tf = |p: V3| -> V3 {
        let q = rotate(p, [0.0, PI, 0.0]);
        let q = rotate(q, [0.0, -half, 0.0]);
        let q = rotate(q, [0.0, 0.0, half]);
        let q = rotate(q, [pitch.to_radians(), 0.0, 0.0]);
        // `translate(0, 0, 0.125)`, in model pixels.
        xf.world([q[0], q[1], q[2] + 2.0])
    };
    let size = [10.0, 16.0, 1.0];
    let (min, uv) = ([-5.0, 0.0, -1.0], (0.0, 0.0));
    add_box(
        out,
        &tf,
        min,
        size,
        size,
        uv,
        false,
        v_off,
        [1.0; 3],
        skin_dims(),
    );
}

/// `LayerHeldBlock`: the carried block as an 8 pixel cube of its flat color in front of the chest.
fn emit_carried(out: &mut MeshData, xf: &Xf, block: u16, v_off: f32) {
    let tf = |p: V3| -> V3 {
        let q = rotate([p[0], p[1] - 1.0, p[2]], [0.0, 45f32.to_radians(), 0.0]);
        let q = rotate(q, [20f32.to_radians(), 0.0, 0.0]);
        xf.world([q[0], q[1] + 11.0, q[2] - 12.0])
    };
    let color = crate::blocks::look(block << 4).color;
    let size = [8.0; 3];
    add_box(
        out,
        &tf,
        [-4.0; 3],
        size,
        size,
        (0.0, 0.0),
        false,
        v_off,
        color,
        skin_dims(),
    );
}

/// Meshes one entity into `out` and returns the index of the first full-bright vertex (the eye
/// layers), or the vertex count when there are none.
fn draw_entity(out: &mut MeshData, d: &EntityDraw, skins: &Skins) -> usize {
    if other_drawer(out, d, skins) {
        return out.vertices.len();
    }
    draw_body(out, d, skins)
}

/// Projectiles, dropped items, item frames and paintings, which skip the body model; `true` when
/// `d` was one of them.
fn other_drawer(out: &mut MeshData, d: &EntityDraw, skins: &Skins) -> bool {
    if let Class::Object(id) = d.class {
        if projectile::handles(id) {
            projectile::build(out, id, d, skins);
            return true;
        }
        // Dropped items draw through `item::build`, TNT and falling blocks through `block::build`, item
        // frames through `hanging::build`.
        if matches!(id, 2 | 50 | 70 | 71) {
            return true;
        }
    }
    d.class == Class::Painting
}

/// The body model, its armor and layers; returns where the full-bright layers start.
fn draw_body(out: &mut MeshData, d: &EntityDraw, skins: &Skins) -> usize {
    let m = model(d);
    let (skin, tint) = base_look(&m, d, skins);
    let generic = m.skin == Skin::White
        && matches!(d.class, Class::Mob(_) | Class::Object(_) | Class::Orb)
        && m.parts.len() == 1;
    let xf = Xf::new(d, generic);
    let white = skins.v_offset(Skin::White);
    let player = matches!(d.class, Class::Player { .. });
    let shown =
        |i: usize| !player || player_part_bit(i).is_none_or(|bit| d.look.skin_parts & bit != 0);

    // `LivingEntityRenderer.renderModel` skips the body of an invisible entity; layers still draw.
    for (i, part) in m
        .parts
        .iter()
        .enumerate()
        .filter(|(i, p)| !d.invisible && shown(*i) && (p.inflate == 0.0 || skin != Skin::White))
    {
        let _ = i;
        emit_part(out, &xf, part, part.inflate, tint, skins.v_offset(skin));
    }
    let holds = matches!(d.class, Class::Player { .. } | Class::Mob(54 | 57 | 51));
    if holds {
        emit_armor(out, &xf, &m, d, skins);
    }
    emit_layers(out, &xf, &m, d, skins, white)
}

/// The held item bar, the cape, the carried block and the model layers; returns where the
/// full-bright layers start.
fn emit_layers(
    out: &mut MeshData,
    xf: &Xf,
    m: &Model,
    d: &EntityDraw,
    skins: &Skins,
    white: f32,
) -> usize {
    if let (Some(sway), Some(cape)) = (d.look.cape, d.skin.and_then(|k| skins.cape(k))) {
        if cape_shown(d) {
            emit_cape(out, xf, d, sway, skins.v_offset(cape));
        }
    }
    if matches!(d.class, Class::Mob(mobs::ENDERMAN)) && d.look.carried != 0 && !d.invisible {
        emit_carried(out, xf, d.look.carried, white);
    }
    let mut bright = None;
    for layer in mobs::layers(d, m).iter().filter(|l| skins.has(l.skin)) {
        if layer.bright {
            bright.get_or_insert(out.vertices.len());
        }
        let v_off = skins.v_offset(layer.skin);
        for part in &layer.parts {
            emit_part(out, xf, part, part.inflate, layer.tint, v_off);
        }
    }
    bright.unwrap_or(out.vertices.len())
}

/// (slot: boots, leggings, chest, helmet), part indices, grow, texture layer
const ARMOR_PIECES: [(usize, &[usize], f32, u8); 4] = [
    (0, &[5, 6], 1.0, 1),
    (1, &[2, 5, 6], 0.5, 2),
    (2, &[2, 3, 4], 1.0, 1),
    (3, &[0], 1.0, 1),
];

/// `ArmorLayer`: head, chest and arms, leggings and boots draw the armor texture over the body,
/// grown by 1 pixel (layer 1) or half a pixel (leggings, layer 2).
fn emit_armor(out: &mut MeshData, xf: &Xf, m: &Model, d: &EntityDraw, skins: &Skins) {
    for (slot, parts, grow, layer) in ARMOR_PIECES {
        let item = d.armor[slot];
        let Some(base) = Skin::armor(item, layer, false) else {
            continue;
        };
        let leather = item <= 301;
        let dye = d.armor_dye[slot].unwrap_or(LEATHER_DEFAULT);
        let dye = [16, 8, 0].map(|shift| ((dye >> shift) & 0xFF) as f32 / 255.0);
        let (tint, layers) = if leather { (dye, 2) } else { ([1.0; 3], 1) };
        for pass in 0..layers {
            let Some((v_off, color)) = armor_pass(item, layer, pass, base, tint, skins) else {
                break;
            };
            for &i in parts {
                emit_part(out, xf, &m.parts[i], grow, color, v_off);
            }
        }
    }
}

/// Texture v offset and color of one armor pass; `None` when the overlay pass has no texture.
fn armor_pass(
    item: i16,
    layer: u8,
    pass: u8,
    base: Skin,
    tint: [f32; 3],
    skins: &Skins,
) -> Option<(f32, [f32; 3])> {
    let skin = if pass == 0 {
        base
    } else {
        Skin::armor(item, layer, true).filter(|o| skins.has(*o))?
    };
    Some(if skins.has(skin) {
        (
            skins.v_offset(skin),
            if pass == 0 { tint } else { [1.0; 3] },
        )
    } else {
        (
            skins.v_offset(Skin::White),
            armor_tint(item).unwrap_or([1.0; 3]),
        )
    })
}

/// Color of undyed leather armor, `ItemArmor.getColor`.
const LEATHER_DEFAULT: i32 = 0xA0_65_40;

fn fallback_tint(skin: Skin) -> [f32; 3] {
    if let Some(c) = mob_tint(skin) {
        return c;
    }
    match skin {
        Skin::Steve | Skin::Alex => [0.75, 0.6, 0.5],
        Skin::Zombie => [0.35, 0.6, 0.3],
        Skin::Skeleton => [0.85, 0.85, 0.8],
        Skin::PigZombie => [0.8, 0.55, 0.5],
        Skin::Creeper => [0.3, 0.7, 0.25],
        Skin::Pig => [0.95, 0.65, 0.65],
        Skin::Cow => [0.4, 0.3, 0.25],
        Skin::Armor(_) | Skin::GuardianElder => [0.7; 3],
        Skin::Arrow
        | Skin::Snowball
        | Skin::Egg
        | Skin::EnderPearl
        | Skin::FireCharge
        | Skin::Bobber
        | Skin::SplashPotion => [0.8, 0.8, 0.85],
        _ => [1.0; 3],
    }
}

/// Flat color of a mob texture that has no vertex tint of its own.
fn mob_tint(skin: Skin) -> Option<[f32; 3]> {
    Some(match skin {
        Skin::Villager(_) => [0.7, 0.55, 0.45],
        Skin::IronGolem => [0.75, 0.75, 0.7],
        Skin::Silverfish => [0.5, 0.5, 0.52],
        Skin::Chicken => [0.95; 3],
        Skin::Sheep | Skin::SheepFur => [0.9; 3],
        Skin::Spider | Skin::CaveSpider => [0.25, 0.22, 0.2],
        Skin::Enderman => [0.1, 0.08, 0.12],
        Skin::SpiderEyes | Skin::EndermanEyes => [1.0; 3],
        _ => return None,
    })
}

fn skin_dims() -> [f32; 2] {
    [Skins::width() as f32, Skins::height() as f32]
}

/// `ModelBox`: six quads with the unfolded-cuboid texture layout of a `layout`-sized box at `uv`
/// (texels, plus `v_off`); `size` may be larger (inflate).
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_box(
    out: &mut MeshData,
    tf: &dyn Fn(V3) -> V3,
    min: V3,
    size: V3,
    layout: V3,
    (u0, v0): (f32, f32),
    mirror: bool,
    v_off: f32,
    color: [f32; 3],
    tex: [f32; 2],
) {
    let [x1, y1, z1] = min;
    let [x2, y2, z2] = [min[0] + size[0], min[1] + size[1], min[2] + size[2]];
    // `ModelBox` lays the texture out by the box size, not by the inflated geometry.
    let [w, h, d] = layout;
    let v0 = v0 + v_off;
    let uv = |u: f32, v: f32| [u / tex[0], v / tex[1]];
    // (corners in model space, texel corners, outward model-space normal, shade)
    let mut faces: [Face; 6] = [
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
    if mirror {
        let (plus, minus) = (faces[0].1, faces[1].1);
        (faces[0].1, faces[1].1) = (minus, plus);
        for f in &mut faces {
            f.1.swap(0, 1);
            f.1.swap(2, 3);
        }
    }
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
                light: [1.0, 1.0],
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

/// One name tag for the HUD: `screen` is the top-centre of the text in pixels, `scale` the pixels
/// per font pixel (vanilla's 0.02666667 blocks per font pixel, so tags shrink with distance), and
/// `anchor` the world point the tag hangs from. A `sneaking` tag is faint only: no solid pass.
pub struct NameTag {
    pub screen: [f32; 2],
    pub scale: f32,
    pub anchor: [f64; 3],
    pub sneaking: bool,
    pub text: String,
}

/// `canRenderName` of the entity's renderer, before the distance test: armor stands follow the
/// always-show byte alone, other mobs need it or the crosshair, players only need to be visible.
fn shows_name(d: &EntityDraw) -> bool {
    if let (Class::Player { .. }, Some(shown)) = (&d.class, d.team_tag.shown) {
        return shown;
    }
    match d.class {
        Class::Object(78) => d.always_name,
        Class::Mob(_) => !d.invisible && !d.ridden && (d.always_name || d.pointed),
        Class::Player { .. } => !d.invisible && !d.ridden,
        _ => d.always_name,
    }
}

/// Name tags in range, above the hitbox (`RendererLivingEntity.renderName`: 64 blocks, sneaking 32,
/// measured from the viewer's feet). `view_proj` maps world space to clip space; positions are in
/// `0..width` by `0..height` pixels.
pub fn nametags(
    draws: &[EntityDraw],
    viewer: [f64; 3],
    view_proj: &[[f32; 4]; 4],
    fov_deg: f32,
    width: f32,
    height: f32,
) -> Vec<NameTag> {
    let view = TagView {
        view_proj,
        px_per_block: height * 0.5 / (fov_deg.to_radians() * 0.5).tan(),
        size: [width, height],
    };
    draws
        .iter()
        .filter_map(|d| {
            let name = d.name.as_ref()?;
            if !shows_name(d) {
                return None;
            }
            let dist_sq: f64 = (0..3).map(|i| (d.pos[i] - viewer[i]).powi(2)).sum();
            let limit = if d.sneaking { 32.0 } else { 64.0 };
            (dist_sq < limit * limit).then(|| view.tags(d, name, dist_sq))
        })
        .flatten()
        .collect()
}

/// The projection that places name tags.
struct TagView<'a> {
    view_proj: &'a [[f32; 4]; 4],
    px_per_block: f32,
    size: [f32; 2],
}

impl TagView<'_> {
    /// The tags of one entity in range: its name, and for a standing player in front of a below-name
    /// objective the score line (`RenderPlayer.renderNameTag`: within 10 blocks, the test is on the squared
    /// distance) with the name moved up by one line.
    fn tags(&self, d: &EntityDraw, name: &str, dist_sq: f64) -> Vec<NameTag> {
        let mut y = d.pos[1] + f64::from(d.size.1) + 0.5;
        if d.child {
            y -= f64::from(d.size.1) / 2.0;
        }
        if d.sneaking {
            y -= 0.25;
        }
        let below =
            d.team_tag.below.as_ref().filter(|_| {
                matches!(d.class, Class::Player { .. }) && !d.sneaking && dist_sq < 100.0
            });
        let name_y = if below.is_some() {
            y + BELOW_NAME_LIFT
        } else {
            y
        };
        below
            .and_then(|b| self.tag(d, y, b))
            .into_iter()
            .chain(self.tag(d, name_y, name))
            .collect()
    }

    fn tag(&self, d: &EntityDraw, y: f64, text: &str) -> Option<NameTag> {
        let anchor = [d.pos[0], y, d.pos[2]];
        let p = anchor.map(|v| v as f32);
        let m = self.view_proj;
        let c: Vec<f32> = (0..4)
            .map(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
            .collect();
        (c[3] > 0.0).then(|| NameTag {
            screen: [
                (c[0] / c[3] * 0.5 + 0.5) * self.size[0],
                (0.5 - c[1] / c[3] * 0.5) * self.size[1],
            ],
            scale: 0.026_666_67 * self.px_per_block / c[3],
            anchor,
            sneaking: d.sneaking,
            text: text.into(),
        })
    }
}

/// `fontHeight * 1.15 * 0.02666667` blocks: how far `renderNameTag` lifts the name over the score line.
const BELOW_NAME_LIFT: f64 = 9.0 * 1.15 * 0.026_666_67;

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn steve() -> EntityDraw {
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
            armor_dye: [None; 4],
            armor_glint: [false; 4],
            held_glint: false,
            held_damage: 0,
            held_dye: None,
            stack: None,
            hover_start: 0.0,
            hang: Default::default(),
            name: None,
            team_tag: TeamTag::default(),
            skin: None,
            burning: false,
            eye_height: 1.62,
            always_name: false,
            pointed: false,
            ridden: false,
            child: false,
            using: None,
            view: [0.0; 2],
            line: None,
            look: Look::default(),
        }
    }

    #[test]
    fn inflating_a_box_keeps_its_texture_layout() {
        let uvs = |grow: f32| {
            let mut out = MeshData::default();
            let (min, size) = (
                [-4.0 - grow, -grow, -2.0 - grow],
                [8.0 + 2.0 * grow, 12.0 + 2.0 * grow, 4.0 + 2.0 * grow],
            );
            add_box(
                &mut out,
                &|p| p,
                min,
                size,
                [8.0, 12.0, 4.0],
                (16.0, 16.0),
                false,
                0.0,
                [1.0; 3],
                [64.0, 64.0],
            );
            out.vertices.iter().map(|v| v.uv).collect::<Vec<_>>()
        };
        assert_eq!(uvs(1.0), uvs(0.0));
        // The back face of the body unfolds at u 32..40, v 20..32.
        let back = uvs(1.0);
        assert!(back
            .iter()
            .any(|uv| uv[0] == 40.0 / 64.0 && uv[1] == 32.0 / 64.0));
        assert!(back
            .iter()
            .all(|uv| uv[0] >= 16.0 / 64.0 && uv[0] <= 40.0 / 64.0));
    }

    fn mesh(d: &EntityDraw) -> MeshData {
        build(std::slice::from_ref(d), &Skins::flat(), |_| [1.0, 1.0])
    }

    fn extent(m: &MeshData, axis: usize) -> f32 {
        let (lo, hi) = m.vertices.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(v.pos[axis]), hi.max(v.pos[axis]))
        });
        hi - lo
    }

    #[test]
    fn every_ported_mob_draws_at_its_vanilla_height() {
        // Heights in blocks from the 1.8.9 Model* boxes: (24 - top) / 16 times the render scale.
        let mobs = [
            (120u8, 34.0 / 16.0 * 0.9375),
            (60, 0.5),
            (93, 0.9375),
            (91, 1.41),
        ];
        let mut bad = Vec::new();
        for (id, expected) in mobs {
            let mut d = steve();
            d.class = Class::Mob(id);
            let m = mesh(&d);
            let h = extent(&m, 1);
            if m.vertices.is_empty() || (h - expected).abs() > 0.05 {
                bad.push((id, h, expected));
            }
        }
        for id in [99u8, 52, 59, 58] {
            let mut d = steve();
            d.class = Class::Mob(id);
            if mesh(&d).vertices.is_empty() {
                bad.push((id, 0.0, 0.0));
            }
        }
        assert!(bad.is_empty(), "(id, got, expected): {bad:?}");
    }

    #[test]
    fn cave_spider_is_seven_tenths_of_a_spider() {
        let mut d = steve();
        d.class = Class::Mob(mobs::SPIDER);
        let spider = extent(&mesh(&d), 0);
        d.class = Class::Mob(mobs::CAVE_SPIDER);
        let cave = extent(&mesh(&d), 0);
        assert!((cave / spider - 0.7).abs() < 0.01, "{cave} / {spider}");
    }

    #[test]
    fn player_skin_layers_and_cape_add_geometry() {
        let img = |w, h| crate::assets::Image {
            width: w,
            height: h,
            rgba: vec![255; w * h * 4],
        };
        let mut skins = Skins::flat();
        skins.set_player(1, &img(64, 64));
        let mut d = steve();
        d.skin = Some(1);
        let count = |skins: &Skins, d: &EntityDraw| {
            build(std::slice::from_ref(d), skins, |_| [1.0, 1.0])
                .vertices
                .len()
        };
        d.look.skin_parts = 0x00;
        let bare = count(&skins, &d);
        d.look.skin_parts = 0x7F;
        // Hat, jacket, two sleeves and two pants legs: six extra boxes of 24 vertices.
        let layered = count(&skins, &d);
        assert_eq!(layered, bare + 6 * 24);
        d.look.cape = Some(CapeSway::default());
        assert_eq!(count(&skins, &d), layered);
        skins.set_cape(1, &img(64, 32));
        assert_eq!(count(&skins, &d), layered + 24);
    }

    #[test]
    fn invisible_player_keeps_armor_but_not_the_body() {
        let count = |d: &EntityDraw| {
            build(std::slice::from_ref(d), &Skins::flat(), |_| [1.0, 1.0])
                .vertices
                .len()
        };
        let mut d = steve();
        d.invisible = true;
        assert_eq!(count(&d), 0);
        // The held item is a model in `item`, not part of this mesh.
        d.held = 276;
        assert_eq!(count(&d), 0);
        d.armor = [310; 4];
        let worn = count(&d);
        assert!(worn > 0);
        d.invisible = false;
        assert!(count(&d) > worn);
    }

    #[test]
    fn skeleton_left_limbs_sample_the_mirrored_right_limb_regions() {
        // Rows 48..64 of the 64x64 layout hold the player's left arm (u 32..48) and leg (u 16..32).
        let in_left_limb_rows = |d: EntityDraw| {
            build(&[d], &Skins::flat(), |_| [1.0, 1.0])
                .vertices
                .iter()
                .filter(|p| {
                    let (u, v) = (
                        p.uv[0] * 64.0,
                        p.uv[1] * Skins::height() as f32 - Skins::flat().v_offset(Skin::White),
                    );
                    (48.0..=64.0).contains(&v) && (16.0..=48.0).contains(&u)
                })
                .count()
        };
        let mut skeleton = steve();
        skeleton.class = Class::Mob(51);
        assert!(in_left_limb_rows(steve()) > 0);
        assert_eq!(in_left_limb_rows(skeleton), 0);
    }

    #[test]
    fn bow_and_block_poses_match_vanilla_model_biped_angles() {
        // Vanilla 1.8.9 ModelBiped.setRotationAngles, age 0: sway z = 0.1, sway x = 0.
        let mut pose = Pose {
            r_arm: [0.0; 3],
            l_arm: [0.0; 3],
            body_rot: [0.0; 3],
            r_arm_pivot: [0.0; 3],
            l_arm_pivot: [0.0; 3],
            head_pivot: [0.0; 3],
            leg_pivots: [[0.0; 3]; 2],
        };
        pose.aim_bow(0.25, 0.5, 0.0);
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        let arm_x = -FRAC_PI_2 + 0.25;
        assert!(close(pose.r_arm, [arm_x, 0.4, 0.1]), "{:?}", pose.r_arm);
        assert!(close(pose.l_arm, [arm_x, 1.0, -0.1]), "{:?}", pose.l_arm);

        // itemInRightHand 1: x * 0.5 - 0.314159265; 3 (blocking): x * 0.5 - 3 * 0.314159265, yaw -PI/6.
        let mut held = [0.8, 0.2, 0.0];
        hold_item(&mut held, false);
        assert!(close(held, [0.4 - 0.314_159_27, 0.2, 0.0]));
        let mut blocking = [0.8, 0.2, 0.0];
        hold_item(&mut blocking, true);
        assert!(close(
            blocking,
            [0.4 - 3.0 * 0.314_159_27, -std::f32::consts::PI / 6.0, 0.0]
        ));
    }

    #[test]
    fn build_applies_bow_and_block_but_not_consume() {
        let verts = |held, using| {
            let d = EntityDraw {
                held,
                using,
                ..steve()
            };
            build(&[d], &Skins::flat(), |_| [1.0, 1.0]).vertices
        };
        let idle = verts(261, None);
        assert_ne!(idle, verts(261, Some(UseAction::Bow)));
        assert_ne!(verts(267, None), verts(267, Some(UseAction::Block)));
        assert_eq!(idle, verts(261, Some(UseAction::Consume)));
    }

    #[test]
    fn worn_armor_adds_one_box_per_covered_part() {
        let count = |armor: [i16; 4]| {
            let mut d = steve();
            d.armor = armor;
            build(&[d], &Skins::flat(), |_| [1.0, 1.0]).vertices.len()
        };
        let bare = count([0; 4]);
        // boots 2 boxes, leggings 3, chestplate 3, helmet 1; a box has 24 vertices.
        assert_eq!(count([309, 0, 0, 0]) - bare, 2 * 24);
        assert_eq!(count([0, 308, 0, 0]) - bare, 3 * 24);
        assert_eq!(count([0, 0, 307, 0]) - bare, 3 * 24);
        assert_eq!(count([0, 0, 0, 306]) - bare, 24);
        assert_eq!(count([309, 308, 307, 306]) - bare, 9 * 24);
    }

    #[test]
    fn armor_items_map_to_their_layer_textures() {
        assert_eq!(Skin::armor(298, 1, false), Some(Skin::Armor(0)));
        assert_eq!(Skin::armor(300, 2, false), Some(Skin::Armor(1)));
        assert_eq!(Skin::armor(310, 1, false), Some(Skin::Armor(6)));
        assert_eq!(Skin::armor(317, 2, false), Some(Skin::Armor(9)));
        assert_eq!(Skin::armor(299, 2, true), Some(Skin::Armor(11)));
        assert_eq!(Skin::armor(306, 1, true), None);
        assert_eq!(Skin::armor(276, 1, false), None);
    }

    #[test]
    fn glint_layer_covers_only_enchanted_pieces() {
        let mut d = steve();
        d.armor = [313, 0, 0, 0];
        d.held = 276;
        assert!(build_glint(&[d.clone()]).is_empty());
        d.armor_glint = [true, false, false, false];
        let boots = build_glint(&[d.clone()]).vertices.len();
        assert_eq!(boots, 2 * 2 * 24, "two boot boxes, two passes");
        // The held item's glint is drawn with its model in `item`, not here.
        d.held_glint = true;
        assert_eq!(build_glint(&[d]).vertices.len(), boots);
    }

    #[test]
    fn glint_uv_follows_vanilla_matrices() {
        // Armor pass 0: scale 1/3 after a 30 degree turn; pass 1 turns -30 degrees.
        let [x, y] = glint_uv_armor([1.0, 0.0], 0.0, 0);
        assert!((x - 30f32.to_radians().cos() / 3.0).abs() < 1e-6);
        assert!((y - 30f32.to_radians().sin() / 3.0).abs() < 1e-6);
        let [_, y] = glint_uv_armor([1.0, 0.0], 0.0, 1);
        assert!(y < 0.0);
        // Item pass 0 at ms 0: scale 8 and a -50 degree turn; the scroll adds 1 after 3000 ms.
        let [x, y] = glint_uv_item([1.0, 0.0], 0.0, 0);
        assert!((x - 8.0 * 50f32.to_radians().cos()).abs() < 1e-5);
        assert!((y + 8.0 * 50f32.to_radians().sin()).abs() < 1e-5);
        let near = glint_uv_item([0.0, 0.0], 2999.0, 0);
        assert!((near[0] - 8.0 * 2999.0 / 3000.0 / 8.0).abs() < 1e-5);
    }

    #[test]
    fn standing_player_spans_the_hitbox_height() {
        let m = build(&[steve()], &Skins::flat(), |_| [1.0, 1.0]);
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
            let m = build(&[d], &Skins::flat(), |_| [1.0, 1.0]);
            let v = |y: f32| (Skins::flat().v_offset(Skin::White) + y) / Skins::height() as f32;
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
        d.class = Class::Object(100);
        d.size = (1.0, 1.0);
        let m = build(&[d], &Skins::flat(), |_| [1.0, 1.0]);
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
        let walking = build(&[d], &Skins::flat(), |_| [1.0, 1.0]);
        let idle = build(&[steve()], &Skins::flat(), |_| [1.0, 1.0]);
        assert_ne!(walking.vertices.len(), 0);
        assert_ne!(walking.vertices[100].pos, idle.vertices[100].pos);
    }

    #[test]
    fn attack_swing_follows_model_biped() {
        // `ModelBiped.setRotationAngles` at progress 0.5 and level head: sin((1 - 0.5^4) pi) * 1.2 + sin(pi / 2) * 0.7 * 0.75.
        let mut pose = Pose {
            r_arm: [0.0; 3],
            l_arm: [0.0; 3],
            body_rot: [0.0; 3],
            r_arm_pivot: [0.0; 3],
            l_arm_pivot: [0.0; 3],
            head_pivot: [0.0; 3],
            leg_pivots: [[0.0; 3]; 2],
        };
        pose.swing(0.5, 0.0);
        let expected = -((0.9375f32 * PI).sin() * 1.2 + 0.7 * 0.75);
        assert!((pose.r_arm[0] - expected).abs() < 1e-5, "{}", pose.r_arm[0]);
    }

    #[test]
    fn skeleton_arms_point_forward_like_zombie_arms() {
        let mut d = steve();
        d.class = Class::Mob(51);
        let arm = &model(&d).parts[3];
        assert!((arm.rot[0] + FRAC_PI_2).abs() < 0.1, "{}", arm.rot[0]);
    }

    #[test]
    fn dead_mob_rotates_about_its_feet() {
        let mut d = steve();
        d.class = Class::Mob(54);
        d.death_time = 20.0;
        let m = build(&[d], &Skins::flat(), |_| [1.0, 1.0]);
        let top = m.vertices.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
        // Lying on its side the highest point is the half-width of the model, not the 1.8 block height.
        assert!(top < 64.0 + 0.6, "{top}");
    }

    #[test]
    fn light_comes_from_the_eye_cell_and_fire_stays_full_bright() {
        let seen = std::cell::Cell::new([0.0; 3]);
        let probe = |p: [f64; 3]| {
            seen.set(p);
            [0.2, 0.0]
        };
        let mut d = steve();
        d.eye_height = 1.62;
        let m = build(&[d.clone()], &Skins::flat(), probe);
        assert!(m.vertices.iter().all(|v| v.light == [0.2, 0.0]));
        assert!((seen.get()[1] - 65.62).abs() < 1e-3);
        d.burning = true;
        let m = build(&[d], &Skins::flat(), |_| [0.2, 0.0]);
        assert!(m.vertices.iter().all(|v| v.light == [1.0, 1.0]));
    }

    #[test]
    fn name_tag_rules_follow_the_renderer_class() {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        // Looking down -z from the origin: clip w = -z, so put entities at z < 0.
        let tags = |mut d: EntityDraw| {
            d.pos = [0.0, 0.0, -10.0];
            d.name = Some("N".into());
            nametags(&[d], [0.0; 3], &flip(id), 70.0, 800.0, 600.0).len()
        };
        let mut stand = steve();
        stand.class = Class::Object(78);
        stand.invisible = true;
        assert_eq!(tags(stand.clone()), 0, "stand without the byte");
        stand.always_name = true;
        assert_eq!(tags(stand), 1, "invisible stand with the byte");
        let mut mob = steve();
        mob.class = Class::Mob(54);
        assert_eq!(tags(mob.clone()), 0, "named mob out of the crosshair");
        mob.pointed = true;
        assert_eq!(tags(mob.clone()), 1);
        mob.pointed = false;
        mob.always_name = true;
        assert_eq!(tags(mob), 1);
        let mut player = steve();
        assert_eq!(tags(player.clone()), 1);
        player.invisible = true;
        assert_eq!(tags(player), 0, "invisible player");
        let mut far = steve();
        far.pos = [0.0, 0.0, -64.0];
        far.name = Some("N".into());
        assert!(nametags(&[far], [0.0; 3], &flip(id), 70.0, 800.0, 600.0).is_empty());
    }

    #[test]
    fn sneaking_ridden_and_child_tags_follow_render_name() {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let tag = |f: &dyn Fn(&mut EntityDraw)| {
            let mut d = steve();
            d.pos = [0.0, 0.0, -10.0];
            d.name = Some("N".into());
            f(&mut d);
            nametags(&[d], [0.0; 3], &flip(id), 70.0, 800.0, 600.0)
        };
        assert!(!tag(&|_| {})[0].sneaking);
        let sneak = &tag(&|d| d.sneaking = true)[0];
        assert!(sneak.sneaking);
        assert!((sneak.anchor[1] - (steve().size.1 as f64 + 0.5 - 0.25)).abs() < 1e-6);
        let child = &tag(&|d| {
            d.child = true;
            d.size = (0.3, 0.9);
        })[0];
        assert!((child.anchor[1] - (0.9 + 0.5 - 0.45)).abs() < 1e-6);
        assert!(tag(&|d| d.ridden = true).is_empty());
    }

    #[test]
    fn team_decides_the_tag_and_below_name_lifts_it() {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let tags = |z: f64, f: &dyn Fn(&mut EntityDraw)| {
            let mut d = steve();
            d.pos = [0.0, 0.0, z];
            d.name = Some("N".into());
            d.team_tag.below = Some("17 HP".into());
            f(&mut d);
            nametags(&[d], [0.0; 3], &flip(id), 70.0, 800.0, 600.0)
        };
        let text = |t: &[NameTag]| t.iter().map(|t| t.text.clone()).collect::<Vec<_>>();
        let near = tags(-9.9, &|_| {});
        assert_eq!(text(&near).len(), 2);
        let (below, name) = (
            near.iter().find(|t| t.text == "17 HP").unwrap(),
            near.iter().find(|t| t.text == "N").unwrap(),
        );
        assert!((name.anchor[1] - below.anchor[1] - 0.276).abs() < 1e-6);
        assert_eq!(
            text(&tags(-10.0, &|_| {})),
            ["N"],
            "10 blocks: squared distance is not under 100"
        );
        assert_eq!(text(&tags(-5.0, &|d| d.sneaking = true)), ["N"]);
        // A team with a visible tag shows an invisible player; `never` hides a visible one.
        let shown = |s: Option<bool>| move |d: &mut EntityDraw| d.team_tag.shown = s;
        assert_eq!(
            tags(-5.0, &|d| {
                d.invisible = true;
                shown(Some(true))(d)
            })
            .len(),
            2
        );
        assert!(tags(-5.0, &shown(Some(false))).is_empty());
    }

    /// `w = -z` so a point at negative z is in front, like the real projection.
    fn flip(mut m: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
        m[2][3] = -1.0;
        m[3][3] = 0.0;
        m
    }
}
