//! Entity packets as [`EntityEvent`] values, and the combat packets a client sends.

use crate::{send, Session};
use ferrocraft_protocol::codec::{MetaEntry, MetaValue, Metadata, Slot, VarInt};
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::play::{serverbound as sb, PlayerListAction, UseAction};
use std::io;
use world::entities::{EntityEvent, ItemRef, SizeMeta};

fn item(slot: &Slot) -> Option<ItemRef> {
    slot.as_ref().map(|s| ItemRef {
        id: s.id,
        damage: s.damage,
        count: s.count.max(0) as u8,
        dye: s.nbt.as_ref().and_then(|n| n.dye()),
        glint: world::inventory::glint(s),
    })
}

fn find(meta: &Metadata, index: u8) -> Option<&MetaValue> {
    meta.0
        .iter()
        .find(|e: &&MetaEntry| e.index == index)
        .map(|e| &e.value)
}

fn flags(meta: &Metadata) -> Option<u8> {
    match find(meta, 0) {
        Some(MetaValue::Byte(b)) => Some(*b as u8),
        _ => None,
    }
}

fn name(meta: &Metadata) -> Option<String> {
    match find(meta, 2) {
        Some(MetaValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn health(meta: &Metadata) -> Option<f32> {
    match find(meta, 6) {
        Some(MetaValue::Float(h)) => Some(*h),
        _ => None,
    }
}

fn potion(id: i32, meta: &Metadata) -> Option<EntityEvent> {
    let color = match find(meta, 7) {
        Some(MetaValue::Int(c)) => Some(*c),
        _ => None,
    };
    let ambient = match find(meta, 8) {
        Some(MetaValue::Byte(b)) => Some(*b > 0),
        _ => None,
    };
    (color.is_some() || ambient.is_some()).then_some(EntityEvent::Potion { id, color, ambient })
}

fn stack(meta: &Metadata) -> Option<ItemRef> {
    // An item entity keeps its stack at index 10, an item frame at index 8.
    match find(meta, 10).or_else(|| find(meta, 8)) {
        Some(MetaValue::Slot(s)) => item(s),
        _ => None,
    }
}

fn size_meta(meta: &Metadata) -> SizeMeta {
    let byte = |index| match find(meta, index) {
        Some(MetaValue::Byte(b)) => Some(*b as u8),
        _ => None,
    };
    SizeMeta {
        stand: byte(10),
        age: byte(12),
        slime: byte(16),
        guardian: match find(meta, 16) {
            Some(MetaValue::Int(i)) => Some(*i),
            _ => None,
        },
        horse: byte(19),
        horse_flags: match find(meta, 16) {
            Some(MetaValue::Int(i)) => Some(*i),
            _ => None,
        },
        always_name: byte(3),
        skeleton: byte(13),
        rotation: byte(9),
    }
}

/// The events of an entity packet; empty for every other packet.
pub(crate) fn events(packet: &cb::Packet) -> Vec<EntityEvent> {
    if let cb::Packet::PlayerListItem(p) = packet {
        return player_list(&p.action);
    }
    let (id, meta) = match packet {
        cb::Packet::SpawnPlayer(p) => (p.entity_id.0, Some(&p.metadata)),
        cb::Packet::SpawnMob(p) => (p.entity_id.0, Some(&p.metadata)),
        cb::Packet::EntityMetadata(p) => (p.entity_id.0, Some(&p.metadata)),
        _ => (0, None),
    };
    spawn_event(packet)
        .or_else(|| motion_event(packet))
        .or_else(|| state_event(packet))
        .into_iter()
        .chain(meta.and_then(|m| potion(id, m)))
        .collect()
}

fn spawn_event(packet: &cb::Packet) -> Option<EntityEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::SpawnPlayer(p) => EntityEvent::SpawnPlayer {
            id: p.entity_id.0,
            uuid: p.uuid.0,
            pos: [p.x, p.y, p.z],
            yaw: p.yaw,
            pitch: p.pitch,
            held: p.current_item,
            flags: flags(&p.metadata),
        },
        P::SpawnMob(p) => EntityEvent::SpawnMob {
            id: p.entity_id.0,
            kind: p.kind,
            pos: [p.x, p.y, p.z],
            yaw: p.yaw,
            pitch: p.pitch,
            head_yaw: p.head_pitch,
            vel: [p.velocity_x, p.velocity_y, p.velocity_z],
            flags: flags(&p.metadata),
            name: name(&p.metadata),
            health: health(&p.metadata),
            size: size_meta(&p.metadata),
        },
        P::SpawnObject(p) => EntityEvent::SpawnObject {
            id: p.entity_id.0,
            kind: p.kind,
            pos: [p.x, p.y, p.z],
            yaw: p.yaw,
            pitch: p.pitch,
            vel: p.velocity.unwrap_or([0; 3]),
            data: p.data,
        },
        P::SpawnPainting(p) => EntityEvent::SpawnPainting {
            id: p.entity_id.0,
            title: p.title.clone(),
            pos: [p.location.x, p.location.y, p.location.z],
            facing: p.direction,
        },
        P::SpawnExperienceOrb(p) => EntityEvent::SpawnExperienceOrb {
            id: p.entity_id.0,
            pos: [p.x, p.y, p.z],
        },
        _ => return None,
    })
}

