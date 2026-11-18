//! Remote entity tracking of the 1.8.9 client: spawn, move, look, metadata and destroy packets
//! become an [`Entities`] store with vanilla's 3-tick position interpolation and limb swing.
//! Also the left-click target pick (`EntityRenderer.getMouseOver`, entity part).
//!
//! The network layer converts packets to [`EntityEvent`]; this module has no protocol dependency.

use std::collections::HashMap;

/// What an entity is. Ids are the 1.8.9 mob and object type ids of the spawn packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Player,
    /// `SpawnMob` type id (50 creeper .. 120 villager).
    Mob(u8),
    /// `SpawnObject` type id (2 item, 60 arrow, ...).
    Object(i8),
    /// `EntityPainting`, from `SpawnPainting`.
    Painting,
    ExperienceOrb,
}

/// Spawn object types with `preventEntitySpawning`: boat, minecarts, primed TNT, end crystal,
/// falling block and armor stand.
const BLOCKING_OBJECTS: [i8; 8] = [1, 10, 11, 12, 50, 51, 70, 78];

impl Kind {
    pub const ITEM: Kind = Kind::Object(2);

    /// Hitbox `(width, height)` as in the vanilla entity constructors.
    pub fn size(self) -> (f32, f32) {
        match self {
            Kind::Player => (0.6, 1.8),
            Kind::ExperienceOrb | Kind::Painting => (0.5, 0.5),
            Kind::Object(2) => (0.25, 0.25),
            Kind::Object(60 | 61 | 62 | 65 | 73 | 75 | 90) => (0.25, 0.25),
            Kind::Object(78) => (0.5, 1.975),
            Kind::Object(1) => (1.5, 0.6),
            Kind::Object(51) => (2.0, 2.0),
            Kind::Object(10..=12) => (0.98, 0.7),
            Kind::Object(_) => (0.98, 0.98),
            Kind::Mob(id) => mob_size(id),
        }
    }

    /// `Entity.getMountedYOffset`: where a rider sits above the entity's feet. Boats and minecarts seat it
    /// 0.3 below their feet line; the rest use three quarters of their height.
    pub fn mounted_y_offset(self) -> f64 {
        match self {
            Kind::Object(1 | 10..=12) => f64::from(-0.3f32),
            _ => f64::from(self.size().1) * 0.75,
        }
    }
}

/// Hitbox `(width, height)` per `SpawnMob` type id; other ids are 0.6 x 1.8.
const MOB_SIZES: [(u8, f32, f32); 30] = [
    (50, 0.6, 1.7),
    (52, 1.4, 0.9),
    (53, 3.6, 10.8),
    (55, 0.51, 0.51),
    (62, 0.51, 0.51),
    (56, 4.0, 4.0),
    (58, 0.6, 2.9),
    (59, 0.7, 0.5),
    (60, 0.4, 0.3),
    (67, 0.4, 0.3),
    (63, 16.0, 8.0),
    (64, 0.9, 3.5),
    (65, 0.5, 0.9),
    (68, 0.85, 0.85),
    (90, 0.9, 0.9),
    (91, 0.9, 1.3),
    (92, 0.9, 1.3),
    (96, 0.9, 1.3),
    (93, 0.4, 0.7),
    (94, 0.95, 0.95),
    (95, 0.6, 0.8),
    (97, 0.7, 1.9),
    (98, 0.6, 0.7),
    (101, 0.6, 0.7),
    (99, 1.4, 2.9),
    (100, 1.4, 1.6),
    (51, 0.6, 1.95),
    (54, 0.6, 1.95),
    (57, 0.6, 1.95),
    (66, 0.6, 1.95),
];

fn mob_size(id: u8) -> (f32, f32) {
    MOB_SIZES
        .iter()
        .find(|m| m.0 == id)
        .map_or((0.6, 1.8), |m| (m.1, m.2))
}

/// Metadata index 0 bit flags (`Entity.getFlag`).
pub mod flag {
    pub const BURNING: u8 = 1;
    pub const SNEAKING: u8 = 1 << 1;
    pub const SPRINTING: u8 = 1 << 3;
    pub const USING_ITEM: u8 = 1 << 4;
    pub const INVISIBLE: u8 = 1 << 5;
}

/// `EntityArmorStand` status byte (metadata index 10).
pub mod stand {
    pub const SMALL: u8 = 1;
    pub const ARMS: u8 = 4;
    pub const NO_BASE_PLATE: u8 = 8;
    pub const MARKER: u8 = 16;
}

/// Baby zombie or ageable mob: age metadata is a child flag for zombies, negative for animals.
pub fn is_baby(kind: Kind, m: &SizeMeta) -> bool {
    match kind {
        Kind::Mob(54 | 57) => m.age.is_some_and(|a| a != 0),
        Kind::Mob(90..=93 | 95 | 96 | 98 | 100 | 101 | 120) => m.age.is_some_and(|a| (a as i8) < 0),
        _ => false,
    }
}

/// The metadata that sets a hitbox or a sound variant: armor stand status (index 10), age of
/// ageable mobs (12, a zombie's child flag), slime size (16), guardian flags (16) and horse
/// type (19), plus the name-tag byte (3, `getAlwaysRenderNameTag`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SizeMeta {
    pub stand: Option<u8>,
    /// `EntityArmorStand` poses in degrees (metadata 11 to 16): head, body, left arm, right arm,
    /// left leg, right leg. `None` keeps the vanilla default of that part.
    pub pose: [Option<[f32; 3]>; 6],
    pub age: Option<u8>,
    pub slime: Option<u8>,
    pub guardian: Option<i32>,
    pub horse: Option<u8>,
    /// Horse flags (index 16, an int): tame, saddled, chested.
    pub horse_flags: Option<i32>,
    /// Horse variant (index 20, an int): the coat in the low byte, the markings in the next.
    pub horse_variant: Option<i32>,
    /// Horse armor tier (index 22, an int): 0 none, 1 iron, 2 gold, 3 diamond.
    pub horse_armor: Option<i32>,
    pub always_name: Option<u8>,
    pub skeleton: Option<u8>,
    /// Arrows: metadata index 16, bit 1 is `getIsCritical`.
    pub arrow: Option<u8>,
    /// Item frames: metadata index 9, the rotation of the displayed item in eighths of a turn.
    pub rotation: Option<u8>,
    /// Living entities: metadata index 9, the arrows stuck in the body (`getArrowCountInEntity`).
    pub arrows: Option<u8>,
    /// Wolf: metadata byte 20, the collar dye (`getCollarColor`, a dye damage value).
    pub collar: Option<u8>,
    /// Wolf: metadata float 18, the health the tail angle reads (`getTailRotation`).
    pub health: Option<f32>,
    /// Ocelot: metadata byte 18, the cat type (`getTameSkin`).
    pub cat: Option<u8>,
    /// Boats and minecarts; see [`crate::vehicle::VehicleMeta`].
    pub vehicle: crate::vehicle::VehicleMeta,
    /// Wolf: metadata byte 19, 1 while begging (`isBegging`).
    pub begging: Option<u8>,
    /// Wither: metadata int 20, `getInvulTime`, the ticks left of the spawn charge.
    pub wither_invul: Option<i32>,
}

impl SizeMeta {
    fn merge(&mut self, new: SizeMeta) {
        self.stand = new.stand.or(self.stand);
        for (old, new) in self.pose.iter_mut().zip(new.pose) {
            *old = new.or(*old);
        }
        self.age = new.age.or(self.age);
        self.slime = new.slime.or(self.slime);
        self.guardian = new.guardian.or(self.guardian);
        self.horse = new.horse.or(self.horse);
        self.horse_flags = new.horse_flags.or(self.horse_flags);
        self.horse_variant = new.horse_variant.or(self.horse_variant);
        self.horse_armor = new.horse_armor.or(self.horse_armor);
        self.always_name = new.always_name.or(self.always_name);
        self.skeleton = new.skeleton.or(self.skeleton);
        self.arrow = new.arrow.or(self.arrow);
        self.rotation = new.rotation.or(self.rotation);
        self.arrows = new.arrows.or(self.arrows);
        self.collar = new.collar.or(self.collar);
        self.health = new.health.or(self.health);
        self.cat = new.cat.or(self.cat);
        self.vehicle.merge(new.vehicle);
        self.begging = new.begging.or(self.begging);
        self.wither_invul = new.wither_invul.or(self.wither_invul);
    }
}

/// Equipment slots of `S04`: 0 held item, 1..=4 boots to helmet.
pub const EQUIPMENT_SLOTS: usize = 5;

/// A held or worn item: item id and damage, as the network sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRef {
    pub id: i16,
    pub damage: i16,
    /// Stack size; a dropped item draws one to five copies of it.
    pub count: u8,
    /// Leather armor color (`display.color`).
    pub dye: Option<i32>,
    /// `ItemStack.hasEffect`: the enchantment glint.
    pub glint: bool,
    /// `SkullOwner.Id` of a skull item: the profile whose skin the head shows.
    pub owner: Option<u128>,
}

/// One `Fireworks.Explosions` entry of a rocket (`Type`, `Trail`, `Flicker`, `Colors`, `FadeColors`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FireworkExplosion {
    /// 0 small ball, 1 large ball, 2 star, 3 creeper, 4 burst.
    pub shape: i8,
    pub trail: bool,
    pub flicker: bool,
    pub colors: Vec<i32>,
    pub fade: Vec<i32>,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: i32,
    pub kind: Kind,
    pub uuid: Option<u128>,
    /// Display name for players (from the player list) or a custom name tag.
    pub name: Option<String>,
    /// Players: the `textures` profile property from the player list (base64 JSON).
    pub textures: Option<String>,
    /// Living entities: metadata index 6 (`getHealth`), as last sent.
    pub health: Option<f32>,
    pub pos: [f64; 3],
    pub prev_pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// Look at the previous tick (`prevRotationYaw`, `prevRotationPitch`).
    pub prev_yaw: f32,
    pub prev_pitch: f32,
    /// `rotationYawHead`; the server sets it at once, never eased.
    pub head_yaw: f32,
    /// `renderYawOffset`: the direction the body model faces, apart from the look yaw.
    pub body_yaw: f32,
    /// `prevRenderYawOffset` and `prevRotationYawHead`: the values at the previous tick.
    pub last_body_yaw: f32,
    pub last_head_yaw: f32,
    body_control: crate::body::BodyControl,
    pub on_ground: bool,
    pub vel: [f64; 3],
    pub flags: u8,
    /// Ticks since the `USING_ITEM` flag was first seen (`None` while it is off).
    pub use_ticks: Option<u32>,
    /// Fixed-point (1/32 block) position the server last sent; relative moves add to it.
    server_pos: [i32; 3],
    target: Target,
    pub limb_swing: f32,
    pub limb_swing_amount: f32,
    pub prev_limb_swing_amount: f32,
    /// 0 when idle; counts up to `effects.arm_swing_duration()` (6 without Haste or Mining Fatigue) during an arm swing.
    pub swing_ticks: i32,
    pub swinging: bool,
    /// Potion effects from `S1D`/`S1E`; `getArmSwingAnimationEnd` reads Haste and Mining Fatigue.
    pub effects: crate::effects::Effects,
    /// Red-flash ticks left after a hurt status (vanilla `hurtTime`, 10 at the hit).
    pub hurt_time: i32,
    /// Ticks since death status; the model tips over for 20 ticks, then the entity is removed.
    pub death_time: i32,
    /// `Entity.removed` of an arrow that a hit ended on the client; the entity leaves at the end of the tick.
    removed: bool,
    /// Sheep: ticks left of the grass-eating animation (`sheepTimer`, 40 at status 10).
    pub sheep_timer: i32,
    /// Iron golem arm and wolf shake timers.
    pub anim: crate::mob_anim::MobAnim,
    /// Metadata that sets the hitbox.
    pub meta: SizeMeta,
    pub equipment: [Option<ItemRef>; EQUIPMENT_SLOTS],
    /// Item entities: the stack they show.
    pub stack: Option<ItemRef>,
    /// Entity id of the vehicle, if riding.
    pub vehicle: Option<i32>,
    /// Mobs: entity id of the holder of the leash (`getLeashedToEntity`).
    pub leash: Option<i32>,
    pub age: u32,
    /// Metadata index 7: the mixed colour of the active potion effects, 0 for none.
    pub potion_color: i32,
    /// Arrows: id of the shooter; fishing bobbers: id of the player who cast it (the `SpawnObject` data). 0 for any
    /// other entity.
    pub owner: i32,
    /// Metadata index 8: every active effect is ambient (beacon).
    pub potion_ambient: bool,
    /// Paintings and item frames: horizontal facing, 0 south, 1 west, 2 north, 3 east.
    pub facing: u8,
    /// Paintings: the motive name (`EntityPainting.EnumArt.title`).
    pub title: String,
    /// Firework rockets: the explosions of the item at metadata index 8.
    pub explosions: Vec<FireworkExplosion>,
    /// Slimes and magma cubes: the jump squish.
    pub squish: crate::squish::Squish,
    /// Squids: the tentacle, roll and body turn animation.
    pub squid: crate::squid::Squid,
    /// Ender dragons: the wing clock and the ring buffer of past yaws and heights.
    pub dragon: Option<Box<crate::mob_anim::Dragon>>,
    /// Arrows, thrown objects, items, TNT, falling blocks and bobbers: the client-side flight state.
    pub(crate) proj: crate::projectile::State,
}

