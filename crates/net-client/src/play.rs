//! Reader thread: play packets in, [`NetEvent`] values out.

use crate::link::{Reader, Writer};
use crate::{chat, entity, events, send, NetEvent};
use ferrocraft_protocol::codec::Wire;
use ferrocraft_protocol::play::{clientbound as cb, serverbound as sb};
use std::io;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use world::chunk::Chunk;
use world::entities::EntityEvent;

pub fn run(mut reader: Reader, writer: Arc<Mutex<Writer>>, tx: Sender<NetEvent>) {
    let mut dimension = 0;
    let end = loop {
        let packet = match reader.recv_or_skip::<cb::Packet>() {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => break format!("connection closed: {e}"),
        };
        match handle(packet, &mut reader, &writer, &mut dimension) {
            Ok(events) => {
                let kicked = events.iter().any(|e| matches!(e, NetEvent::Disconnect(_)));
                let mut open = true;
                for e in events {
                    open &= tx.send(e).is_ok();
                }
                if !open {
                    return;
                }
                if kicked {
                    return;
                }
            }
            Err(e) => break format!("connection closed: {e}"),
        }
    };
    let _ = tx.send(NetEvent::Disconnect(end));
}

/// `WorldType.parseWorldType` compares names case-insensitively.
fn is_flat(level_type: &str) -> bool {
    level_type.eq_ignore_ascii_case("flat")
}

fn handle(
    packet: cb::Packet,
    reader: &mut Reader,
    writer: &Mutex<Writer>,
    dimension: &mut i32,
) -> io::Result<Vec<NetEvent>> {
    Ok(match packet {
        cb::Packet::KeepAlive(k) => {
            send(writer, sb::Packet::from(sb::KeepAlive { id: k.id }))?;
            vec![]
        }
        cb::Packet::SetCompression(c) => {
            reader.set_compression(c.threshold.0);
            writer
                .lock()
                .map_err(|_| io::Error::other("writer poisoned"))?
                .set_compression(c.threshold.0);
            vec![]
        }
        cb::Packet::Respawn(r) => {
            let changed = *dimension != r.dimension;
            *dimension = r.dimension;
            vec![
                NetEvent::Respawn {
                    dimension_changed: changed,
                },
                NetEvent::Dimension(r.dimension),
                NetEvent::LevelType {
                    flat: is_flat(&r.level_type),
                },
                NetEvent::Entity(EntityEvent::Clear),
                NetEvent::GameMode(r.gamemode & 7),
                NetEvent::Difficulty(r.difficulty),
            ]
        }
        cb::Packet::Camera(c) => vec![NetEvent::Camera(c.camera_id.0)],
        // Reason 3 is "change game mode"; the value is the mode as a float.
        cb::Packet::ChangeGameState(g) if g.reason == 3 => {
            vec![NetEvent::GameMode(g.value as u8 & 7)]
        }
        cb::Packet::JoinGame(j) => {
            *dimension = i32::from(j.dimension);
            vec![
                NetEvent::JoinGame {
                    entity_id: j.entity_id,
                    gamemode: j.gamemode & 7,
                    hardcore: j.gamemode & 8 != 0,
                    difficulty: j.difficulty,
                    max_players: j.max_players,
                    reduced_debug_info: j.reduced_debug_info,
                },
                NetEvent::Dimension(i32::from(j.dimension)),
                NetEvent::LevelType {
                    flat: is_flat(&j.level_type),
                },
            ]
        }
        cb::Packet::PluginMessage(m) => vec![NetEvent::PluginMessage {
            channel: m.channel,
            data: m.data.0,
        }],
        cb::Packet::Disconnect(d) => vec![NetEvent::Disconnect(chat::plain_text(&d.reason))],
        other => game_events(other, *dimension)?,
    })
}

/// `NetHandlerPlayClient.handleJoinGame`: `MC|Brand` with the brand as a length-prefixed string.
fn brand_message(brand: &str) -> sb::PluginMessage {
    let mut data = Vec::new();
    brand.to_string().write(&mut data);
    sb::PluginMessage {
        channel: "MC|Brand".into(),
        data: ferrocraft_protocol::codec::Rest(data),
    }
}

pub(crate) fn send_brand(writer: &Mutex<Writer>, brand: &str) -> io::Result<()> {
    send(writer, sb::Packet::from(brand_message(brand)))
}

/// `S44` as `NetHandlerPlayClient.handleWorldBorder` applies it; the speed is in milliseconds.
fn border_update(a: ferrocraft_protocol::play::WorldBorderAction) -> world::border::Update {
    use ferrocraft_protocol::play::WorldBorderAction as A;
    use world::border::Update as U;
    match a {
        A::SetSize { diameter } => U::SetSize(diameter),
        A::LerpSize {
            old_diameter,
            new_diameter,
            speed,
        } => U::LerpSize {
            old: old_diameter,
            new: new_diameter,
            millis: speed.0,
        },
        A::SetCenter { x, z } => U::SetCenter { x, z },
        A::Initialize {
            x,
            z,
            old_diameter,
            new_diameter,
            speed,
            warning_time,
            warning_blocks,
            ..
        } => U::Initialize {
            x,
            z,
            old: old_diameter,
            new: new_diameter,
            millis: speed.0,
            warning_time: warning_time.0,
            warning_blocks: warning_blocks.0,
        },
        A::SetWarningTime { seconds } => U::WarningTime(seconds.0),
        A::SetWarningBlocks { blocks } => U::WarningBlocks(blocks.0),
    }
}

