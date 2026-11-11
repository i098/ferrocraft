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
            Kind::ExperienceOrb => (0.5, 0.5),
            Kind::Object(2) => (0.25, 0.25),
            Kind::Object(60 | 61 | 62 | 65 | 90) => (0.25, 0.25),
            Kind::Object(78) => (0.5, 1.975),
            Kind::Object(1) => (1.5, 0.6),
            Kind::Object(51) => (2.0, 2.0),
            Kind::Object(10..=12) => (0.98, 0.7),
            Kind::Object(_) => (0.98, 0.98),
            Kind::Mob(id) => mob_size(id),
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SizeMeta {
    pub stand: Option<u8>,
    pub age: Option<u8>,
    pub slime: Option<u8>,
    pub guardian: Option<i32>,
    pub horse: Option<u8>,
    pub always_name: Option<u8>,
    pub skeleton: Option<u8>,
}

impl SizeMeta {
    fn merge(&mut self, new: SizeMeta) {
        self.stand = new.stand.or(self.stand);
        self.age = new.age.or(self.age);
        self.slime = new.slime.or(self.slime);
        self.guardian = new.guardian.or(self.guardian);
        self.horse = new.horse.or(self.horse);
        self.always_name = new.always_name.or(self.always_name);
        self.skeleton = new.skeleton.or(self.skeleton);
    }
}

/// Equipment slots of `S04`: 0 held item, 1..=4 boots to helmet.
pub const EQUIPMENT_SLOTS: usize = 5;

/// A held or worn item: item id and damage, as the network sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRef {
    pub id: i16,
    pub damage: i16,
    /// Leather armor color (`display.color`).
    pub dye: Option<i32>,
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
    pub head_yaw: f32,
    pub on_ground: bool,
    pub vel: [f64; 3],
    pub flags: u8,
    /// Fixed-point (1/32 block) position the server last sent; relative moves add to it.
    server_pos: [i32; 3],
    target: Target,
    pub limb_swing: f32,
    pub limb_swing_amount: f32,
    pub prev_limb_swing_amount: f32,
    /// 0 when idle; counts up to 6 during an arm swing.
    pub swing_ticks: i32,
    pub swinging: bool,
    /// Red-flash ticks left after a hurt status (vanilla `hurtTime`, 10 at the hit).
    pub hurt_time: i32,
    /// Ticks since death status; the model tips over for 20 ticks, then the entity is removed.
    pub death_time: i32,
    /// Metadata that sets the hitbox.
    pub meta: SizeMeta,
    pub equipment: [Option<ItemRef>; EQUIPMENT_SLOTS],
    /// Item entities: the stack they show.
    pub stack: Option<ItemRef>,
    /// Entity id of the vehicle, if riding.
    pub vehicle: Option<i32>,
    pub age: u32,
    /// Metadata index 7: the mixed colour of the active potion effects, 0 for none.
    pub potion_color: i32,
    /// Fishing bobbers: id of the player who cast it (the `SpawnObject` data), 0 for any other entity.
    pub owner: i32,
    /// Metadata index 8: every active effect is ambient (beacon).
    pub potion_ambient: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Target {
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    head_yaw: f32,
    steps: u32,
}

