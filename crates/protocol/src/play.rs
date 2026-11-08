//! Play-state packets for protocol 47.
//!
//! Entity coordinates in 1.8.9 are fixed-point `i32` (divide by 32) and angles are
//! `i8` steps of 1/256 turn; they are kept raw so decode then encode is byte-exact.
//! Chat components are JSON strings.

use crate::codec::*;
use crate::nbt::Nbt;

type Chat = String;

wire_struct!(ChunkMeta {
    x: i32,
    z: i32,
    bitmask: u16
});
wire_struct!(BlockRecord {
    horizontal_pos: u8,
    y: u8,
    block_id: VarInt
});
wire_struct!(Offset {
    x: i8,
    y: i8,
    z: i8
});
wire_struct!(Modifier {
    uuid: Uuid,
    amount: f64,
    operation: i8
});
wire_struct!(Attribute { key: String, value: f64, modifiers: Vec<Modifier> });
wire_struct!(Stat {
    name: String,
    value: VarInt
});
wire_struct!(MapIcon {
    dir_type: i8,
    x: i8,
    z: i8
});
wire_struct!(MapUpdate {
    rows: u8,
    x: u8,
    z: u8,
    data: Bytes
});
wire_struct!(ProfileProperty { name: String, value: String, signature: Opt<String> });
wire_struct!(PlayerListAdd {
    name: String,
    properties: Vec<ProfileProperty>,
    gamemode: VarInt,
    ping: VarInt,
    display_name: Opt<Chat>,
});
wire_struct!(ObjectiveData {
    value: String,
    kind: String
});
wire_struct!(TeamInfo {
    display_name: String,
    prefix: String,
    suffix: String,
    friendly_fire: i8,
    name_tag_visibility: String,
    color: i8,
});

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok((A::read(r)?, B::read(r)?))
    }
    fn write(&self, w: &mut Vec<u8>) {
        self.0.write(w);
        self.1.write(w);
    }
}

fn invalid<T>(what: &'static str) -> Result<T> {
    Err(Error::InvalidValue(what))
}

/// Player list update; the variant decides the action id.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerListAction {
    Add(Vec<(Uuid, PlayerListAdd)>),
    Gamemode(Vec<(Uuid, VarInt)>),
    Latency(Vec<(Uuid, VarInt)>),
    DisplayName(Vec<(Uuid, Opt<Chat>)>),
    Remove(Vec<Uuid>),
}

impl Wire for PlayerListAction {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match VarInt::read(r)?.0 {
            0 => Self::Add(Wire::read(r)?),
            1 => Self::Gamemode(Wire::read(r)?),
            2 => Self::Latency(Wire::read(r)?),
            3 => Self::DisplayName(Wire::read(r)?),
            4 => Self::Remove(Wire::read(r)?),
            _ => return invalid("player list action"),
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Self::Add(v) => tagged(0, v, w),
            Self::Gamemode(v) => tagged(1, v, w),
            Self::Latency(v) => tagged(2, v, w),
            Self::DisplayName(v) => tagged(3, v, w),
            Self::Remove(v) => tagged(4, v, w),
        }
    }
}