fn game_events(packet: cb::Packet, dimension: i32) -> io::Result<Vec<NetEvent>> {
    match crate::tile::event(&packet) {
        Some(tile) => Ok(vec![tile]),
        None => other_events(packet, dimension),
    }
}

fn other_events(packet: cb::Packet, dimension: i32) -> io::Result<Vec<NetEvent>> {
    Ok(match packet {
        cb::Packet::PlayerPositionAndLook(p) => vec![NetEvent::PlayerPosLook {
            x: p.x,
            y: p.y,
            z: p.z,
            yaw: p.yaw,
            pitch: p.pitch,
            flags: p.flags,
        }],
        cb::Packet::ChatMessage(c) if c.position == 2 => {
            vec![NetEvent::ActionBar(chat::plain_text(&c.json))]
        }
        cb::Packet::TimeUpdate(t) => vec![NetEvent::TimeUpdate {
            world_age: t.world_age,
            time_of_day: t.time_of_day,
        }],
        cb::Packet::WorldBorder(b) => vec![NetEvent::WorldBorder(border_update(b.action))],
        cb::Packet::ResourcePackSend(p) => vec![NetEvent::ResourcePack {
            url: p.url,
            hash: p.hash,
        }],
        cb::Packet::ChangeGameState(g) => vec![NetEvent::GameState {
            reason: g.reason,
            value: g.value,
        }],
        // `NetHandlerPlayClient.handleSpawnGlobalEntity`: only type 1 (lightning) exists; positions are in 1/32 blocks.
        cb::Packet::SpawnGlobalEntity(s) if s.kind == 1 => vec![NetEvent::Lightning {
            pos: [s.x, s.y, s.z].map(|v| f64::from(v) / 32.0),
        }],
        cb::Packet::Maps(m) => maps_events(&m),
        cb::Packet::ChatMessage(c) => vec![NetEvent::Chat(chat::styled(&c.json))],
        cb::Packet::BlockChange(b) => vec![block_change(
            b.location.x,
            b.location.y,
            b.location.z,
            b.block_id.0,
        )],
        cb::Packet::MultiBlockChange(m) => multi_block_change(&m),
        cb::Packet::ChunkData(c) => chunk_data(&c, dimension == 0)?,
        cb::Packet::MapChunkBulk(b) => chunk_bulk(&b)?,
        // The player list feeds both entity names and the tab list.
        other => entity::events(&other)
            .into_iter()
            .map(NetEvent::Entity)
            .chain(events::events(&other))
            .collect(),
    })
}

/// `S34PacketMaps` (gu offsets 38-76): each icon byte holds the type in its high and the
/// rotation in its low nibble; then the colour area.
fn maps_events(m: &cb::Maps) -> Vec<NetEvent> {
    let id = m.item_damage.0;
    let icons = m
        .icons
        .iter()
        .map(|i| ((i.dir_type as u8) >> 4, i.x, i.z, i.dir_type as u8 & 15))
        .collect();
    let updates = m.update.iter().map(|u| NetEvent::Map {
        id,
        area: [u.x, u.z, m.columns, u.rows].map(usize::from),
        data: u.data.0.clone(),
    });
    std::iter::once(NetEvent::MapIcons { id, icons })
        .chain(updates)
        .collect()
}

fn block_change(x: i32, y: i32, z: i32, state: i32) -> NetEvent {
    NetEvent::BlockChange {
        x,
        y,
        z,
        state: state as u16,
    }
}

fn multi_block_change(m: &cb::MultiBlockChange) -> Vec<NetEvent> {
    m.records
        .iter()
        .map(|r| {
            block_change(
                m.chunk_x * 16 + i32::from(r.horizontal_pos >> 4),
                i32::from(r.y),
                m.chunk_z * 16 + i32::from(r.horizontal_pos & 15),
                r.block_id.0,
            )
        })
        .collect()
}

fn decode_err(e: world::chunk::Truncated) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

fn chunk_data(c: &cb::ChunkData, sky_light: bool) -> io::Result<Vec<NetEvent>> {
    if c.continuous && c.bitmask == 0 {
        return Ok(vec![NetEvent::UnloadChunk(c.x, c.z)]);
    }
    // A partial update (`!continuous`) has no biomes.
    let (chunk, _) = Chunk::decode(c.x, c.z, c.bitmask, &c.data.0, sky_light, c.continuous)
        .map_err(decode_err)?;
    if c.continuous {
        return Ok(vec![NetEvent::Chunk(Box::new(chunk))]);
    }
    // A partial update must not replace the whole column: send its blocks one by one.
    Ok((0..16usize)
        .filter_map(|s| chunk.section(s).map(|sec| (s, sec)))
        .flat_map(|(s, sec)| {
            sec.iter().enumerate().map(move |(i, &state)| {
                block_change(
                    c.x * 16 + (i & 15) as i32,
                    (s * 16 + (i >> 8)) as i32,
                    c.z * 16 + (i >> 4 & 15) as i32,
                    i32::from(state),
                )
            })
        })
        .collect())
}