/// Painting motives: name, width and height in pixels, and the origin in the paintings texture.
pub const PAINTINGS: [(&str, u16, u16, u16, u16); 26] = [
    ("Kebab", 16, 16, 0, 0),
    ("Aztec", 16, 16, 16, 0),
    ("Alban", 16, 16, 32, 0),
    ("Aztec2", 16, 16, 48, 0),
    ("Bomb", 16, 16, 64, 0),
    ("Plant", 16, 16, 80, 0),
    ("Wasteland", 16, 16, 96, 0),
    ("Pool", 32, 16, 0, 32),
    ("Courbet", 32, 16, 32, 32),
    ("Sea", 32, 16, 64, 32),
    ("Sunset", 32, 16, 96, 32),
    ("Creebet", 32, 16, 128, 32),
    ("Wanderer", 16, 32, 0, 64),
    ("Graham", 16, 32, 16, 64),
    ("Match", 32, 32, 0, 128),
    ("Bust", 32, 32, 32, 128),
    ("Stage", 32, 32, 64, 128),
    ("Void", 32, 32, 96, 128),
    ("SkullAndRoses", 32, 32, 128, 128),
    ("Wither", 32, 32, 160, 128),
    ("Fighters", 64, 32, 0, 96),
    ("Pointer", 64, 64, 0, 192),
    ("Pigscene", 64, 64, 64, 192),
    ("BurningSkull", 64, 64, 128, 192),
    ("Skeleton", 64, 48, 192, 64),
    ("DonkeyKong", 64, 48, 192, 112),
];

/// `EntityHanging.updateBoundingBox`: the fixed-point centre of a hanging entity at block `pos`
/// facing `facing`, `width` by `height` pixels.
fn hanging_raw(pos: [i32; 3], facing: u8, width: u16, height: u16) -> [i32; 3] {
    let (ox, oz) = match facing & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    };
    // `rotateYCCW`: south to east, east to north, north to west, west to south.
    let (lx, lz) = (oz, -ox);
    let half = |p: u16| if p.is_multiple_of(32) { 16 } else { 0 };
    [
        pos[0] * 32 + 16 - ox * 15 + half(width) * lx,
        pos[1] * 32 + 16 + half(height),
        pos[2] * 32 + 16 - oz * 15 + half(width) * lz,
    ]
}

/// `EntityHanging.updateBoundingBox`: the slab around the centre `pos`, `width` by `height` pixels, one pixel
/// (a 32nd of a block, each side) thick on the axis of `facing`.
fn hanging_bounds(pos: [f64; 3], facing: u8, width: u16, height: u16) -> ([f64; 3], [f64; 3]) {
    let (w, h) = (f64::from(width) / 32.0, f64::from(height) / 32.0);
    let thick = 1.0 / 32.0;
    // Facing 0 and 2 look along z, so the slab is wide in x.
    let (hx, hz) = if facing & 1 == 0 {
        (w, thick)
    } else {
        (thick, w)
    };
    (
        [pos[0] - hx, pos[1] - h, pos[2] - hz],
        [pos[0] + hx, pos[1] + h, pos[2] + hz],
    )
}