/// Writes an action id followed by one body value.
fn tagged<T: Wire>(id: i32, body: &T, w: &mut Vec<u8>) {
    VarInt(id).write(w);
    body.write(w);
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldBorderAction {
    SetSize {
        diameter: f64,
    },
    LerpSize {
        old_diameter: f64,
        new_diameter: f64,
        speed: VarLong,
    },
    SetCenter {
        x: f64,
        z: f64,
    },
    Initialize {
        x: f64,
        z: f64,
        old_diameter: f64,
        new_diameter: f64,
        speed: VarLong,
        portal_teleport_boundary: VarInt,
        warning_time: VarInt,
        warning_blocks: VarInt,
    },
    SetWarningTime {
        seconds: VarInt,
    },
    SetWarningBlocks {
        blocks: VarInt,
    },
}

impl Wire for WorldBorderAction {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match VarInt::read(r)?.0 {
            0 => Self::SetSize {
                diameter: Wire::read(r)?,
            },
            1 => Self::LerpSize {
                old_diameter: Wire::read(r)?,
                new_diameter: Wire::read(r)?,
                speed: Wire::read(r)?,
            },
            2 => Self::SetCenter {
                x: Wire::read(r)?,
                z: Wire::read(r)?,
            },
            3 => Self::Initialize {
                x: Wire::read(r)?,
                z: Wire::read(r)?,
                old_diameter: Wire::read(r)?,
                new_diameter: Wire::read(r)?,
                speed: Wire::read(r)?,
                portal_teleport_boundary: Wire::read(r)?,
                warning_time: Wire::read(r)?,
                warning_blocks: Wire::read(r)?,
            },
            4 => Self::SetWarningTime {
                seconds: Wire::read(r)?,
            },
            5 => Self::SetWarningBlocks {
                blocks: Wire::read(r)?,
            },
            _ => return invalid("world border action"),
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Self::SetSize { diameter } => tagged(0, diameter, w),
            Self::LerpSize {
                old_diameter,
                new_diameter,
                speed,
            } => {
                VarInt(1).write(w);
                old_diameter.write(w);
                new_diameter.write(w);
                speed.write(w);
            }
            Self::SetCenter { x, z } => {
                VarInt(2).write(w);
                x.write(w);
                z.write(w);
            }
            Self::Initialize {
                x,
                z,
                old_diameter,
                new_diameter,
                speed,
                portal_teleport_boundary,
                warning_time,
                warning_blocks,
            } => {
                VarInt(3).write(w);
                x.write(w);
                z.write(w);
                old_diameter.write(w);
                new_diameter.write(w);
                speed.write(w);
                portal_teleport_boundary.write(w);
                warning_time.write(w);
                warning_blocks.write(w);
            }
            Self::SetWarningTime { seconds } => tagged(4, seconds, w),
            Self::SetWarningBlocks { blocks } => tagged(5, blocks, w),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TitleAction {
    SetTitle(Chat),
    SetSubtitle(Chat),
    SetTimes {
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    Hide,
    Reset,
}

impl Wire for TitleAction {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match VarInt::read(r)?.0 {
            0 => Self::SetTitle(Wire::read(r)?),
            1 => Self::SetSubtitle(Wire::read(r)?),
            2 => Self::SetTimes {
                fade_in: Wire::read(r)?,
                stay: Wire::read(r)?,
                fade_out: Wire::read(r)?,
            },
            3 => Self::Hide,
            4 => Self::Reset,
            _ => return invalid("title action"),
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Self::SetTitle(t) => tagged(0, t, w),
            Self::SetSubtitle(t) => tagged(1, t, w),
            Self::SetTimes {
                fade_in,
                stay,
                fade_out,
            } => {
                VarInt(2).write(w);
                fade_in.write(w);
                stay.write(w);
                fade_out.write(w);
            }
            Self::Hide => VarInt(3).write(w),
            Self::Reset => VarInt(4).write(w),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatEventKind {
    Enter,
    End {
        duration: VarInt,
        entity_id: i32,
    },
    EntityDead {
        player_id: VarInt,
        entity_id: i32,
        message: Chat,
    },
}

impl Wire for CombatEventKind {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match VarInt::read(r)?.0 {
            0 => Self::Enter,
            1 => Self::End {
                duration: Wire::read(r)?,
                entity_id: Wire::read(r)?,
            },
            2 => Self::EntityDead {
                player_id: Wire::read(r)?,
                entity_id: Wire::read(r)?,
                message: Wire::read(r)?,
            },
            _ => return invalid("combat event"),
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Self::Enter => VarInt(0).write(w),
            Self::End {
                duration,
                entity_id,
            } => {
                VarInt(1).write(w);
                duration.write(w);
                entity_id.write(w);
            }
            Self::EntityDead {
                player_id,
                entity_id,
                message,
            } => {
                VarInt(2).write(w);
                player_id.write(w);
                entity_id.write(w);
                message.write(w);
            }
        }
    }
}

/// Entity interaction of a `UseEntity` packet.
#[derive(Debug, Clone, PartialEq)]
pub enum UseAction {
    Interact,
    Attack,
    InteractAt { x: f32, y: f32, z: f32 },
}

impl Wire for UseAction {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match VarInt::read(r)?.0 {
            0 => Self::Interact,
            1 => Self::Attack,
            2 => Self::InteractAt {
                x: Wire::read(r)?,
                y: Wire::read(r)?,
                z: Wire::read(r)?,
            },
            _ => return invalid("use entity action"),
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Self::Interact => VarInt(0).write(w),
            Self::Attack => VarInt(1).write(w),
            Self::InteractAt { x, y, z } => {
                VarInt(2).write(w);
                x.write(w);
                y.write(w);
                z.write(w);
            }
        }
    }
}

pub mod clientbound {
    use super::*;

    packet!(KeepAlive = 0x00 { id: VarInt });
    packet!(JoinGame = 0x01 {
        entity_id: i32,
        gamemode: u8,
        dimension: i8,
        difficulty: u8,
        max_players: u8,
        level_type: String,
        reduced_debug_info: bool,
    });
    // `position`: 0 chat, 1 system message, 2 above hotbar.
    packet!(ChatMessage = 0x02 { json: Chat, position: i8 });
    packet!(TimeUpdate = 0x03 { world_age: i64, time_of_day: i64 });
    packet!(EntityEquipment = 0x04 { entity_id: VarInt, slot: i16, item: Slot });
    packet!(SpawnPosition = 0x05 { location: Position });
    packet!(UpdateHealth = 0x06 { health: f32, food: VarInt, saturation: f32 });
    packet!(Respawn = 0x07 { dimension: i32, difficulty: u8, gamemode: u8, level_type: String });
    packet!(PlayerPositionAndLook = 0x08 {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u8,
    });
    packet!(HeldItemChange = 0x09 { slot: i8 });
    packet!(UseBed = 0x0A { entity_id: VarInt, location: Position });
    packet!(Animation = 0x0B { entity_id: VarInt, animation: u8 });
    packet!(SpawnPlayer = 0x0C {
        entity_id: VarInt,
        uuid: Uuid,
        x: i32,
        y: i32,
        z: i32,
        yaw: i8,
        pitch: i8,
        current_item: i16,
        metadata: Metadata,
    });
    packet!(CollectItem = 0x0D { collected: VarInt, collector: VarInt });

    /// Velocity is present on the wire only when `data` is greater than zero.
    #[derive(Debug, Clone, PartialEq)]
    pub struct SpawnObject {
        pub entity_id: VarInt,
        pub kind: i8,
        pub x: i32,
        pub y: i32,
        pub z: i32,
        pub pitch: i8,
        pub yaw: i8,
        pub data: i32,
        pub velocity: Option<[i16; 3]>,
    }
    impl Wire for SpawnObject {
        fn read(r: &mut Reader) -> Result<Self> {
            let mut p = SpawnObject {
                entity_id: Wire::read(r)?,
                kind: Wire::read(r)?,
                x: Wire::read(r)?,
                y: Wire::read(r)?,
                z: Wire::read(r)?,
                pitch: Wire::read(r)?,
                yaw: Wire::read(r)?,
                data: Wire::read(r)?,
                velocity: None,
            };
            if p.data > 0 {
                p.velocity = Some(Wire::read(r)?);
            }
            Ok(p)
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.entity_id.write(w);
            self.kind.write(w);
            self.x.write(w);
            self.y.write(w);
            self.z.write(w);
            self.pitch.write(w);
            self.yaw.write(w);
            self.data.write(w);
            if let (true, Some(v)) = (self.data > 0, &self.velocity) {
                v.write(w);
            }
        }
    }
    packet_id!(SpawnObject = 0x0E);

    packet!(SpawnMob = 0x0F {
        entity_id: VarInt,
        kind: u8,
        x: i32,
        y: i32,
        z: i32,
        yaw: i8,
        pitch: i8,
        head_pitch: i8,
        velocity_x: i16,
        velocity_y: i16,
        velocity_z: i16,
        metadata: Metadata,
    });
    packet!(SpawnPainting = 0x10 {
        entity_id: VarInt,
        title: String,
        location: Position,
        direction: u8,
    });
    packet!(SpawnExperienceOrb = 0x11 { entity_id: VarInt, x: i32, y: i32, z: i32, count: i16 });
    packet!(EntityVelocity = 0x12 { entity_id: VarInt, x: i16, y: i16, z: i16 });
    packet!(DestroyEntities = 0x13 { entity_ids: Vec<VarInt> });
    packet!(Entity = 0x14 { entity_id: VarInt });
    packet!(EntityRelativeMove = 0x15 {
        entity_id: VarInt,
        dx: i8,
        dy: i8,
        dz: i8,
        on_ground: bool,
    });
    packet!(EntityLook = 0x16 { entity_id: VarInt, yaw: i8, pitch: i8, on_ground: bool });
    packet!(EntityLookAndRelativeMove = 0x17 {
        entity_id: VarInt,
        dx: i8,
        dy: i8,
        dz: i8,
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    });
    packet!(EntityTeleport = 0x18 {
        entity_id: VarInt,
        x: i32,
        y: i32,
        z: i32,
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    });
    packet!(EntityHeadLook = 0x19 { entity_id: VarInt, head_yaw: i8 });
    packet!(EntityStatus = 0x1A { entity_id: i32, status: i8 });
    packet!(AttachEntity = 0x1B { entity_id: i32, vehicle_id: i32, leash: bool });
    packet!(EntityMetadata = 0x1C { entity_id: VarInt, metadata: Metadata });
    packet!(EntityEffect = 0x1D {
        entity_id: VarInt,
        effect_id: i8,
        amplifier: i8,
        duration: VarInt,
        hide_particles: bool,
    });
    packet!(RemoveEntityEffect = 0x1E { entity_id: VarInt, effect_id: i8 });
    packet!(SetExperience = 0x1F { bar: f32, level: VarInt, total: VarInt });
    packet!(EntityProperties = 0x20 { entity_id: VarInt, attributes: Vec32<Attribute> });
    packet!(ChunkData = 0x21 {
        x: i32,
        z: i32,
        continuous: bool,
        bitmask: u16,
        data: Bytes,
    });
    packet!(MultiBlockChange = 0x22 { chunk_x: i32, chunk_z: i32, records: Vec<BlockRecord> });
    packet!(BlockChange = 0x23 { location: Position, block_id: VarInt });
    packet!(BlockAction = 0x24 {
        location: Position,
        byte1: u8,
        byte2: u8,
        block_type: VarInt,
    });
    packet!(BlockBreakAnimation = 0x25 {
        entity_id: VarInt,
        location: Position,
        destroy_stage: i8,
    });
    // `data` holds every chunk's section data back to back.
    packet!(MapChunkBulk = 0x26 { sky_light: bool, chunks: Vec<ChunkMeta>, data: Rest });
    packet!(Explosion = 0x27 {
        x: f32,
        y: f32,
        z: f32,
        radius: f32,
        records: Vec32<Offset>,
        velocity_x: f32,
        velocity_y: f32,
        velocity_z: f32,
    });
    packet!(Effect = 0x28 {
        effect_id: i32,
        location: Position,
        data: i32,
        disable_relative_volume: bool,
    });
    packet!(SoundEffect = 0x29 {
        name: String,
        x: i32,
        y: i32,
        z: i32,
        volume: f32,
        pitch: u8,
    });

    /// `data` has 2 entries for particle 36, 1 entry for 37 and 38, none otherwise.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Particle {
        pub id: i32,
        pub long_distance: bool,
        pub x: f32,
        pub y: f32,
        pub z: f32,
        pub offset_x: f32,
        pub offset_y: f32,
        pub offset_z: f32,
        pub speed: f32,
        pub count: i32,
        pub data: Vec<VarInt>,
    }
    impl Wire for Particle {
        fn read(r: &mut Reader) -> Result<Self> {
            let id = i32::read(r)?;
            let mut p = Particle {
                id,
                long_distance: Wire::read(r)?,
                x: Wire::read(r)?,
                y: Wire::read(r)?,
                z: Wire::read(r)?,
                offset_x: Wire::read(r)?,
                offset_y: Wire::read(r)?,
                offset_z: Wire::read(r)?,
                speed: Wire::read(r)?,
                count: Wire::read(r)?,
                data: Vec::new(),
            };
            for _ in 0..Self::data_len(id) {
                p.data.push(Wire::read(r)?);
            }
            Ok(p)
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.id.write(w);
            self.long_distance.write(w);
            for v in [
                self.x,
                self.y,
                self.z,
                self.offset_x,
                self.offset_y,
                self.offset_z,
                self.speed,
            ] {
                v.write(w);
            }
            self.count.write(w);
            self.data
                .iter()
                .take(Self::data_len(self.id))
                .for_each(|v| v.write(w));
        }
    }
    impl Particle {
        fn data_len(id: i32) -> usize {
            match id {
                36 => 2,
                37 | 38 => 1,
                _ => 0,
            }
        }
    }
    packet_id!(Particle = 0x2A);

    packet!(ChangeGameState = 0x2B { reason: u8, value: f32 });
    packet!(SpawnGlobalEntity = 0x2C { entity_id: VarInt, kind: i8, x: i32, y: i32, z: i32 });

    /// `horse_entity_id` is present only when `window_type` is `EntityHorse`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct OpenWindow {
        pub window_id: u8,
        pub window_type: String,
        pub title: Chat,
        pub slots: u8,
        pub horse_entity_id: Option<i32>,
    }
    impl Wire for OpenWindow {
        fn read(r: &mut Reader) -> Result<Self> {
            let mut p = OpenWindow {
                window_id: Wire::read(r)?,
                window_type: Wire::read(r)?,
                title: Wire::read(r)?,
                slots: Wire::read(r)?,
                horse_entity_id: None,
            };
            if p.window_type == "EntityHorse" {
                p.horse_entity_id = Some(Wire::read(r)?);
            }
            Ok(p)
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.window_id.write(w);
            self.window_type.write(w);
            self.title.write(w);
            self.slots.write(w);
            if let (true, Some(id)) = (self.window_type == "EntityHorse", self.horse_entity_id) {
                id.write(w);
            }
        }
    }
    packet_id!(OpenWindow = 0x2D);

    packet!(CloseWindow = 0x2E { window_id: u8 });
    packet!(SetSlot = 0x2F { window_id: i8, slot: i16, item: Slot });
    packet!(WindowItems = 0x30 { window_id: u8, slots: Vec16<Slot> });
    packet!(WindowProperty = 0x31 { window_id: u8, property: i16, value: i16 });
    packet!(ConfirmTransaction = 0x32 { window_id: u8, action: i16, accepted: bool });
    packet!(UpdateSign = 0x33 { location: Position, lines: [Chat; 4] });

    /// `update` is present only when `columns` is above zero.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Maps {
        pub item_damage: VarInt,
        pub scale: i8,
        pub icons: Vec<MapIcon>,
        pub columns: u8,
        pub update: Option<MapUpdate>,
    }
    impl Wire for Maps {
        fn read(r: &mut Reader) -> Result<Self> {
            let mut p = Maps {
                item_damage: Wire::read(r)?,
                scale: Wire::read(r)?,
                icons: Wire::read(r)?,
                columns: Wire::read(r)?,
                update: None,
            };
            if p.columns > 0 {
                p.update = Some(Wire::read(r)?);
            }
            Ok(p)
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.item_damage.write(w);
            self.scale.write(w);
            self.icons.write(w);
            self.columns.write(w);
            if let (true, Some(u)) = (self.columns > 0, &self.update) {
                u.write(w);
            }
        }
    }
    packet_id!(Maps = 0x34);

    packet!(UpdateBlockEntity = 0x35 { location: Position, action: u8, nbt: Option<Nbt> });
    packet!(SignEditorOpen = 0x36 { location: Position });
    packet!(Statistics = 0x37 { stats: Vec<Stat> });
    packet!(PlayerListItem = 0x38 { action: PlayerListAction });
    packet!(PlayerAbilities = 0x39 { flags: u8, flying_speed: f32, walking_speed: f32 });
    packet!(TabComplete = 0x3A { matches: Vec<String> });

    /// `data` is present for modes 0 (create) and 2 (update).
    #[derive(Debug, Clone, PartialEq)]
    pub struct ScoreboardObjective {
        pub name: String,
        pub mode: i8,
        pub data: Option<ObjectiveData>,
    }
    impl Wire for ScoreboardObjective {
        fn read(r: &mut Reader) -> Result<Self> {
            let name = Wire::read(r)?;
            let mode = i8::read(r)?;
            let data = if mode == 0 || mode == 2 {
                Some(Wire::read(r)?)
            } else {
                None
            };
            Ok(ScoreboardObjective { name, mode, data })
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.name.write(w);
            self.mode.write(w);
            if let (true, Some(d)) = (self.mode == 0 || self.mode == 2, &self.data) {
                d.write(w);
            }
        }
    }
    packet_id!(ScoreboardObjective = 0x3B);

    /// `value` is present unless `action` is 1 (remove).
    #[derive(Debug, Clone, PartialEq)]
    pub struct UpdateScore {
        pub name: String,
        pub action: i8,
        pub objective: String,
        pub value: Option<VarInt>,
    }
    impl Wire for UpdateScore {
        fn read(r: &mut Reader) -> Result<Self> {
            let name = Wire::read(r)?;
            let action = i8::read(r)?;
            let objective = Wire::read(r)?;
            let value = if action != 1 {
                Some(Wire::read(r)?)
            } else {
                None
            };
            Ok(UpdateScore {
                name,
                action,
                objective,
                value,
            })
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.name.write(w);
            self.action.write(w);
            self.objective.write(w);
            if let (true, Some(v)) = (self.action != 1, &self.value) {
                v.write(w);
            }
        }
    }
    packet_id!(UpdateScore = 0x3C);

    packet!(DisplayScoreboard = 0x3D { position: i8, name: String });

    /// `info` is present for modes 0 and 2; `players` for modes 0, 3 and 4.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Teams {
        pub name: String,
        pub mode: i8,
        pub info: Option<TeamInfo>,
        pub players: Option<Vec<String>>,
    }
    impl Wire for Teams {
        fn read(r: &mut Reader) -> Result<Self> {
            let name = Wire::read(r)?;
            let mode = i8::read(r)?;
            let info = if mode == 0 || mode == 2 {
                Some(Wire::read(r)?)
            } else {
                None
            };
            let players = if matches!(mode, 0 | 3 | 4) {
                Some(Wire::read(r)?)
            } else {
                None
            };
            Ok(Teams {
                name,
                mode,
                info,
                players,
            })
        }
        fn write(&self, w: &mut Vec<u8>) {
            self.name.write(w);
            self.mode.write(w);
            if let (true, Some(i)) = (self.mode == 0 || self.mode == 2, &self.info) {
                i.write(w);
            }
            if let (true, Some(p)) = (matches!(self.mode, 0 | 3 | 4), &self.players) {
                p.write(w);
            }
        }
    }
    packet_id!(Teams = 0x3E);

    packet!(PluginMessage = 0x3F { channel: String, data: Rest });
    packet!(Disconnect = 0x40 { reason: Chat });
    packet!(ServerDifficulty = 0x41 { difficulty: u8 });
    packet!(CombatEvent = 0x42 { event: CombatEventKind });
    packet!(Camera = 0x43 { camera_id: VarInt });
    packet!(WorldBorder = 0x44 { action: WorldBorderAction });
    packet!(Title = 0x45 { action: TitleAction });
    packet!(SetCompression = 0x46 { threshold: VarInt });
    packet!(PlayerListHeaderFooter = 0x47 { header: Chat, footer: Chat });
    packet!(ResourcePackSend = 0x48 { url: String, hash: String });
    packet!(UpdateEntityNbt = 0x49 { entity_id: VarInt, nbt: Option<Nbt> });

    packet_set!(Packet {
        KeepAlive,
        JoinGame,
        ChatMessage,
        TimeUpdate,
        EntityEquipment,
        SpawnPosition,
        UpdateHealth,
        Respawn,
        PlayerPositionAndLook,
        HeldItemChange,
        UseBed,
        Animation,
        SpawnPlayer,
        CollectItem,
        SpawnObject,
        SpawnMob,
        SpawnPainting,
        SpawnExperienceOrb,
        EntityVelocity,
        DestroyEntities,
        Entity,
        EntityRelativeMove,
        EntityLook,
        EntityLookAndRelativeMove,
        EntityTeleport,
        EntityHeadLook,
        EntityStatus,
        AttachEntity,
        EntityMetadata,
        EntityEffect,
        RemoveEntityEffect,
        SetExperience,
        EntityProperties,
        ChunkData,
        MultiBlockChange,
        BlockChange,
        BlockAction,
        BlockBreakAnimation,
        MapChunkBulk,
        Explosion,
        Effect,
        SoundEffect,
        Particle,
        ChangeGameState,
        SpawnGlobalEntity,
        OpenWindow,
        CloseWindow,
        SetSlot,
        WindowItems,
        WindowProperty,
        ConfirmTransaction,
        UpdateSign,
        Maps,
        UpdateBlockEntity,
        SignEditorOpen,
        Statistics,
        PlayerListItem,
        PlayerAbilities,
        TabComplete,
        ScoreboardObjective,
        UpdateScore,
        DisplayScoreboard,
        Teams,
        PluginMessage,
        Disconnect,
        ServerDifficulty,
        CombatEvent,
        Camera,
        WorldBorder,
        Title,
        SetCompression,
        PlayerListHeaderFooter,
        ResourcePackSend,
        UpdateEntityNbt,
    });
}