fn motion_event(packet: &cb::Packet) -> Option<EntityEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::EntityRelativeMove(p) => EntityEvent::Move {
            id: p.entity_id.0,
            delta: [p.dx, p.dy, p.dz],
            look: None,
            on_ground: p.on_ground,
        },
        P::EntityLookAndRelativeMove(p) => EntityEvent::Move {
            id: p.entity_id.0,
            delta: [p.dx, p.dy, p.dz],
            look: Some((p.yaw, p.pitch)),
            on_ground: p.on_ground,
        },
        P::EntityLook(p) => EntityEvent::Look {
            id: p.entity_id.0,
            yaw: p.yaw,
            pitch: p.pitch,
            on_ground: p.on_ground,
        },
        P::EntityTeleport(p) => EntityEvent::Teleport {
            id: p.entity_id.0,
            pos: [p.x, p.y, p.z],
            yaw: p.yaw,
            pitch: p.pitch,
            on_ground: p.on_ground,
        },
        P::EntityHeadLook(p) => EntityEvent::HeadLook {
            id: p.entity_id.0,
            head_yaw: p.head_yaw,
        },
        P::EntityVelocity(p) => EntityEvent::Velocity {
            id: p.entity_id.0,
            vel: [p.x, p.y, p.z],
        },
        P::DestroyEntities(p) => EntityEvent::Destroy(p.entity_ids.iter().map(|v| v.0).collect()),
        _ => return None,
    })
}

fn state_event(packet: &cb::Packet) -> Option<EntityEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::EntityMetadata(p) => EntityEvent::Metadata {
            id: p.entity_id.0,
            flags: flags(&p.metadata),
            name: name(&p.metadata),
            stack: stack(&p.metadata),
            health: health(&p.metadata),
            size: size_meta(&p.metadata),
        },
        P::EntityEquipment(p) => EntityEvent::Equipment {
            id: p.entity_id.0,
            slot: p.slot,
            item: item(&p.item),
        },
        P::Animation(p) => EntityEvent::Animation {
            id: p.entity_id.0,
            animation: p.animation,
        },
        P::CollectItem(p) => EntityEvent::Pickup { id: p.collected.0 },
        P::EntityStatus(p) => EntityEvent::Status {
            id: p.entity_id,
            status: p.status,
        },
        P::AttachEntity(p) if !p.leash => EntityEvent::Attach {
            id: p.entity_id,
            vehicle: p.vehicle_id,
        },
        _ => return None,
    })
}