/// `EntityLeashKnot.updateBoundingBox`: 0.375 wide and 0.5 high, centred on the block of the knot, 0.125 up.
fn leash_knot_bounds(pos: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let c = pos.map(|p| p.floor() + 0.5);
    (
        [c[0] - 0.1875, c[1] - 0.25 + 0.125, c[2] - 0.1875],
        [c[0] + 0.1875, c[1] + 0.25 + 0.125, c[2] + 0.1875],
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct Target {
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    steps: u32,
}

/// Interpolation steps vanilla uses for remote entities (`newPosRotationIncrements`).
const STEPS: u32 = 3;
const DEATH_TICKS: i32 = 20;
/// Mob id of the squid.
const SQUID: u8 = 94;
/// Mob id of the ender dragon, whose body follows its own ring buffer.
const DRAGON: u8 = 63;

fn angle(raw: i8) -> f32 {
    f32::from(raw) * 360.0 / 256.0
}

fn fixed(raw: i32) -> f64 {
    f64::from(raw) / 32.0
}

impl Entity {
    fn new(id: i32, kind: Kind, raw: [i32; 3], yaw: i8, pitch: i8) -> Entity {
        let pos = raw.map(fixed);
        let (yaw, pitch) = (angle(yaw), angle(pitch));
        Entity {
            id,
            kind,
            uuid: None,
            name: None,
            textures: None,
            health: None,
            pos,
            prev_pos: pos,
            yaw,
            pitch,
            prev_yaw: yaw,
            prev_pitch: pitch,
            head_yaw: yaw,
            body_yaw: yaw,
            last_body_yaw: yaw,
            last_head_yaw: yaw,
            body_control: crate::body::BodyControl::default(),
            on_ground: false,
            vel: [0.0; 3],
            flags: 0,
            use_ticks: None,
            server_pos: raw,
            target: Target {
                pos,
                yaw,
                pitch,
                steps: 0,
            },
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            prev_limb_swing_amount: 0.0,
            swing_ticks: 0,
            swinging: false,
            effects: Default::default(),
            hurt_time: 0,
            death_time: 0,
            removed: false,
            sheep_timer: 0,
            anim: crate::mob_anim::MobAnim {
                guardian: crate::mob_anim::Guardian::new(i64::from(id)),
                ..Default::default()
            },
            meta: SizeMeta::default(),
            equipment: [None; EQUIPMENT_SLOTS],
            stack: None,
            vehicle: None,
            leash: None,
            age: 0,
            potion_color: 0,
            potion_ambient: false,
            explosions: Vec::new(),
            facing: 0,
            title: String::new(),
            owner: 0,
            squish: crate::squish::Squish::default(),
            squid: crate::squid::Squid::new(id, yaw),
            dragon: (kind == Kind::Mob(63)).then(Box::default),
            proj: Default::default(),
        }
    }

    /// `EntityArrow.getIsCritical`.
    pub fn critical_arrow(&self) -> bool {
        self.kind == Kind::Object(60) && self.meta.arrow.unwrap_or(0) & 1 != 0
    }

    pub fn is_dead(&self) -> bool {
        self.death_time > 0
    }

    /// `Entity.preventEntitySpawning`: living entities (until `setDead`, after the death
    /// animation) and the objects of `BLOCKING_OBJECTS` refuse a block placed into their box.
    pub fn blocks_placement(&self) -> bool {
        match self.kind {
            Kind::Player | Kind::Mob(_) => true,
            Kind::Object(t) => BLOCKING_OBJECTS.contains(&t),
            Kind::ExperienceOrb | Kind::Painting => false,
        }
    }

    /// `Entity.getCollisionShape` (jar offset 0 of `BoatEntity.getCollisionShape`): only a boat returns its box
    /// (`[x0, y0, z0, x1, y1, z1]`), which `World.getCollisions` adds for a mover. `MinecartEntity.getCollisionShape`
    /// is null; its `getCollisionAgainstShape` only counts when the minecart is the mover.
    pub fn collision_box(&self) -> Option<[f64; 6]> {
        matches!(self.kind, Kind::Object(1)).then(|| {
            let (min, max) = self.bounds();
            [min[0], min[1], min[2], max[0], max[1], max[2]]
        })
    }

    pub fn sneaking(&self) -> bool {
        self.flags & flag::SNEAKING != 0
    }

    pub fn sprinting(&self) -> bool {
        self.flags & flag::SPRINTING != 0
    }

    pub fn invisible(&self) -> bool {
        self.flags & flag::INVISIBLE != 0
    }

    fn immune_to_fire(&self) -> bool {
        match self.kind {
            Kind::Mob(51) => self.meta.skeleton == Some(1),
            Kind::Mob(56 | 57 | 62 | 63 | 64) | Kind::Object(50) => true,
            _ => false,
        }
    }

    pub fn burning(&self) -> bool {
        if self.kind == Kind::Mob(61) {
            // Blaze metadata index 16 carries its fire-active bit.
            return self.meta.slime.is_some_and(|flags| flags & 1 != 0);
        }
        !self.immune_to_fire() && self.flags & flag::BURNING != 0
    }

    /// `Entity.getAlwaysRenderNameTag`: metadata byte 3 is 1.
    pub fn always_name(&self) -> bool {
        self.meta.always_name == Some(1)
    }

    /// `Entity.getEyeHeight`: players 1.62 (1.54 sneaking), armor stands 0.9 of the height (0.5
    /// when small), the spider, cave spider and enderman overrides, else 0.85 of the height.
    pub fn eye_height(&self) -> f32 {
        match self.kind {
            Kind::Player => 1.62 - if self.sneaking() { 0.08 } else { 0.0 },
            Kind::Object(78) => self.size().1 * if self.is_child() { 0.5 } else { 0.9 },
            Kind::Object(77) => -0.0625,
            Kind::Mob(52) => 0.65,
            Kind::Mob(59) => 0.45,
            Kind::Mob(58) => 2.55,
            _ => self.size().1 * 0.85,
        }
    }

    /// `handleEntityAttach` with the leash flag: only a mob (`EntityLiving`) can be leashed, and a negative
    /// holder id unties it.
    fn set_leash(&mut self, holder: i32) {
        if matches!(self.kind, Kind::Mob(_)) {
            self.leash = (holder >= 0).then_some(holder);
        }
    }

    /// `isChild`: a small armor stand, a baby zombie or a baby ageable mob.
    pub fn is_child(&self) -> bool {
        match self.kind {
            Kind::Object(78) => self.meta.stand.unwrap_or(0) & stand::SMALL != 0,
            kind => is_baby(kind, &self.meta),
        }
    }

    /// Axis-aligned hitbox `(min, max)` at the current position.
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        match self.kind {
            Kind::Painting => {
                let (_, w, h, ..) = PAINTINGS
                    .iter()
                    .find(|p| p.0 == self.title)
                    .copied()
                    .unwrap_or(PAINTINGS[0]);
                hanging_bounds(self.pos, self.facing, w, h)
            }
            Kind::Object(71) => hanging_bounds(self.pos, self.facing, 12, 12),
            Kind::Object(77) => leash_knot_bounds(self.pos),
            _ => bounds_at(self.pos, self.size()),
        }
    }

    /// Hitbox `(width, height)`: armor stands (marker only; 1.8.9 keeps the box of a small one), baby mobs and
    /// slimes size themselves from metadata (`EntityArmorStand`, `EntityAgeable.setScaleForAge`, `EntitySlime.setSlimeSize`).
    pub fn size(&self) -> (f32, f32) {
        let (w, h) = self.kind.size();
        let m = self.meta;
        let half = |baby: bool| if baby { (w * 0.5, h * 0.5) } else { (w, h) };
        match self.kind {
            Kind::Object(78) if m.stand.unwrap_or(0) & stand::MARKER != 0 => (0.0, 0.0),
            Kind::Mob(55 | 62) => {
                let s = 0.51000005 * f32::from(m.slime.unwrap_or(1).max(1));
                (s, s)
            }
            Kind::Mob(54 | 57 | 90..=93 | 95 | 96 | 98 | 100 | 101 | 120) => half(self.is_child()),
            _ => (w, h),
        }
    }

    fn set_target(&mut self, raw: [i32; 3], yaw: f32, pitch: f32) {
        self.server_pos = raw;
        self.target.pos = raw.map(fixed);
        self.target.yaw = yaw;
        self.target.pitch = pitch;
        if matches!(self.kind, Kind::Object(t) if crate::projectile::snaps(t)) {
            // `Entity.lerpPositionAndAngles` of a non-living entity sets the pose at once.
            (self.pos, self.yaw, self.pitch) = (self.target.pos, yaw, pitch);
            self.target.steps = 0;
        } else {
            self.target.steps = STEPS;
        }
    }

    /// `handleEntityTeleport`: a teleport within 1/32 (x, z) and 1/64 (y) of the entity eases to where it already is.
    fn teleport(&mut self, raw: [i32; 3], yaw: f32, pitch: f32, on_ground: bool) {
        let to = raw.map(fixed);
        let small = (to[0] - self.pos[0]).abs() < 0.03125
            && (to[1] - self.pos[1]).abs() < 0.015625
            && (to[2] - self.pos[2]).abs() < 0.03125;
        let here = self.pos;
        self.set_target(raw, yaw, pitch);
        if small {
            (self.pos, self.target.pos) = (here, here);
        }
        self.on_ground = on_ground;
    }

    /// `handleEntityMove`: a move without angles keeps the current yaw and pitch, so a turn in progress stops.
    fn relative_move(&mut self, delta: [i8; 3], look: Option<(i8, i8)>, on_ground: bool) {
        let raw = std::array::from_fn(|i| self.server_pos[i] + i32::from(delta[i]));
        let (yaw, pitch) = look.map_or((self.yaw, self.pitch), |(y, p)| (angle(y), angle(p)));
        self.set_target(raw, yaw, pitch);
        self.on_ground = on_ground;
    }

    /// Client-side flight of arrows, thrown objects, items, TNT, falling blocks and bobbers; an arrow that ends on a hit
    /// plays its sound.
    fn fly(
        &mut self,
        at: &dyn Fn([i32; 3]) -> u16,
        targets: &[crate::projectile::Target],
    ) -> Option<Fx> {
        let Kind::Object(object) = self.kind else {
            return None;
        };
        if !crate::projectile::simulated(object) {
            return None;
        }
        let mut b = crate::projectile::Body {
            id: self.id,
            owner: self.owner,
            targets,
            removed: false,
            hit_sound: None,
            age: self.age,
            size: self.size(),
            pos: self.pos,
            vel: self.vel,
            yaw: self.yaw,
            pitch: self.pitch,
            prev_yaw: self.prev_yaw,
            prev_pitch: self.prev_pitch,
            on_ground: self.on_ground,
            proj: std::mem::take(&mut self.proj),
        };
        crate::projectile::step(object, &mut b, at);
        (self.pos, self.vel, self.yaw, self.pitch) = (b.pos, b.vel, b.yaw, b.pitch);
        (self.prev_yaw, self.prev_pitch, self.on_ground, self.proj) =
            (b.prev_yaw, b.prev_pitch, b.on_ground, b.proj);
        self.removed |= b.removed;
        b.hit_sound.map(|(pos, pitch)| Fx::BowHit { pos, pitch })
    }

    /// One client tick: interpolation toward the server pose, limb swing, swing and hurt timers.
    fn tick(
        &mut self,
        at: &dyn Fn([i32; 3]) -> u16,
        targets: &[crate::projectile::Target],
    ) -> Option<Fx> {
        let mut fx = None;
        self.age += 1;
        // `EnderDragonEntity.mobTick` stores yaw and height before the position eases toward the server's.
        if let Some(dragon) = &mut self.dragon {
            dragon.tick(
                self.vel,
                self.yaw,
                self.pos[1],
                self.health.is_some_and(|h| h <= 0.0),
            );
        }
        self.use_ticks =
            (self.flags & flag::USING_ITEM != 0).then(|| self.use_ticks.map_or(0, |t| t + 1));
        self.prev_pos = self.pos;
        (self.prev_yaw, self.prev_pitch) = (self.yaw, self.pitch);
        self.last_body_yaw = self.body_yaw;
        self.last_head_yaw = self.head_yaw;
        let interpolating = self.target.steps > 0;
        if interpolating {
            let n = f64::from(self.target.steps);
            for i in 0..3 {
                self.pos[i] += (self.target.pos[i] - self.pos[i]) / n;
            }
            let k = self.target.steps as f32;
            self.yaw += wrap(self.target.yaw - self.yaw) / k;
            self.pitch += (self.target.pitch - self.pitch) / k;
            self.target.steps -= 1;
        } else {
            fx = self.fly(at, targets);
        }

        // `EntityLivingBase.onUpdate` -> `updateLimbSwing`.
        self.prev_limb_swing_amount = self.limb_swing_amount;
        let dx = self.pos[0] - self.prev_pos[0];
        let dz = self.pos[2] - self.prev_pos[2];
        let f = ((dx * dx + dz * dz).sqrt() as f32 * 4.0).min(1.0);
        self.limb_swing_amount += (f - self.limb_swing_amount) * 0.4;
        self.limb_swing += self.limb_swing_amount;

        if self.swinging {
            self.swing_ticks += 1;
            if self.swing_ticks >= self.effects.arm_swing_duration() {
                self.swing_ticks = 0;
                self.swinging = false;
            }
        } else {
            self.swing_ticks = 0;
        }
        if let Kind::Mob(id @ (55 | 62)) = self.kind {
            self.squish.tick(self.on_ground, id == 62);
        }
        if self.kind == Kind::Mob(68) {
            // `Entity.handleWaterMovement`: the box shrunk by 0.4 above and below, as the squid's by 0.6.
            let (w, h) = self.size();
            let hw = f64::from(w) / 2.0 - 0.001;
            let min = [
                self.pos[0] - hw,
                self.pos[1] + 0.4 + 0.001,
                self.pos[2] - hw,
            ];
            let max = [
                self.pos[0] + hw,
                self.pos[1] + f64::from(h) - 0.4 - 0.001,
                self.pos[2] + hw,
            ];
            let in_water = crate::fluid::water_drag(at, min, max, false).0;
            let moving = self.meta.guardian.unwrap_or(0) & crate::mob_anim::GUARDIAN_MOVING != 0;
            self.anim.guardian.tick(in_water, moving);
        }
        if self.kind == Kind::Mob(SQUID) {
            let (w, h) = self.size();
            let (hw, hh) = (f64::from(w) / 2.0 - 0.001, f64::from(h));
            let min = [
                self.pos[0] - hw,
                self.pos[1] + 0.6 + 0.001,
                self.pos[2] - hw,
            ];
            let max = [
                self.pos[0] + hw,
                self.pos[1] + hh - 0.6 - 0.001,
                self.pos[2] + hw,
            ];
            let in_water = crate::fluid::water_drag(at, min, max, false).0;
            self.squid.tick(
                &mut self.vel,
                [dx, dz],
                self.yaw,
                interpolating,
                in_water,
                &mut self.head_yaw,
            );
        }
        self.turn_body([dx, dz]);
        if self.hurt_time > 0 {
            self.hurt_time -= 1;
        }
        if self.death_time > 0 {
            self.death_time += 1;
        }
        self.sheep_timer = (self.sheep_timer - 1).max(0);
        self.anim.begging = self.meta.begging == Some(1);
        self.anim.tick();
        self.anim.horse.tick(self.meta.horse_flags.unwrap_or(0));
        fx
    }

    /// `LivingEntity.tick` after the entity moved by `moved`: a player's body follows `bodyMovement`,
    /// a mob's follows its `BodyControl`. The squid keeps its own state, an armor stand's body is its yaw.
    fn turn_body(&mut self, moved: [f64; 2]) {
        match self.kind {
            Kind::Player => {
                let aim =
                    crate::body::player_aim(self.body_yaw, self.yaw, moved, self.swing_ticks > 0);
                self.body_yaw = crate::body::body_movement(self.body_yaw, self.yaw, aim);
            }
            Kind::Mob(SQUID | DRAGON) => {}
            Kind::Mob(_) => {
                self.body_control
                    .tick(self.yaw, moved, &mut self.body_yaw, &mut self.head_yaw)
            }
            _ => {}
        }
    }

    fn set_metadata(
        &mut self,
        flags: Option<u8>,
        name: Option<String>,
        stack: Option<ItemRef>,
        health: Option<f32>,
        meta: SizeMeta,
    ) {
        if let Some(f) = flags {
            self.flags = f;
        }
        if let (Kind::Mob(_) | Kind::Object(78), Some(n)) = (self.kind, name) {
            self.name = Some(n).filter(|n| !n.is_empty());
        }
        if stack.is_some() {
            self.stack = stack;
        }
        if health.is_some() {
            self.health = health;
        }
        self.meta.merge(meta);
    }

    fn animate(&mut self, animation: u8) {
        match animation {
            // `swingItem`: restart unless a swing is already in its first half.
            0 if !self.swinging
                || self.swing_ticks >= self.effects.arm_swing_duration() / 2
                || self.swing_ticks < 0 =>
            {
                self.swing_ticks = -1;
                self.swinging = true;
            }
            1 => self.hurt_time = 10,
            _ => {}
        }
    }

    fn set_status(&mut self, status: i8) {
        if let Kind::Mob(m) = self.kind {
            self.anim.set_status(m, status);
            if m == SQUID {
                self.squid.set_status(status);
            }
        }
        match status {
            // `handleStatusUpdate(2)`: the hit also kicks the limb swing to 1.5.
            2 => {
                self.limb_swing_amount = 1.5;
                self.hurt_time = 10;
            }
            // `EntitySheep.handleStatusUpdate(10)`.
            10 if self.kind == Kind::Mob(91) => self.sheep_timer = 40,
            3 if self.death_time == 0 => {
                self.hurt_time = 10;
                self.death_time = 1;
            }
            _ => {}
        }
    }

    /// The particles `handleStatusUpdate` starts for `status` on this mob type.
    fn status_fx(&self, status: i8) -> Option<Fx> {
        let Kind::Mob(kind) = self.kind else {
            return None;
        };
        let (pos, size) = (self.pos, self.size());
        // Pig, sheep, cow, chicken, wolf, mooshroom, ocelot, horse, rabbit extend `EntityAnimal`;
        // wolf and ocelot extend `EntityTameable`; horse sends taming particles itself.
        let animal = matches!(kind, 90..=93 | 95 | 96 | 98 | 100 | 101);
        let taming = matches!(kind, 95 | 98 | 100);
        let mood = match (status, kind) {
            (20, _) => return Some(Fx::Death { pos, size }),
            (18, _) if animal => Mood::Hearts,
            (7, _) if taming => Mood::Hearts,
            (6, _) if taming => Mood::Smoke,
            (12, 120) => Mood::VillagerHearts,
            (13, 120) => Mood::Angry,
            (14, 120) => Mood::Happy,
            (15, 66) => return Some(Fx::Witch { pos, size }),
            _ => return None,
        };
        Some(Fx::Mood { mood, pos, size })
    }

    /// Arm swing progress in `0.0..1.0`, interpolated between ticks (`getSwingProgress`).
    pub fn swing_progress(&self, partial: f32) -> f32 {
        if !self.swinging {
            return 0.0;
        }
        (self.swing_ticks as f32 + partial) / self.effects.arm_swing_duration() as f32
    }

    /// Where a rider with `rider_y_offset` (`getYOffset`) sits on this vehicle standing at `pos`: `Entity.updateRiderPosition`, and for a horse the
    /// `EntityHorse.updateRiderPosition` shift forward and up while it rears.
    pub fn rider_seat(&self, [x, y, z]: [f64; 3], rider_y_offset: f64) -> [f64; 3] {
        let mut seat = [x, y + self.kind.mounted_y_offset() + rider_y_offset, z];
        let rearing = self.anim.horse.prev_rearing();
        if self.kind == Kind::Mob(100) && rearing > 0.0 {
            let (s, c) = (self.yaw * std::f32::consts::PI / 180.0).sin_cos();
            seat[0] += f64::from(0.7 * rearing * s);
            seat[1] += f64::from(0.15 * rearing);
            seat[2] -= f64::from(0.7 * rearing * c);
        }
        seat
    }

    /// Position between the previous and current tick.
    pub fn lerp_pos(&self, partial: f64) -> [f64; 3] {
        std::array::from_fn(|i| self.prev_pos[i] + (self.pos[i] - self.prev_pos[i]) * partial)
    }

    /// Yaw and pitch between the previous and current tick, yaw the short way round
    /// (`orientCamera` after `EntityLivingBase.onUpdate` wraps `prevRotationYaw`).
    /// Yaw, head yaw and pitch of the frame, as `LivingEntityRenderer.render` eases them: the yaw is
    /// the body yaw of a player or mob, the look yaw of anything else.
    pub fn lerp_pose(&self, partial: f32) -> (f32, f32, f32) {
        let (yaw, pitch) = self.lerp_look(partial);
        let ease = |last: f32, now: f32| last + wrap(now - last) * partial;
        let head = ease(self.last_head_yaw, self.head_yaw);
        match self.kind {
            Kind::Player => (ease(self.last_body_yaw, self.body_yaw), head, pitch),
            Kind::Mob(SQUID | DRAGON) => (yaw, head, pitch),
            Kind::Mob(_) => (ease(self.last_body_yaw, self.body_yaw), head, pitch),
            _ => (yaw, head, pitch),
        }
    }

    pub fn lerp_look(&self, partial: f32) -> (f32, f32) {
        (
            self.prev_yaw + wrap(self.yaw - self.prev_yaw) * partial,
            self.prev_pitch + (self.pitch - self.prev_pitch) * partial,
        )
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

fn bounds_at(pos: [f64; 3], (w, h): (f32, f32)) -> ([f64; 3], [f64; 3]) {
    let half = f64::from(w) / 2.0;
    (
        [pos[0] - half, pos[1], pos[2] - half],
        [pos[0] + half, pos[1] + f64::from(h), pos[2] + half],
    )
}

/// Packet-level changes, already decoded. Positions are the raw 1/32 block fixed-point values and
/// angles the raw 1/256 turn bytes, exactly as on the wire.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityEvent {
    SpawnPlayer {
        id: i32,
        uuid: u128,
        pos: [i32; 3],
        yaw: i8,
        pitch: i8,
        held: i16,
        flags: Option<u8>,
    },
    SpawnMob {
        id: i32,
        kind: u8,
        pos: [i32; 3],
        yaw: i8,
        pitch: i8,
        head_yaw: i8,
        vel: [i16; 3],
        flags: Option<u8>,
        name: Option<String>,
        health: Option<f32>,
        size: SizeMeta,
    },
    SpawnObject {
        id: i32,
        kind: i8,
        pos: [i32; 3],
        yaw: i8,
        pitch: i8,
        vel: [i16; 3],
        data: i32,
    },
    SpawnExperienceOrb {
        id: i32,
        pos: [i32; 3],
    },
    /// `SpawnPainting`: motive name, block position and horizontal facing.
    SpawnPainting {
        id: i32,
        title: String,
        pos: [i32; 3],
        facing: u8,
    },
    Move {
        id: i32,
        delta: [i8; 3],
        look: Option<(i8, i8)>,
        on_ground: bool,
    },
    Look {
        id: i32,
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    },
    Teleport {
        id: i32,
        pos: [i32; 3],
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    },
    HeadLook {
        id: i32,
        head_yaw: i8,
    },
    Velocity {
        id: i32,
        vel: [i16; 3],
    },
    Destroy(Vec<i32>),
    Metadata {
        id: i32,
        flags: Option<u8>,
        name: Option<String>,
        health: Option<f32>,
        stack: Option<ItemRef>,
        size: SizeMeta,
    },
    /// Metadata index 8 of a firework rocket: the `Fireworks.Explosions` of its item.
    Firework {
        id: i32,
        explosions: Vec<FireworkExplosion>,
    },
    /// Metadata index 7 (potion colour) and 8 (ambient), where a packet carried them.
    Potion {
        id: i32,
        color: Option<i32>,
        ambient: Option<bool>,
    },
    Equipment {
        id: i32,
        slot: i16,
        item: Option<ItemRef>,
    },
    /// `S0B`: 0 swing, 1 take damage, 2 leave bed, 3 eat, 4 critical, 5 magic critical.
    Animation {
        id: i32,
        animation: u8,
    },
    /// `S1D`: the entity gets a potion effect.
    Effect {
        id: i32,
        effect: crate::effects::Effect,
    },
    /// `S1E`: the entity loses the potion effect `effect_id`.
    RemoveEffect {
        id: i32,
        effect_id: i8,
    },
    /// `S0D` collect item: the entity flies to the collector and is picked up.
    Pickup {
        id: i32,
        collector: i32,
    },
    /// `S1A`: 2 hurt, 3 dead.
    Status {
        id: i32,
        status: i8,
    },
    Attach {
        id: i32,
        vehicle: i32,
    },
    /// `S1B` with the leash flag: the mob is tied to `holder`, or untied when `holder` is negative.
    Leash {
        id: i32,
        holder: i32,
    },
    /// Player list entry: names for `SpawnPlayer` uuids.
    PlayerName {
        uuid: u128,
        name: String,
        /// Base64 value of the profile's `textures` property.
        textures: Option<String>,
    },
    PlayerRemove {
        uuid: u128,
    },
    /// Respawn or dimension change: every entity is gone.
    Clear,
}

#[derive(Default)]
pub struct Entities {
    map: HashMap<i32, Entity>,
    names: HashMap<u128, (String, Option<String>)>,
    /// Running `EmitterParticle`s from animations 4 and 5.
    emitters: Vec<Emitter>,
    /// Particle requests of the last ticks; the engine drains them with `take_fx`.
    fx: Vec<Fx>,
    /// The local player as an arrow or bobber target: it is not in `map`.
    local: Option<crate::projectile::Target>,
    /// `EntityPickupParticle`s: items that fly to their collector for `PICKUP_TICKS`.
    pickups: Vec<Pickup>,
}

/// `EntityPickupParticle`: the removed entity, frozen where it was, and the entity it flies to.
struct Pickup {
    entity: Entity,
    collector: i32,
    age: u8,
}

/// `EntityPickupParticle.maxAge`.
const PICKUP_TICKS: u8 = 3;
/// The `yOffset` `handleEntityPickup` gives the particle: the target is half a block above the collector's feet.
const PICKUP_Y_OFFSET: f64 = 0.5;

/// `EntityPickupParticle.renderParticle`: the entity's position `age` ticks and `partial` into the flight,
/// moving from `from` to `to` by `((age + partial) / 3)^2`.
pub fn pickup_pos(age: u8, partial: f32, from: [f64; 3], to: [f64; 3]) -> [f64; 3] {
    let f = (f32::from(age) + partial) / f32::from(PICKUP_TICKS);
    let f = f64::from(f * f);
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * f)
}

/// `EmitterParticle`: one burst per tick for `EMITTER_TICKS` ticks, started by animation 4 (crit) or 5 (enchant crit).
struct Emitter {
    id: i32,
    magic: bool,
    age: u8,
}

const EMITTER_TICKS: u8 = 3;

/// Particles an entity causes; `size` is `(width, height)` and positions are feet-level.
#[derive(Clone, Debug, PartialEq)]
pub enum Fx {
    /// One `EmitterParticle.tick` burst around the entity (`magic` is the enchant crit).
    Hit {
        magic: bool,
        pos: [f64; 3],
        size: (f32, f32),
    },
    /// `EntityLivingBase.onDeathUpdate` at tick 20: the 20-particle puff.
    Death { pos: [f64; 3], size: (f32, f32) },
    /// A `handleStatusUpdate` particle shower in the entity's box (see `Mood`).
    Mood {
        mood: Mood,
        pos: [f64; 3],
        size: (f32, f32),
    },
    /// `EntityWitch.handleStatusUpdate(15)`: witch magic specks around the head; `size` is `(width, height)`.
    Witch { pos: [f64; 3], size: (f32, f32) },
    /// `ArrowEntity.tick`: `random.bowhit` at an arrow that `takeDamage` accepted, with this pitch.
    BowHit { pos: [f64; 3], pitch: f32 },
    /// `EntityFireworkRocket.handleStatusUpdate(17)`: the `Starter` at the rocket, moving as it did.
    Firework {
        pos: [f64; 3],
        vel: [f64; 3],
        explosions: Vec<FireworkExplosion>,
    },
}

impl Entity {
    /// What an arrow or bobber can meet in this entity: `hasCollision` (see `collidable`; the dragon's own box has
    /// none), the class that `takeDamage` answers `true` for on a client, and the enderman exemption.
    fn target(&self) -> Option<crate::projectile::Target> {
        use crate::projectile::Hurt;
        if !collidable(self) || self.kind == Kind::Mob(63) {
            return None;
        }
        let (min, max) = self.bounds();
        let hurt = match self.kind {
            Kind::Player | Kind::Painting | Kind::Object(1 | 10..=12 | 51 | 71 | 77) => Hurt::Yes,
            Kind::Object(63) => Hurt::IfAttacker,
            _ => Hurt::No,
        };
        Some(crate::projectile::Target {
            id: self.id,
            min,
            max,
            hurt,
            player: self.kind == Kind::Player,
            invulnerable: false,
            enderman: self.kind == Kind::Mob(58),
        })
    }
}

/// The particle of a mob mood status: `EntityAnimal` 18, `EntityTameable` 6 and 7, `EntityVillager` 12 to 14.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Hearts,
    Smoke,
    VillagerHearts,
    Angry,
    Happy,
}