/// Interpolation steps vanilla uses for remote entities (`newPosRotationIncrements`).
const STEPS: u32 = 3;
const DEATH_TICKS: i32 = 20;

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
            head_yaw: yaw,
            on_ground: false,
            vel: [0.0; 3],
            flags: 0,
            server_pos: raw,
            target: Target {
                pos,
                yaw,
                pitch,
                head_yaw: yaw,
                steps: 0,
            },
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            prev_limb_swing_amount: 0.0,
            swing_ticks: 0,
            swinging: false,
            hurt_time: 0,
            death_time: 0,
            meta: SizeMeta::default(),
            equipment: [None; EQUIPMENT_SLOTS],
            stack: None,
            vehicle: None,
            age: 0,
            potion_color: 0,
            potion_ambient: false,
            owner: 0,
        }
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
            Kind::ExperienceOrb => false,
        }
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

    /// `Entity.getEyeHeight`: players 1.62 (1.54 sneaking), armor stands 0.9 of the height (0.5 when small), else 0.85.
    pub fn eye_height(&self) -> f32 {
        match self.kind {
            Kind::Player => 1.62 - if self.sneaking() { 0.08 } else { 0.0 },
            Kind::Object(78) => self.size().1 * if self.is_child() { 0.5 } else { 0.9 },
            _ => self.size().1 * 0.85,
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
        bounds_at(self.pos, self.size())
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
        self.target.steps = STEPS;
    }

    /// One client tick: interpolation toward the server pose, limb swing, swing and hurt timers.
    fn tick(&mut self) {
        self.age += 1;
        self.prev_pos = self.pos;
        if self.target.steps > 0 {
            let n = f64::from(self.target.steps);
            for i in 0..3 {
                self.pos[i] += (self.target.pos[i] - self.pos[i]) / n;
            }
            let k = self.target.steps as f32;
            self.yaw += wrap(self.target.yaw - self.yaw) / k;
            self.pitch += (self.target.pitch - self.pitch) / k;
            self.head_yaw += wrap(self.target.head_yaw - self.head_yaw) / k;
            self.target.steps -= 1;
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
            if self.swing_ticks >= 6 {
                self.swing_ticks = 0;
                self.swinging = false;
            }
        } else {
            self.swing_ticks = 0;
        }
        if self.hurt_time > 0 {
            self.hurt_time -= 1;
        }
        if self.death_time > 0 {
            self.death_time += 1;
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
            0 if !self.swinging || self.swing_ticks >= 3 || self.swing_ticks < 0 => {
                self.swing_ticks = -1;
                self.swinging = true;
            }
            1 => self.hurt_time = 10,
            _ => {}
        }
    }

    fn set_status(&mut self, status: i8) {
        match status {
            2 => self.hurt_time = 10,
            3 if self.death_time == 0 => {
                self.hurt_time = 10;
                self.death_time = 1;
            }
            _ => {}
        }
    }

    /// Arm swing progress in `0.0..1.0`, interpolated between ticks (`getSwingProgress`).
    pub fn swing_progress(&self, partial: f32) -> f32 {
        if !self.swinging {
            return 0.0;
        }
        (self.swing_ticks as f32 + partial) / 6.0
    }

    /// Position between the previous and current tick.
    pub fn lerp_pos(&self, partial: f64) -> [f64; 3] {
        std::array::from_fn(|i| self.prev_pos[i] + (self.pos[i] - self.prev_pos[i]) * partial)
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
    /// `S0D` collect item: the entity flies to the collector and is picked up.
    Pickup {
        id: i32,
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
            E::SpawnExperienceOrb { id, pos } => {
                let e = Entity::new(id, Kind::ExperienceOrb, pos, 0, 0);
                self.map.insert(id, e);
            }
            E::Move {
                id,
                delta,
                look,
                on_ground,
            } => self.with(id, |e| {
                let raw = std::array::from_fn(|i| e.server_pos[i] + i32::from(delta[i]));
                let (yaw, pitch) = look.map_or((e.target.yaw, e.target.pitch), |(y, p)| {
                    (angle(y), angle(p))
                });
                e.set_target(raw, yaw, pitch);
                e.on_ground = on_ground;
            }),
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
            } => self.with(id, |e| {
                e.set_target(pos, angle(yaw), angle(pitch));
                e.on_ground = on_ground;
            }),
            E::HeadLook { id, head_yaw } => self.with(id, |e| {
                e.target.head_yaw = angle(head_yaw);
                e.target.steps = e.target.steps.max(STEPS);
            }),
            E::Velocity { id, vel } => self.with(id, |e| e.vel = velocity(vel)),
            E::Destroy(ids) => ids.iter().for_each(|id| {
                self.map.remove(id);
            }),
            E::Clear => self.map.clear(),
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
                        dye: None,
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
                e.target.head_yaw = e.head_yaw;
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
                e.vel = velocity(vel);
                if kind == 90 {
                    e.owner = data;
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
            E::Animation { id, animation } => self.with(id, |e| e.animate(animation)),
            E::Status { id, status } => self.with(id, |e| e.set_status(status)),
            E::Attach { id, vehicle } => {
                self.with(id, |e| e.vehicle = (vehicle >= 0).then_some(vehicle));
            }
            E::PlayerName {
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
            E::PlayerRemove { uuid } => {
                self.names.remove(&uuid);
            }
            _ => {}
        }
    }

    /// One 20 TPS step for every tracked entity; dead ones leave after their death animation.
    pub fn tick(&mut self) {
        for e in self.map.values_mut() {
            e.tick();
        }
        self.map.retain(|_, e| e.death_time <= DEATH_TICKS);
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
            .filter(|e| {
                !matches!(
                    e.kind,
                    Kind::ExperienceOrb
                        | Kind::Object(2 | 60 | 61 | 62 | 64 | 65 | 66 | 72 | 73 | 75 | 76 | 90)
                )
            })
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
    use super::*;

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

    #[test]
    fn yaw_interpolates_the_short_way_around() {
        let mut es = Entities::default();
        spawn(&mut es, 1, [0; 3]);
        let mut e = es.get(1).unwrap().clone();
        e.yaw = 170.0;
        e.target.yaw = angle(-127);
        e.target.steps = STEPS;
        e.tick();
        assert!(e.yaw > 170.0, "went the long way: {}", e.yaw);
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
}