fn player_list(action: &PlayerListAction) -> Vec<EntityEvent> {
    match action {
        PlayerListAction::Add(v) => v
            .iter()
            .map(|(u, a)| EntityEvent::PlayerName {
                uuid: u.0,
                name: a.name.clone(),
                textures: a
                    .properties
                    .iter()
                    .find(|p| p.name == "textures")
                    .map(|p| p.value.clone()),
            })
            .collect(),
        PlayerListAction::Remove(v) => v
            .iter()
            .map(|u| EntityEvent::PlayerRemove { uuid: u.0 })
            .collect(),
        _ => vec![],
    }
}

impl Session {
    /// C05 `PlayerLook`: `EntityPlayerSP.onUpdate` sends it instead of a position while riding.
    pub fn send_look(&mut self, yaw: f32, pitch: f32, on_ground: bool) -> io::Result<()> {
        let p = sb::PlayerLook {
            yaw,
            pitch,
            on_ground,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C0C `SteerVehicle` (`PacketInput`): flag 1 is jump, flag 2 is the dismount (sneak).
    pub fn send_steer(
        &mut self,
        sideways: f32,
        forward: f32,
        jump: bool,
        dismount: bool,
    ) -> io::Result<()> {
        let p = sb::SteerVehicle {
            sideways,
            forward,
            flags: u8::from(jump) | u8::from(dismount) << 1,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// Left click on an entity: `UseEntity` attack. The caller sends the arm swing first.
    pub fn attack_entity(&mut self, target: i32) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::UseEntity {
                target: VarInt(target),
                action: UseAction::Attack,
            }),
        )
    }

    /// Right click on an entity: `InteractAt` with the hit point relative to the entity.
    pub fn interact_entity_at(&mut self, target: i32, [x, y, z]: [f32; 3]) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::UseEntity {
                target: VarInt(target),
                action: UseAction::InteractAt { x, y, z },
            }),
        )
    }

    /// Right click on an entity that `InteractAt` did not handle: `Interact`.
    pub fn interact_entity(&mut self, target: i32) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::UseEntity {
                target: VarInt(target),
                action: UseAction::Interact,
            }),
        )
    }

    /// `C0B` entity action: 0 sneak, 1 unsneak, 3 start sprint, 4 stop sprint, 5 horse jump
    /// (`jump_boost` is the charge in percent), 6 open the horse inventory.
    pub fn entity_action(
        &mut self,
        entity_id: i32,
        action: i32,
        jump_boost: i32,
    ) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::EntityAction {
                entity_id: VarInt(entity_id),
                action_id: VarInt(action),
                jump_boost: VarInt(jump_boost),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_protocol::PacketSet;

    #[test]
    fn spawn_mob_metadata_gives_flags_and_name() {
        let p = cb::Packet::from(cb::SpawnMob {
            entity_id: VarInt(5),
            kind: 54,
            x: 32,
            y: 64,
            z: 96,
            yaw: 0,
            pitch: 0,
            head_pitch: 0,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
            metadata: Metadata(vec![
                MetaEntry {
                    index: 0,
                    value: MetaValue::Byte(0b10),
                },
                MetaEntry {
                    index: 6,
                    value: MetaValue::Float(150.0),
                },
                MetaEntry {
                    index: 2,
                    value: MetaValue::String("Bob".into()),
                },
            ]),
        });
        let frame = p.encode();
        let decoded = cb::Packet::decode(&frame).unwrap();
        let EntityEvent::SpawnMob {
            id,
            flags,
            name,
            health,
            pos,
            ..
        } = events(&decoded).remove(0)
        else {
            panic!("not a spawn")
        };
        assert_eq!(
            (id, flags, name.as_deref(), health, pos),
            (5, Some(2), Some("Bob"), Some(150.0), [32, 64, 96])
        );
    }

    #[test]
    fn collect_item_gives_the_collected_entity() {
        let p = cb::Packet::from(cb::CollectItem {
            collected: VarInt(9),
            collector: VarInt(1),
        });
        let decoded = cb::Packet::decode(&p.encode()).unwrap();
        assert_eq!(events(&decoded), vec![EntityEvent::Pickup { id: 9 }]);
    }
}