impl Mood {
    /// Animals and tameables spawn 7 particles from half a block above the feet;
    /// villagers spawn 5 from one block above.
    pub fn count_and_rise(self) -> (u32, f64) {
        match self {
            Mood::Hearts | Mood::Smoke => (7, 0.5),
            Mood::VillagerHearts | Mood::Angry | Mood::Happy => (5, 1.0),
        }
    }
}

impl Entities {
    pub fn get(&self, id: i32) -> Option<&Entity> {
        self.map.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.map.values()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Name the player list has for `uuid`.
    pub fn name_of(&self, uuid: u128) -> Option<&str> {
        self.names.get(&uuid).map(|n| n.0.as_str())
    }

    pub fn apply(&mut self, ev: EntityEvent) {
        use EntityEvent as E;
        match ev {
            E::SpawnPlayer { .. } | E::SpawnMob { .. } | E::SpawnObject { .. } => self.spawn(ev),
            E::SpawnPainting {
                id,
                title,
                pos,
                facing,
            } => {
                let (_, w, h, ..) = PAINTINGS
                    .iter()
                    .find(|p| p.0 == title)
                    .copied()
                    .unwrap_or(PAINTINGS[0]);
                let mut e = Entity::new(id, Kind::Painting, hanging_raw(pos, facing, w, h), 0, 0);
                (e.facing, e.title) = (facing & 3, title);
                self.map.insert(id, e);
            }
            E::SpawnExperienceOrb { id, pos } => {
                let e = Entity::new(id, Kind::ExperienceOrb, pos, 0, 0);
                self.map.insert(id, e);
            }
            E::Move {
                id,
                delta,
                look,
                on_ground,
            } => self.with(id, |e| e.relative_move(delta, look, on_ground)),
            E::Look {
                id,
                yaw,
                pitch,
                on_ground,
            } => self.with(id, |e| {
                e.set_target(e.server_pos, angle(yaw), angle(pitch));
                e.on_ground = on_ground;
            }),
            E::Teleport {
                id,
                pos,
                yaw,
                pitch,
                on_ground,
            } => self.with(id, |e| e.teleport(pos, angle(yaw), angle(pitch), on_ground)),
            E::HeadLook { id, head_yaw } => self.with(id, |e| e.head_yaw = angle(head_yaw)),
            E::Velocity { id, vel } => self.with(id, |e| e.vel = velocity(vel)),
            E::Destroy(ids) => ids.iter().for_each(|id| {
                self.map.remove(id);
            }),
            E::Clear => {
                self.map.clear();
                self.pickups.clear();
            }
            E::Pickup { id, collector } => {
                if let Some(entity) = self.map.remove(&id) {
                    self.pickups.push(Pickup {
                        entity,
                        collector,
                        age: 0,
                    });
                }
            }
            other => self.apply_state(other),
        }
    }

    fn with(&mut self, id: i32, f: impl FnOnce(&mut Entity)) {
        if let Some(e) = self.map.get_mut(&id) {
            f(e);
        }
    }

    fn spawn(&mut self, ev: EntityEvent) {
        let (id, e) = match ev {
            EntityEvent::SpawnPlayer {
                id,
                uuid,
                pos,
                yaw,
                pitch,
                held,
                flags,
            } => {
                let mut e = Entity::new(id, Kind::Player, pos, yaw, pitch);
                // `renderYawOffset` of a fresh player is 0 until its first tick turns it toward the yaw.
                (e.body_yaw, e.last_body_yaw) = (0.0, 0.0);
                e.uuid = Some(uuid);
                if let Some((name, textures)) = self.names.get(&uuid) {
                    e.name = Some(name.clone());
                    e.textures = textures.clone();
                }
                e.flags = flags.unwrap_or(0);
                if held > 0 {
                    e.equipment[0] = Some(ItemRef {
                        id: held,
                        damage: 0,
                        count: 1,
                        dye: None,
                        glint: false,
                        owner: None,
                    });
                }
                (id, e)
            }
            EntityEvent::SpawnMob {
                id,
                kind,
                pos,
                yaw,
                pitch,
                head_yaw,
                vel,
                flags,
                name,
                health,
                size,
            } => {
                let mut e = Entity::new(id, Kind::Mob(kind), pos, yaw, pitch);
                e.head_yaw = angle(head_yaw);
                (e.body_yaw, e.last_body_yaw, e.last_head_yaw) =
                    (e.head_yaw, e.head_yaw, e.head_yaw);
                e.vel = velocity(vel);
                e.flags = flags.unwrap_or(0);
                e.meta = size;
                e.name = name.filter(|n| !n.is_empty());
                e.health = health;
                (id, e)
            }
            EntityEvent::SpawnObject {
                id,
                kind,
                pos,
                yaw,
                pitch,
                vel,
                data,
            } => {
                let mut e = Entity::new(id, Kind::Object(kind), pos, yaw, pitch);
                if kind == 71 {
                    // The packet carries the block position of an item frame.
                    let block = pos.map(|c| c.div_euclid(32));
                    e = Entity::new(
                        id,
                        Kind::Object(kind),
                        hanging_raw(block, data as u8, 12, 12),
                        0,
                        0,
                    );
                    e.facing = data as u8 & 3;
                }
                e.vel = velocity(vel);
                e.meta.vehicle.cart = crate::vehicle::cart_type(kind, data);
                if matches!(kind, 60 | 90) {
                    e.owner = data;
                }
                if kind == 70 {
                    // `Block.deserialize`: id in the low 12 bits, metadata in the next 4.
                    e.stack = Some(ItemRef {
                        id: (data & 4095) as i16,
                        damage: (data >> 12 & 15) as i16,
                        count: 1,
                        dye: None,
                        glint: false,
                        owner: None,
                    });
                }
                (id, e)
            }
            _ => return,
        };
        self.map.insert(id, e);
    }

    /// Metadata, equipment, animation and status events.
    fn apply_state(&mut self, ev: EntityEvent) {
        use EntityEvent as E;
        match ev {
            E::Metadata {
                id,
                flags,
                name,
                health,
                stack,
                size,
            } => self.with(id, |e| e.set_metadata(flags, name, stack, health, size)),
            E::Potion { id, color, ambient } => self.with(id, |e| {
                e.potion_color = color.unwrap_or(e.potion_color);
                e.potion_ambient = ambient.unwrap_or(e.potion_ambient);
            }),
            E::Equipment { id, slot, item } => self.with(id, |e| {
                if let Some(s) = e.equipment.get_mut(slot as usize) {
                    *s = item;
                }
            }),
            E::Animation { id, animation } => {
                if let 4 | 5 = animation {
                    self.emit_hit(id, animation == 5);
                }
                self.with(id, |e| e.animate(animation))
            }
            E::Effect { .. } | E::RemoveEffect { .. } => self.apply_effect(ev),
            E::Firework { id, explosions } => self.with(id, |e| e.explosions = explosions),
            E::Status { id, status } => self.handle_status(id, status),
            E::Attach { id, vehicle } => {
                self.with(id, |e| e.vehicle = (vehicle >= 0).then_some(vehicle));
            }
            E::Leash { id, holder } => self.with(id, |e| e.set_leash(holder)),
            E::PlayerName { .. } | E::PlayerRemove { .. } => self.apply_player(ev),
            _ => {}
        }
    }

    fn handle_status(&mut self, id: i32, status: i8) {
        if status == 17 {
            self.firework_explode(id);
            return;
        }
        if let Some(fx) = self.map.get(&id).and_then(|e| e.status_fx(status)) {
            self.fx.push(fx);
        }
        self.with(id, |e| e.set_status(status));
    }

    fn apply_player(&mut self, ev: EntityEvent) {
        match ev {
            EntityEvent::PlayerName {
                uuid,
                name,
                textures,
            } => {
                for e in self.map.values_mut().filter(|e| e.uuid == Some(uuid)) {
                    e.name = Some(name.clone());
                    e.textures = textures.clone();
                }
                self.names.insert(uuid, (name, textures));
            }
            EntityEvent::PlayerRemove { uuid } => {
                self.names.remove(&uuid);
            }
            _ => {}
        }
    }

    fn apply_effect(&mut self, ev: EntityEvent) {
        match ev {
            EntityEvent::Effect { id, effect } => self.with(id, |e| e.effects.add(effect)),
            EntityEvent::RemoveEffect { id, effect_id } => {
                self.with(id, |e| e.effects.remove(effect_id))
            }
            _ => {}
        }
    }

    fn firework_explode(&mut self, id: i32) {
        if let Some(e) = self.map.get(&id).filter(|e| e.kind == Kind::Object(76)) {
            self.fx.push(Fx::Firework {
                pos: e.pos,
                vel: [0, 1, 2].map(|i| e.pos[i] - e.prev_pos[i]),
                explosions: e.explosions.clone(),
            });
        }
    }

    /// Sets `Entity.isWet()` of every wolf for the next [`Entities::tick`]: in water, or rained on at the feet or
    /// the head (`canLightningStrike`, given by `rained_on`); `block` gives block states.
    pub fn update_wet(
        &mut self,
        block: impl Fn(i32, i32, i32) -> u16,
        rained_on: impl Fn(i32, i32, i32) -> bool,
    ) {
        for e in self.map.values_mut().filter(|e| e.kind == Kind::Mob(95)) {
            let height = f64::from(e.size().1);
            let at = |y: f64| {
                rained_on(
                    e.pos[0].floor() as i32,
                    y.floor() as i32,
                    e.pos[2].floor() as i32,
                )
            };
            e.anim.in_water = crate::motion_sound::in_water(e.pos, height, &block)
                || at(e.pos[1])
                || at(e.pos[1] + height);
        }
    }

    /// One 20 TPS step for every tracked entity with no water anywhere; dead ones leave after their death animation.
    pub fn tick(&mut self) {
        self.tick_in(&|_| 0);
    }

    /// Sets the local player's box and `abilities.invulnerable` for the entity scan of arrows and bobbers; `None`
    /// for a spectator (`EntityFilter.NOT_SPECTATOR`) or before the join.
    pub fn set_local(&mut self, id: i32, min: [f64; 3], max: [f64; 3], invulnerable: bool) {
        self.local = Some(crate::projectile::Target {
            id,
            min,
            max,
            hurt: crate::projectile::Hurt::No,
            player: true,
            invulnerable,
            enderman: false,
        });
    }

    /// Drops the local target (see `set_local`).
    pub fn clear_local(&mut self) {
        self.local = None;
    }

    /// `tick`, with `at` giving the block state (`id << 4 | meta`) at a position, for the squid's `isInWater`.
    pub fn tick_in(&mut self, at: &dyn Fn([i32; 3]) -> u16) {
        // Only an arrow or a bobber scans for entities; its targets are the boxes as they stood at the tick start.
        let scans = self
            .map
            .values()
            .any(|e| matches!(e.kind, Kind::Object(60 | 90)));
        let targets: Vec<_> = if scans {
            self.map
                .values()
                .filter_map(Entity::target)
                .chain(self.local)
                .collect()
        } else {
            Vec::new()
        };
        for e in self.map.values_mut() {
            self.fx.extend(e.tick(at, &targets));
        }
        for i in 0..self.emitters.len() {
            self.burst(i);
        }
        self.pickups.retain_mut(|p| {
            p.age += 1;
            p.age < PICKUP_TICKS
        });
        self.emitters
            .retain(|m| m.age < EMITTER_TICKS && self.map.contains_key(&m.id));
        let fx = &mut self.fx;
        self.map.retain(|_, e| {
            let keep = e.death_time <= DEATH_TICKS;
            if !keep {
                fx.push(Fx::Death {
                    pos: e.pos,
                    size: e.size(),
                });
            }
            keep && !e.removed
        });
    }

    /// `EffectRenderer.emitParticleAtEntity`: starts the 3-tick crit (or, with `magic`, enchant crit) emitter on
    /// entity `id`. The server animation 4 and 5 and a local attack (`EntityPlayer.onCriticalHit`) both start it.
    pub fn emit_hit(&mut self, id: i32, magic: bool) {
        if self.map.contains_key(&id) {
            self.emitters.push(Emitter { id, magic, age: 0 });
            self.burst(self.emitters.len() - 1);
        }
    }

    /// `EmitterParticle.tick` for emitter `i`: one burst request, then one tick older.
    fn burst(&mut self, i: usize) {
        let m = &mut self.emitters[i];
        m.age += 1;
        if let Some(e) = self.map.get(&m.id) {
            self.fx.push(Fx::Hit {
                magic: m.magic,
                pos: e.pos,
                size: e.size(),
            });
        }
    }

    /// The entities flying to their collector, each placed by `pickup_pos`. `collector_pos` gives the interpolated
    /// feet position of a collector id; `None` leaves the entity in place.
    pub fn pickup_ghosts(
        &self,
        partial: f32,
        collector_pos: impl Fn(i32) -> Option<[f64; 3]>,
    ) -> Vec<Entity> {
        self.pickups
            .iter()
            .map(|p| {
                let mut e = p.entity.clone();
                if let Some(c) = collector_pos(p.collector) {
                    let to = [c[0], c[1] + PICKUP_Y_OFFSET, c[2]];
                    e.pos = pickup_pos(p.age, partial, e.pos, to);
                }
                e.prev_pos = e.pos;
                (e.prev_yaw, e.prev_pitch) = (e.yaw, e.pitch);
                e
            })
            .collect()
    }

    /// The particle requests since the last call.
    pub fn take_fx(&mut self) -> Vec<Fx> {
        std::mem::take(&mut self.fx)
    }

    /// The nearest collidable entity along the view ray within `reach`, ignoring `skip`
    /// (the local player). `vehicle` (the entity the local player rides) is hit only when no
    /// other entity is. `block_dist` is the distance to the block the crosshair hits; an
    /// entity behind it is not hit. Vanilla widens each hitbox by `getCollisionBorderSize`
    /// and also accepts a ray that starts inside the box.
    pub fn pick(
        &self,
        eye: [f64; 3],
        dir: [f64; 3],
        reach: f64,
        skip: i32,
        vehicle: Option<i32>,
        block_dist: Option<f64>,
    ) -> Option<(i32, f64)> {
        let limit = block_dist.map_or(reach, |b| b.min(reach));
        self.map
            .values()
            .filter(|e| e.id != skip)
            .filter(|e| collidable(e))
            .filter_map(|e| {
                let border = if e.kind == Kind::Object(63) { 1.0 } else { 0.1 };
                let (mut lo, mut hi) = e.bounds();
                for i in 0..3 {
                    lo[i] -= border;
                    hi[i] += border;
                }
                ray_box(eye, dir, lo, hi).map(|d| (Some(e.id) == vehicle, d, e.id))
            })
            .filter(|(_, d, _)| *d <= limit)
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
            .map(|(_, d, id)| (id, d))
    }

    /// `EntityThrowable.onUpdate` entity scan along the step `from`..`to`: the distance to the
    /// first collidable entity whose box, grown by 0.3, the step crosses. While the throwable
    /// is `young` (under 5 ticks in the air) an entity it starts inside is skipped: the client does not know the thrower, so this stands in for the thrower exemption. `player` is the local player's box, which is not in the map.
    pub fn hit_along(
        &self,
        from: [f64; 3],
        to: [f64; 3],
        young: bool,
        player: Option<([f64; 3], [f64; 3])>,
    ) -> Option<f64> {
        let d = [0, 1, 2].map(|i| to[i] - from[i]);
        let len = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        if len < 1e-9 {
            return None;
        }
        let dir = d.map(|v| v / len);
        self.map
            .values()
            .filter(|e| collidable(e) && !e.is_dead())
            .map(Entity::bounds)
            .chain(player)
            .filter_map(|(lo, hi)| {
                let ray = ray_box(from, dir, lo.map(|v| v - 0.3), hi.map(|v| v + 0.3))?;
                (ray <= len && !(young && ray == 0.0)).then_some(ray)
            })
            .min_by(f64::total_cmp)
    }
}

/// `Entity.hasCollision`: what a ray or a thrown entity can hit. `ArmorStandEntity.hasCollision`
/// is false for a marker stand.
fn collidable(e: &Entity) -> bool {
    let marker = e.kind == Kind::Object(78) && e.meta.stand.unwrap_or(0) & stand::MARKER != 0;
    !marker
        && !matches!(
            e.kind,
            Kind::ExperienceOrb
                | Kind::Object(2 | 60 | 61 | 62 | 64 | 65 | 66 | 72 | 73 | 75 | 76 | 90)
        )
}

fn velocity(v: [i16; 3]) -> [f64; 3] {
    v.map(|c| f64::from(c) / 8000.0)
}

/// Distance along a unit `dir` to the box, 0 when the origin is inside it.
fn ray_box(origin: [f64; 3], dir: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for i in 0..3 {
        if dir[i].abs() < 1e-12 {
            if origin[i] < lo[i] || origin[i] > hi[i] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[i] - origin[i]) / dir[i], (hi[i] - origin[i]) / dir[i]);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far && far >= 0.0).then(|| near.max(0.0))
}