pub mod serverbound {
    use super::*;

    packet!(KeepAlive = 0x00 { id: VarInt });
    packet!(ChatMessage = 0x01 { message: String });
    packet!(UseEntity = 0x02 { target: VarInt, action: UseAction });
    packet!(Player = 0x03 { on_ground: bool });
    packet!(PlayerPosition = 0x04 { x: f64, y: f64, z: f64, on_ground: bool });
    packet!(PlayerLook = 0x05 { yaw: f32, pitch: f32, on_ground: bool });
    packet!(PlayerPositionAndLook = 0x06 {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    });
    packet!(PlayerDigging = 0x07 { status: u8, location: Position, face: u8 });
    // `direction` is 255 for a use-item without a target block.
    packet!(PlayerBlockPlacement = 0x08 {
        location: Position,
        direction: u8,
        held_item: Slot,
        cursor_x: u8,
        cursor_y: u8,
        cursor_z: u8,
    });
    packet!(HeldItemChange = 0x09 { slot: i16 });
    packet!(Animation = 0x0A {});
    packet!(EntityAction = 0x0B { entity_id: VarInt, action_id: VarInt, jump_boost: VarInt });
    packet!(SteerVehicle = 0x0C { sideways: f32, forward: f32, flags: u8 });
    packet!(CloseWindow = 0x0D { window_id: u8 });
    packet!(ClickWindow = 0x0E {
        window_id: u8,
        slot: i16,
        button: i8,
        action: i16,
        mode: i8,
        clicked_item: Slot,
    });
    packet!(ConfirmTransaction = 0x0F { window_id: u8, action: i16, accepted: bool });
    packet!(CreativeInventoryAction = 0x10 { slot: i16, item: Slot });
    packet!(EnchantItem = 0x11 { window_id: u8, enchantment: i8 });
    packet!(UpdateSign = 0x12 { location: Position, lines: [Chat; 4] });
    packet!(PlayerAbilities = 0x13 { flags: u8, flying_speed: f32, walking_speed: f32 });
    packet!(TabComplete = 0x14 { text: String, looked_at: Opt<Position> });
    packet!(ClientSettings = 0x15 {
        locale: String,
        view_distance: i8,
        chat_mode: i8,
        chat_colors: bool,
        skin_parts: u8,
    });
    packet!(ClientStatus = 0x16 { action_id: VarInt });
    packet!(PluginMessage = 0x17 { channel: String, data: Rest });
    packet!(Spectate = 0x18 { target: Uuid });
    packet!(ResourcePackStatus = 0x19 { hash: String, result: VarInt });

    packet_set!(Packet {
        KeepAlive,
        ChatMessage,
        UseEntity,
        Player,
        PlayerPosition,
        PlayerLook,
        PlayerPositionAndLook,
        PlayerDigging,
        PlayerBlockPlacement,
        HeldItemChange,
        Animation,
        EntityAction,
        SteerVehicle,
        CloseWindow,
        ClickWindow,
        ConfirmTransaction,
        CreativeInventoryAction,
        EnchantItem,
        UpdateSign,
        PlayerAbilities,
        TabComplete,
        ClientSettings,
        ClientStatus,
        PluginMessage,
        Spectate,
        ResourcePackStatus,
    });
}