fn chunk_bulk(b: &cb::MapChunkBulk) -> io::Result<Vec<NetEvent>> {
    let mut data = &b.data.0[..];
    let mut events = Vec::with_capacity(b.chunks.len());
    for m in &b.chunks {
        let (chunk, used) =
            Chunk::decode(m.x, m.z, m.bitmask, data, b.sky_light, true).map_err(decode_err)?;
        data = &data[used..];
        events.push(NetEvent::Chunk(Box::new(chunk)));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_protocol::PacketSet;

    #[test]
    fn level_type_is_flat_ignoring_case() {
        assert!(is_flat("flat") && is_flat("FLAT"));
        assert!(!is_flat("default") && !is_flat("flatten"));
    }

    /// An icons-only map packet still replaces the decorations: type 1 rotation 8 at (-4, 5).
    #[test]
    fn map_packet_icon_byte_splits_into_type_and_rotation() {
        let p = cb::Maps {
            item_damage: ferrocraft_protocol::codec::VarInt(3),
            scale: 0,
            icons: vec![ferrocraft_protocol::play::MapIcon {
                dir_type: 0x18,
                x: -4,
                z: 5,
            }],
            columns: 0,
            update: None,
        };
        assert_eq!(
            maps_events(&p),
            vec![NetEvent::MapIcons {
                id: 3,
                icons: vec![(1, -4, 5, 8)]
            }]
        );
    }

    /// A Map Chunk Bulk frame captured from a vanilla 1.8.9 server with a flat world.
    #[test]
    fn captured_bulk_decodes_to_flat_world_columns() {
        let frame = include_bytes!("../tests/fixtures/map_chunk_bulk.bin");
        let cb::Packet::MapChunkBulk(b) = cb::Packet::decode(frame).unwrap() else {
            panic!("not a bulk packet");
        };
        let events = chunk_bulk(&b).unwrap();
        assert_eq!(events.len(), b.chunks.len());
        for (e, meta) in events.iter().zip(&b.chunks) {
            let NetEvent::Chunk(c) = e else { panic!() };
            assert_eq!((c.x, c.z), (meta.x, meta.z));
            let col: Vec<u16> = (0..5).map(|y| c.block(8, y, 8)).collect();
            // bedrock, dirt, dirt, grass, air
            assert_eq!(col, [7 << 4, 3 << 4, 3 << 4, 2 << 4, 0]);
        }
    }

    /// The `MC|Brand` data: the brand as a length-prefixed string.
    fn brand_data(brand: &str) -> Vec<u8> {
        assert_eq!(brand_message(brand).channel, "MC|Brand");
        brand_message(brand).data.0
    }

    #[test]
    fn brand_is_a_length_prefixed_string() {
        assert_eq!(brand_data("vanilla"), [&[7u8][..], b"vanilla"].concat());
    }

    #[test]
    fn plugin_message_becomes_event() {
        let p = cb::Packet::from(cb::PluginMessage {
            channel: "MC|BOpen".into(),
            data: ferrocraft_protocol::codec::Rest(vec![]),
        });
        let p = cb::Packet::decode(&p.encode()).unwrap();
        let (a, b) = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let c = std::net::TcpStream::connect(l.local_addr().unwrap()).unwrap();
            (c, l.accept().unwrap().0)
        };
        let mut r = Reader::new(a.try_clone().unwrap());
        let w = Mutex::new(Writer::new(b));
        assert_eq!(
            handle(p, &mut r, &w, &mut 0).unwrap(),
            vec![NetEvent::PluginMessage {
                channel: "MC|BOpen".into(),
                data: vec![]
            }]
        );
    }

    fn global_entity(kind: i8) -> cb::Packet {
        cb::Packet::from(cb::SpawnGlobalEntity {
            entity_id: ferrocraft_protocol::codec::VarInt(7),
            kind,
            x: 64,
            y: -32,
            z: 16,
        })
    }

    /// `handleSpawnGlobalEntity`: type 1 is lightning at fixed-point (1/32 block) coordinates.
    #[test]
    fn global_entity_type_one_is_lightning_at_block_position() {
        let p = cb::Packet::decode(&global_entity(1).encode()).unwrap();
        assert_eq!(
            game_events(p, 0).unwrap(),
            vec![NetEvent::Lightning {
                pos: [2.0, -1.0, 0.5]
            }]
        );
    }

    #[test]
    fn other_global_entity_types_give_no_event() {
        let p = cb::Packet::decode(&global_entity(2).encode()).unwrap();
        assert!(game_events(p, 0).unwrap().is_empty());
    }
}