/// Unit view vector of a camera with vanilla yaw and pitch degrees (`Entity.getLook`).
pub fn look_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

#[cfg(test)]
mod tests {

    fn spawn_object(es: &mut Entities, kind: i8, pos: [i32; 3], vel: [i16; 3]) {
        es.apply(EntityEvent::SpawnObject {
            id: 5,
            kind,
            pos,
            yaw: 0,
            pitch: 0,
            vel,
            data: 0,
        });
    }

    fn open_air(_: [i32; 3]) -> u16 {
        0
    }

    /// `ArrowEntity.tick` in open air: add the motion, then scale by 0.99 and drop 0.05, every tick.
    #[test]
    fn an_arrow_flies_with_drag_and_gravity_between_packets() {
        let mut es = Entities::default();
        spawn_object(&mut es, 60, [0, 100 * 32, 0], [8000, 0, 0]);
        let (mut x, mut y, mut vx, mut vy) = (0.0_f64, 100.0_f64, 1.0_f64, 0.0_f64);
        for _ in 0..20 {
            es.tick_in(&open_air);
            x += vx;
            y += vy;
            (vx, vy) = (
                vx * f64::from(0.99_f32),
                vy * f64::from(0.99_f32) - f64::from(0.05_f32),
            );
            let a = es.get(5).unwrap();
            assert!((a.pos[0] - x).abs() < 1e-9 && (a.pos[1] - y).abs() < 1e-9);
        }
    }

    /// `ItemEntity.tick`: gravity 0.04, then drag 0.98; a stone floor stops the fall and the item rests on it.
    #[test]
    fn a_dropped_item_falls_and_rests_on_a_block() {
        let stone = |p: [i32; 3]| if p[1] < 64 { 1 << 4 } else { 0 };
        let mut es = Entities::default();
        spawn_object(&mut es, 2, [0, 66 * 32, 0], [0; 3]);
        es.tick_in(&stone);
        let first = es.get(5).unwrap();
        assert!((first.pos[1] - (66.0 - 0.039_999_999_105_930_33)).abs() < 1e-12);
        for _ in 0..60 {
            es.tick_in(&stone);
        }
        let item = es.get(5).unwrap();
        assert_eq!(item.pos[1], 64.0);
        assert!(item.on_ground);
    }

    /// `ThrownEntity.tick`: a snowball falls 0.03 per tick under drag 0.99 and ignores blocks on the client.
    #[test]
    fn a_snowball_uses_gravity_0_03() {
        let mut es = Entities::default();
        spawn_object(&mut es, 61, [0, 100 * 32, 0], [0; 3]);
        es.tick_in(&open_air);
        es.tick_in(&open_air);
        let s = es.get(5).unwrap();
        assert!((s.pos[1] - 99.97).abs() < 1e-7);
        assert!((s.vel[1] - (-0.03 * 0.99 - 0.03)).abs() < 1e-7);
    }

    #[test]
    fn pickup_eases_toward_a_moving_collector() {
        let mut es = Entities::default();
        let item = Entity::new(7, Kind::Object(2), [10 * 32, 64 * 32, 0], 0, 0);
        es.map.insert(7, item);
        es.apply(EntityEvent::Pickup {
            id: 7,
            collector: 1,
        });
        assert!(es.get(7).is_none());
        // The collector walks 4 blocks on x per tick from x = 0; its feet are at y = 64.
        let expect = |age: u8, partial: f32| {
            let x = 4.0 * (f64::from(age) + f64::from(partial));
            let f = f64::from(((f32::from(age) + partial) / 3.0).powi(2));
            [10.0 + (x - 10.0) * f, 64.0 + 0.5 * f, 0.0]
        };
        for age in 0..3u8 {
            for partial in [0.0f32, 0.5] {
                let x = 4.0 * (f64::from(age) + f64::from(partial));
                let g = es.pickup_ghosts(partial, |_| Some([x, 64.0, 0.0]));
                let want = expect(age, partial);
                for i in 0..3 {
                    assert!(
                        (g[0].pos[i] - want[i]).abs() < 1e-6,
                        "age {age} {partial} axis {i}"
                    );
                }
            }
            es.tick();
        }
        assert!(es.pickup_ghosts(0.0, |_| Some([0.0; 3])).is_empty());
    }

    /// Oracle: `EntityHorse.updateRiderPosition` worked by hand. The rearing amount of the last tick is 0 after one
    /// angry tick and 0.45 after two (`0 + (1 - 0) * 0.4 + 0.05`); a horse facing west (yaw 90) shifts the rider
    /// 0.7 * 0.45 along +x and 0.15 * 0.45 up. The rider rests at 0.75 * 1.6 - 0.35 above the feet.
    #[test]
    fn rearing_horse_lifts_and_pushes_its_rider() {
        let mut horse = Entity::new(1, Kind::Mob(100), [0, 64 * 32, 0], 0, 0);
        horse.yaw = 90.0;
        horse.meta.horse_flags = Some(64);
        let rest = 64.0 + f64::from(1.6f32) * 0.75 - f64::from(0.35f32);
        horse.anim.horse.tick(64);
        assert_eq!(
            horse.rider_seat(horse.pos, -f64::from(0.35f32)),
            [0.0, rest, 0.0]
        );
        horse.anim.horse.tick(64);
        let [x, y, z] = horse.rider_seat(horse.pos, -f64::from(0.35f32));
        assert!((x - 0.315).abs() < 1e-6 && (y - rest - 0.0675).abs() < 1e-6 && z.abs() < 1e-6);
        horse.anim.horse.tick(0);
        horse.anim.horse.tick(0);
        assert!(horse.rider_seat(horse.pos, 0.0)[0] < 0.315);
    }

    /// Oracle: `EntityHanging.updateBoundingBox` and `EntityLeashKnot.updateBoundingBox` worked by hand for
    /// blocks at (0, 64, 0): the slab is 1/16 thick on the facing axis and sits 1/32 from the block face.
    #[test]
    fn hanging_entities_have_a_facing_dependent_box() {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnPainting {
            id: 1,
            title: "Pool".into(),
            pos: [0, 64, 0],
            facing: 1,
        });
        // West: the slab is thin in x, 2 wide in z (Pool is 32x16 pixels, centred one block along rotateYCCW).
        let (lo, hi) = es.get(1).unwrap().bounds();
        assert_eq!(
            (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]),
            (0.0625, 1.0, 2.0)
        );
        assert_eq!((lo[0], lo[1]), (0.96875 - 0.03125, 64.0));
        let frame = Entity {
            facing: 2,
            ..Entity::new(2, Kind::Object(71), [16, 2064, 16 - 15], 0, 0)
        };
        let (lo, hi) = frame.bounds();
        assert_eq!(
            (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]),
            (0.75, 0.75, 0.0625)
        );
        let (lo, hi) = Entity::new(3, Kind::Object(77), [0, 64 * 32, 0], 0, 0).bounds();
        assert_eq!(
            (lo, hi),
            ([0.3125, 64.375, 0.3125], [0.6875, 64.875, 0.6875])
        );
    }

    /// Oracle: `EntityHanging.updateBoundingBox` worked by hand for the 2x1 "Pool" painting on block (0, 64, 0):
    /// the slab is 1/16 thick on the facing axis, 2 wide on the other, and sits 1/32 inside the block face.
    fn assert_pool_box(facing: u8, x: [f64; 2], z: [f64; 2]) {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnPainting {
            id: 1,
            title: "Pool".into(),
            pos: [0, 64, 0],
            facing,
        });
        let (lo, hi) = es.get(1).unwrap().bounds();
        assert_eq!(
            ([lo[0], hi[0]], [lo[1], hi[1]], [lo[2], hi[2]]),
            (x, [64.0, 65.0], z),
            "facing {facing}"
        );
    }

    #[test]
    fn painting_box_facing_0() {
        assert_pool_box(0, [0.0, 2.0], [0.0, 0.0625]);
    }

    #[test]
    fn painting_box_facing_1() {
        assert_pool_box(1, [0.9375, 1.0], [0.0, 2.0]);
    }

    #[test]
    fn painting_box_facing_2() {
        assert_pool_box(2, [-1.0, 1.0], [0.9375, 1.0]);
    }

    #[test]
    fn painting_box_facing_3() {
        assert_pool_box(3, [0.0, 0.0625], [-1.0, 1.0]);
    }

    use super::*;

    /// `EntityHanging.updateBoundingBox`: a 2x1 painting facing north on the wall of block (4, 70, 9)
    /// sits 0.46875 towards the wall and half a block along the wall.
    #[test]
    fn painting_centres_on_its_wall_like_entity_hanging() {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnPainting {
            id: 5,
            title: "Pool".into(),
            pos: [4, 70, 9],
            facing: 2,
        });
        let e = es.get(5).unwrap();
        // North: the wall is to the south; `rotateYCCW` of north is west, so -x.
        assert_eq!(e.pos, [4.0, 70.5, 9.5 + 0.46875]);
        assert_eq!((e.facing, e.title.as_str()), (2, "Pool"));
    }

    fn spawn(es: &mut Entities, id: i32, pos: [i32; 3]) {
        es.apply(EntityEvent::SpawnMob {
            id,
            kind: 54,
            pos,
            yaw: 0,
            pitch: 0,
            head_yaw: 0,
            vel: [0; 3],
            flags: None,
            name: None,
            health: None,
            size: SizeMeta::default(),
        });
    }

    /// `EntitySquid.isInWater`: on land the pitch eases toward -90, in a water cell it does not.
    #[test]
    fn squid_swims_only_where_its_box_touches_water() {
        let pitch_after_tick = |at: &dyn Fn([i32; 3]) -> u16| {
            let mut es = Entities::default();
            es.apply(EntityEvent::SpawnMob {
                id: 1,
                kind: SQUID,
                pos: [0, 64 * 32, 0],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size: SizeMeta::default(),
            });
            es.tick_in(at);
            es.get(1).unwrap().squid.pose(1.0).pitch
        };
        assert!(pitch_after_tick(&|_| 0) < -1.0);
        assert!(pitch_after_tick(&|[_, y, _]| if y == 64 { 9 << 4 } else { 0 }).abs() < 1e-3);
    }

    #[test]
    fn relative_moves_accumulate_on_server_position_and_interpolate_in_three_ticks() {
        let mut es = Entities::default();
        spawn(&mut es, 7, [0, 64 * 32, 0]);
        for _ in 0..2 {
            es.apply(EntityEvent::Move {
                id: 7,
                delta: [16, 0, 0],
                look: None,
                on_ground: true,
            });
        }
        let t = es.get(7).unwrap().target.pos;
        assert_eq!(t[0], 1.0);
        for _ in 0..3 {
            es.tick();
        }
        assert!((es.get(7).unwrap().pos[0] - 1.0).abs() < 1e-9);
    }

    /// `handleEntityTeleport`: a teleport within 1/32 (x, z) and 1/64 (y) of the entity keeps its position.
    #[test]
    fn a_small_teleport_keeps_the_position_and_only_turns() {
        let mut es = Entities::default();
        spawn(&mut es, 7, [0, 64 * 32, 0]);
        es.apply(EntityEvent::Move {
            id: 7,
            delta: [16, 0, 0],
            look: None,
            on_ground: true,
        });
        es.tick();
        let here = es.get(7).unwrap().pos;
        let teleport = |es: &mut Entities, x: i32| {
            es.apply(EntityEvent::Teleport {
                id: 7,
                pos: [x, 64 * 32, 0],
                yaw: 64,
                pitch: 0,
                on_ground: true,
            });
        };
        // 5/32 = 0.15625 is 0.0104 from the entity at 0.1667: small, so the position stays.
        teleport(&mut es, 5);
        for _ in 0..3 {
            es.tick();
        }
        let e = es.get(7).unwrap();
        assert_eq!(e.pos, here);
        assert!((e.yaw - 90.0).abs() < 1e-3);
        // 2 blocks away is not small: the entity eases to the packet position.
        teleport(&mut es, 64);
        for _ in 0..3 {
            es.tick();
        }
        assert!((es.get(7).unwrap().pos[0] - 2.0).abs() < 1e-9);
    }

    /// `handleEntityMove` with no angles passes the entity's current yaw and pitch, so a turn stops.
    #[test]
    fn a_move_without_look_stops_a_turn_in_progress() {
        let mut es = Entities::default();
        spawn(&mut es, 7, [0, 64 * 32, 0]);
        es.apply(EntityEvent::Look {
            id: 7,
            yaw: 64,
            pitch: 0,
            on_ground: true,
        });
        es.tick();
        let yaw = es.get(7).unwrap().yaw;
        assert!(yaw > 0.0 && yaw < 90.0);
        es.apply(EntityEvent::Move {
            id: 7,
            delta: [1, 0, 0],
            look: None,
            on_ground: true,
        });
        for _ in 0..3 {
            es.tick();
        }
        assert_eq!(es.get(7).unwrap().yaw, yaw);
    }

    #[test]
    fn yaw_interpolates_the_short_way_around() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        let mut e = es.get(1).unwrap().clone();
        e.yaw = 170.0;
        e.target.yaw = angle(-127);
        e.target.steps = STEPS;
        e.tick(&|_| 0, &[]);
        assert!(e.yaw > 170.0, "went the long way: {}", e.yaw);
    }

    #[test]
    fn look_interpolates_between_ticks_the_short_way_around() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        let mut e = es.get(1).unwrap().clone();
        (e.prev_yaw, e.yaw, e.prev_pitch, e.pitch) = (170.0, -170.0, 10.0, 30.0);
        let (yaw, pitch) = e.lerp_look(0.5);
        assert!((yaw - 180.0).abs() < 1e-4, "went the long way: {yaw}");
        assert!((pitch - 20.0).abs() < 1e-4);
        e.tick(&|_| 0, &[]);
        assert_eq!((e.prev_yaw, e.prev_pitch), (-170.0, 30.0));
    }

    #[test]
    fn mob_eye_heights_match_vanilla_get_eye_height() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        let mut e = es.get(1).unwrap().clone();
        for (kind, want) in [(52, 0.65), (59, 0.45), (58, 2.55), (50, 1.7 * 0.85)] {
            e.kind = Kind::Mob(kind);
            assert!(
                (e.eye_height() - want).abs() < 1e-5,
                "mob {kind}: {}",
                e.eye_height()
            );
        }
    }

    #[test]
    fn death_status_removes_the_entity_after_the_animation() {
        let mut es = Entities::default();
        spawn(&mut es, 3, [0; 3]);
        es.apply(EntityEvent::Status { id: 3, status: 3 });
        for _ in 0..DEATH_TICKS - 1 {
            es.tick();
            assert!(es.get(3).is_some());
        }
        es.tick();
        assert!(es.get(3).is_none());
    }

    #[test]
    fn mood_statuses_pick_the_vanilla_particle_for_the_mob_type() {
        let mob = |kind| {
            let mut es = Entities::default();
            es.apply(EntityEvent::SpawnMob {
                id: 1,
                kind,
                pos: [0; 3],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size: SizeMeta::default(),
            });
            es
        };
        let moods = |kind, status| {
            let mut es = mob(kind);
            es.apply(EntityEvent::Status { id: 1, status });
            es.take_fx()
                .into_iter()
                .map(|f| match f {
                    Fx::Mood { mood, .. } => Some(mood),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(moods(120, 12), [Some(Mood::VillagerHearts)]);
        assert_eq!(moods(120, 13), [Some(Mood::Angry)]);
        assert_eq!(moods(120, 14), [Some(Mood::Happy)]);
        assert_eq!(moods(95, 7), [Some(Mood::Hearts)]);
        assert_eq!(moods(95, 6), [Some(Mood::Smoke)]);
        assert_eq!(moods(100, 7), [Some(Mood::Hearts)]);
        assert_eq!(moods(100, 6), [Some(Mood::Smoke)]);
        assert_eq!(moods(98, 7), [Some(Mood::Hearts)]);
        assert_eq!(moods(98, 6), [Some(Mood::Smoke)]);
        assert_eq!(moods(91, 18), [Some(Mood::Hearts)]);
        let mut zombie = mob(54);
        zombie.apply(EntityEvent::Status { id: 1, status: 20 });
        match zombie.take_fx().as_slice() {
            [Fx::Death { pos, size }] => {
                assert_eq!(*pos, [0.0; 3]);
                assert_eq!(*size, (0.6, 1.95));
            }
            other => panic!("expected one Death fx, got {}", other.len()),
        }
        assert!(moods(54, 12).is_empty(), "zombie is no villager");
        assert!(moods(120, 18).is_empty(), "villager is no animal");
        let mut witch = mob(66);
        witch.apply(EntityEvent::Status { id: 1, status: 15 });
        assert!(matches!(witch.take_fx().as_slice(), [Fx::Witch { .. }]));
    }

    #[test]
    fn remote_swing_length_follows_haste_and_mining_fatigue() {
        let effect = |id| crate::effects::Effect {
            id,
            amplifier: if id == crate::effects::id::HASTE {
                1
            } else {
                0
            },
            duration: 600,
            permanent: false,
            ambient: false,
            particles: true,
        };
        let length = |id: Option<i8>| {
            let mut es = Entities::default();
            spawn(&mut es, 3, [0; 3]);
            if let Some(id) = id {
                es.apply(EntityEvent::Effect {
                    id: 3,
                    effect: effect(id),
                });
            }
            es.apply(EntityEvent::Animation {
                id: 3,
                animation: 0,
            });
            (1..).find(|_| {
                es.tick();
                !es.get(3).unwrap().swinging
            })
        };
        assert_eq!(length(None), Some(7));
        assert_eq!(length(Some(crate::effects::id::HASTE)), Some(5));
        assert_eq!(length(Some(crate::effects::id::MINING_FATIGUE)), Some(9));
    }

    #[test]
    fn crit_animation_bursts_three_ticks_and_death_puffs_once() {
        let mut es = Entities::default();
        spawn(&mut es, 3, [0; 3]);
        es.apply(EntityEvent::Animation {
            id: 3,
            animation: 4,
        });
        es.tick();
        es.tick();
        es.tick();
        let hits = es
            .take_fx()
            .iter()
            .filter(|f| matches!(f, Fx::Hit { magic: false, .. }))
            .count();
        assert_eq!(hits, 3);
        es.apply(EntityEvent::Status { id: 3, status: 2 });
        let e = es.get(3).unwrap();
        assert_eq!((e.hurt_time, e.limb_swing_amount), (10, 1.5));
        es.apply(EntityEvent::Status { id: 3, status: 3 });
        for _ in 0..DEATH_TICKS {
            es.tick();
        }
        assert!(matches!(es.take_fx().as_slice(), [Fx::Death { .. }]));
    }

    #[test]
    fn eat_grass_status_runs_the_sheep_timer_down_and_ignores_other_mobs() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        es.with(1, |e| e.kind = Kind::Mob(91));
        spawn(&mut es, 2, [0; 3]);
        for id in [1, 2] {
            es.apply(EntityEvent::Status { id, status: 10 });
        }
        assert_eq!(es.get(1).unwrap().sheep_timer, 40);
        assert_eq!(es.get(2).unwrap().sheep_timer, 0);
        for _ in 0..45 {
            es.tick();
        }
        assert_eq!(es.get(1).unwrap().sheep_timer, 0);
    }

    fn status_of(kind: u8, status: i8) -> Entities {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        es.with(1, |e| e.kind = Kind::Mob(kind));
        es.apply(EntityEvent::Status { id: 1, status });
        es
    }

    #[test]
    fn golem_attack_status_sets_ten_and_counts_down_for_golems_only() {
        let mut es = status_of(99, 4);
        assert_eq!(es.get(1).unwrap().anim.golem_attack, 10);
        es.tick();
        assert_eq!(es.get(1).unwrap().anim.golem_attack, 9);
        assert_eq!(status_of(95, 4).get(1).unwrap().anim.golem_attack, 0);
    }

    #[test]
    fn golem_rose_status_sets_400_and_counts_down() {
        let mut es = status_of(99, 11);
        assert_eq!(es.get(1).unwrap().anim.golem_rose, 400);
        for _ in 0..401 {
            es.tick();
        }
        assert_eq!(es.get(1).unwrap().anim.golem_rose, 0);
    }

    #[test]
    fn wolf_shake_status_shakes_about_forty_ticks_then_stops() {
        let mut es = status_of(95, 8);
        assert!(es.get(1).unwrap().anim.shaking);
        es.tick();
        let a = es.get(1).unwrap().anim;
        assert_eq!((a.prev_shake_time, a.shake_time), (0.0, 0.05));
        for _ in 0..30 {
            es.tick();
        }
        assert!(es.get(1).unwrap().anim.shaking);
        for _ in 0..15 {
            es.tick();
        }
        assert!(!es.get(1).unwrap().anim.shaking);
        assert_eq!(es.get(1).unwrap().anim.shake_time, 0.0);
        assert!(!status_of(99, 8).get(1).unwrap().anim.shaking);
    }

    #[test]
    fn rain_at_the_wolfs_head_wets_it() {
        let mut es = status_of(95, 8);
        es.with(1, |e| e.pos[1] = 0.5);
        let (y, h) = (0.5, f64::from(Kind::Mob(95).size().1));
        let head = (y + h).floor() as i32;
        let feet = y.floor() as i32;
        assert_ne!(head, feet);
        es.update_wet(|_, _, _| 0, |_, by, _| by == head);
        assert!(es.get(1).unwrap().anim.in_water);
        es.update_wet(|_, _, _| 0, |_, _, _| false);
        assert!(!es.get(1).unwrap().anim.in_water);
    }

    #[test]
    fn boats_minecarts_tnt_crystals_and_dying_mobs_block_placement() {
        for kind in [1, 10, 11, 12, 50, 51, 70, 78] {
            let e = Entity::new(1, Kind::Object(kind), [0; 3], 0, 0);
            assert!(e.blocks_placement(), "{kind}");
        }
        for kind in [2, 60, 71] {
            let e = Entity::new(1, Kind::Object(kind), [0; 3], 0, 0);
            assert!(!e.blocks_placement(), "{kind}");
        }
        assert!(!Entity::new(1, Kind::ExperienceOrb, [0; 3], 0, 0).blocks_placement());
        let (lo, hi) = Entity::new(1, Kind::Object(51), [0; 3], 0, 0).bounds();
        assert_eq!((hi[0] - lo[0], hi[1] - lo[1]), (2.0, 2.0));
        let mut es = Entities::default();
        spawn(&mut es, 3, [0; 3]);
        es.apply(EntityEvent::Status { id: 3, status: 3 });
        es.tick();
        assert!(es.get(3).unwrap().is_dead());
        assert!(es.get(3).unwrap().blocks_placement());
    }

    #[test]
    fn destroying_a_thrown_entity_makes_no_particles() {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnObject {
            id: 7,
            kind: 65,
            pos: [32, 64, 96],
            yaw: 0,
            pitch: 0,
            vel: [0; 3],
            data: 0,
        });
        es.apply(EntityEvent::Destroy(vec![7]));
        assert!(es.take_fx().is_empty());
        assert!(es.get(7).is_none());
    }

    #[test]
    fn an_armor_stand_hitbox_follows_its_status_byte() {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnObject {
            id: 5,
            kind: 78,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            vel: [0; 3],
            data: 0,
        });
        let size = |es: &Entities| es.get(5).unwrap().size();
        assert_eq!(size(&es), (0.5, 1.975));
        // The small bit (1) leaves the 0.5 x 1.975 box: `EntityArmorStand` sizes only a marker.
        for (status, want) in [
            (1, (0.5, 1.975)),
            (stand::MARKER, (0.0, 0.0)),
            (stand::MARKER | 1, (0.0, 0.0)),
            (4 | 8, (0.5, 1.975)),
        ] {
            es.apply(EntityEvent::Metadata {
                id: 5,
                flags: None,
                name: None,
                health: None,
                stack: None,
                size: SizeMeta {
                    stand: Some(status),
                    ..SizeMeta::default()
                },
            });
            assert_eq!(size(&es), want, "{status}");
        }
    }

    #[test]
    fn a_ray_passes_through_a_marker_armor_stand() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0, 0, 5 * 32]);
        es.apply(EntityEvent::SpawnObject {
            id: 5,
            kind: 78,
            pos: [0, 0, 3 * 32],
            yaw: 0,
            pitch: 0,
            vel: [0; 3],
            data: 0,
        });
        let set = |es: &mut Entities, status| {
            es.apply(EntityEvent::Metadata {
                id: 5,
                flags: None,
                name: None,
                health: None,
                stack: None,
                size: SizeMeta {
                    stand: Some(status),
                    ..SizeMeta::default()
                },
            });
        };
        let (eye, dir) = ([0.0, 0.9, 0.0], look_vector(0.0, 0.0));
        let nearest = |es: &Entities| es.pick(eye, dir, 6.0, 0, None, None).map(|h| h.0);
        // A plain stand blocks the ray; a marker stand does not (`hasCollision` is false).
        assert_eq!(nearest(&es), Some(5));
        set(&mut es, stand::MARKER);
        assert_eq!(nearest(&es), Some(1));
        assert_eq!(
            es.hit_along([0.0, 0.9, 0.0], [0.0, 0.9, 4.0], false, None),
            None
        );
    }

    #[test]
    fn baby_mobs_and_slimes_size_themselves_from_metadata() {
        let mut es = Entities::default();
        let mut mob = |id: i32, kind: u8, size: SizeMeta| {
            es.apply(EntityEvent::SpawnMob {
                id,
                kind,
                pos: [0; 3],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size,
            });
            es.get(id).unwrap().size()
        };
        let age = |a: u8| SizeMeta {
            age: Some(a),
            ..SizeMeta::default()
        };
        let slime = |s: u8| SizeMeta {
            slime: Some(s),
            ..SizeMeta::default()
        };
        assert_eq!(mob(1, 90, age(0)), (0.9, 0.9));
        assert_eq!(mob(2, 90, age(255)), (0.45, 0.45));
        assert_eq!(mob(3, 54, age(1)), (0.3, 0.975));
        assert_eq!(mob(4, 54, age(0)), (0.6, 1.95));
        assert_eq!(mob(5, 55, slime(1)), (0.51000005, 0.51000005));
        assert_eq!(mob(6, 62, slime(4)), (0.51000005 * 4.0, 0.51000005 * 4.0));
        assert_eq!(mob(7, 50, age(255)), (0.6, 1.7));
        assert_eq!(mob(8, 100, age(255)), (0.7, 0.8));
        assert_eq!(mob(9, 51, age(0)), (0.6, 1.95));
    }

    #[test]
    fn pick_takes_nearest_hit_and_respects_reach_and_blocks() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0, 0, 5 * 32]);
        spawn(&mut es, 2, [0, 0, 3 * 32]);
        let eye = [0.0, 0.9, 0.0];
        let dir = look_vector(0.0, 0.0);
        assert_eq!(es.pick(eye, dir, 6.0, 0, None, None).map(|h| h.0), Some(2));
        assert_eq!(es.pick(eye, dir, 2.0, 0, None, None), None);
        assert_eq!(es.pick(eye, dir, 6.0, 2, None, Some(4.0)), None);
        assert_eq!(es.pick(eye, dir, 6.0, 2, None, None).map(|h| h.0), Some(1));
        assert_eq!(
            es.pick(eye, look_vector(90.0, 0.0), 6.0, 0, None, None),
            None
        );
        assert_eq!(
            es.pick(eye, dir, 6.0, 0, Some(2), None).map(|h| h.0),
            Some(1)
        );
        assert_eq!(
            es.pick(eye, dir, 6.0, 1, Some(2), None).map(|h| h.0),
            Some(2)
        );
    }

    #[test]
    fn spawned_player_takes_name_from_earlier_player_list() {
        let mut es = Entities::default();
        es.apply(EntityEvent::PlayerName {
            uuid: 9,
            name: "Alex".into(),
            textures: Some("dGV4".into()),
        });
        es.apply(EntityEvent::SpawnPlayer {
            id: 4,
            uuid: 9,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            held: 276,
            flags: Some(flag::SNEAKING),
        });
        let e = es.get(4).unwrap();
        assert_eq!(e.name.as_deref(), Some("Alex"));
        assert_eq!(e.textures.as_deref(), Some("dGV4"));
        assert!(e.sneaking());
        assert_eq!(e.equipment[0].unwrap().id, 276);
    }

    #[test]
    fn use_ticks_count_from_flag_and_drive_bow_stage() {
        let mut es = Entities::default();
        es.apply(EntityEvent::SpawnPlayer {
            id: 4,
            uuid: 9,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            held: 261,
            flags: Some(flag::USING_ITEM),
        });
        let stage = |es: &Entities| {
            let t = es.get(4).unwrap().use_ticks.unwrap();
            crate::inventory::items::held_variant(261, 0, t as i32, false)
        };
        es.tick();
        assert_eq!(es.get(4).unwrap().use_ticks, Some(0));
        assert_eq!(stage(&es), 0);
        for _ in 0..14 {
            es.tick();
        }
        assert_eq!(es.get(4).unwrap().use_ticks, Some(14));
        assert_eq!(stage(&es), -2);
        for _ in 0..4 {
            es.tick();
        }
        assert_eq!(stage(&es), -3);
        es.apply(EntityEvent::Metadata {
            id: 4,
            flags: Some(0),
            name: None,
            health: None,
            stack: None,
            size: SizeMeta::default(),
        });
        es.tick();
        assert_eq!(es.get(4).unwrap().use_ticks, None);
    }

    #[test]
    fn fire_flags_respect_immunity_and_blaze_activity() {
        let mut entity = Entity::new(1, Kind::Mob(54), [0; 3], 0, 0);
        entity.flags = flag::BURNING | flag::INVISIBLE;
        assert!(entity.burning());
        entity.flags = 0;
        assert!(!entity.burning());
        // 1.8.9 `isImmuneToFire` overrides: EntityGhast 56, EntityPigZombie 57, EntityMagmaCube 62,
        // EntityDragon 63, EntityWither 64, EntityTNTPrimed (object 50). Every other id burns;
        // EntityBlaze 61 is driven by metadata and EntitySkeleton 51 by its type, both below.
        for id in 0..=u8::MAX {
            for kind in [Kind::Mob(id), Kind::Object(id as i8)] {
                if matches!(kind, Kind::Mob(51 | 61)) {
                    continue;
                }
                entity.kind = kind;
                entity.flags = flag::BURNING;
                let immune = matches!(kind, Kind::Mob(56 | 57 | 62 | 63 | 64) | Kind::Object(50));
                assert_eq!(entity.burning(), !immune, "{kind:?}");
            }
        }
        entity.kind = Kind::Mob(51);
        entity.flags = flag::BURNING;
        entity.meta.skeleton = Some(0);
        assert!(entity.burning());
        entity.meta.skeleton = Some(1);
        assert!(!entity.burning());
        entity.kind = Kind::Mob(61);
        assert!(!entity.burning());
        entity.flags = 0;
        entity.meta.slime = Some(1);
        assert!(entity.burning());
    }

    /// Packet path of #270: a mob spawn, then ground flags from move packets, drive the squish the
    /// renderer reads. A zombie (54) must never squish.
    #[test]
    fn ground_flags_from_move_packets_squish_slimes_and_magma_cubes() {
        for (kind, squishes) in [(55u8, true), (62, true), (54, false)] {
            let mut es = Entities::default();
            es.apply(EntityEvent::SpawnMob {
                id: 1,
                kind,
                pos: [0, 64 * 32, 0],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size: SizeMeta::default(),
            });
            let mv = |es: &mut Entities, on_ground| {
                es.apply(EntityEvent::Move {
                    id: 1,
                    delta: [0; 3],
                    look: None,
                    on_ground,
                });
                es.tick();
            };
            mv(&mut es, false);
            mv(&mut es, false);
            mv(&mut es, true);
            mv(&mut es, true);
            let landed = es.get(1).unwrap().squish.factor(1.0);
            assert_eq!(landed < 0.0, squishes, "kind {kind}: {landed}");
            for _ in 0..80 {
                mv(&mut es, true);
            }
            assert!(es.get(1).unwrap().squish.factor(1.0).abs() < 1e-3);
        }
    }

    /// `handleEntityAttach` with the leash flag ties a mob to its holder and a negative id unties it; an
    /// object cannot be leashed.
    #[test]
    fn a_leash_packet_ties_a_mob_and_a_negative_holder_unties_it() {
        let mut es = Entities::default();
        es.map
            .insert(1, Entity::new(1, Kind::Mob(92), [0, 0, 0], 0, 0));
        es.map
            .insert(2, Entity::new(2, Kind::Object(1), [0, 0, 0], 0, 0));
        for id in [1, 2] {
            es.apply(EntityEvent::Leash { id, holder: 9 });
        }
        assert_eq!(es.get(1).unwrap().leash, Some(9));
        assert_eq!(es.get(2).unwrap().leash, None);
        es.apply(EntityEvent::Leash { id: 1, holder: -1 });
        assert_eq!(es.get(1).unwrap().leash, None);
    }
}
