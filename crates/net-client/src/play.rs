//! Reader thread: play packets in, [`NetEvent`] values out.

use crate::link::{Reader, Writer};
use crate::{chat, entity, send, NetEvent};
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
            *dimension = r.dimension;
            vec![
                NetEvent::Respawn,
                NetEvent::Entity(EntityEvent::Clear),
                NetEvent::GameMode(r.gamemode & 7),
            ]
        }
        // Reason 3 is "change game mode"; the value is the mode as a float.
        cb::Packet::ChangeGameState(g) if g.reason == 3 => {
            vec![NetEvent::GameMode(g.value as u8 & 7)]
        }
        cb::Packet::JoinGame(j) => {
            *dimension = i32::from(j.dimension);
            // Only now is the server surely in the play state; earlier play packets get "Bad packet id".
            let settings = sb::ClientSettings {
                locale: "en_US".into(),
                view_distance: 8,
                chat_mode: 0,
                chat_colors: true,
                skin_parts: 0x7F,
            };
            send(writer, sb::Packet::from(settings))?;
            vec![NetEvent::JoinGame {
                entity_id: j.entity_id,
                gamemode: j.gamemode & 7,
            }]
        }
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
        cb::Packet::ChangeGameState(g) => vec![NetEvent::GameState {
            reason: g.reason,
            value: g.value,
        }],
        cb::Packet::ChatMessage(c) => vec![NetEvent::Chat(chat::styled(&c.json))],
        cb::Packet::Disconnect(d) => vec![NetEvent::Disconnect(chat::plain_text(&d.reason))],
        cb::Packet::BlockChange(b) => vec![block_change(
            b.location.x,
            b.location.y,
            b.location.z,
            b.block_id.0,
        )],
        cb::Packet::MultiBlockChange(m) => multi_block_change(&m),
        cb::Packet::ChunkData(c) => chunk_data(&c, *dimension == 0)?,
        cb::Packet::MapChunkBulk(b) => chunk_bulk(&b)?,
        other => entity::events(&other)
            .into_iter()
            .map(NetEvent::Entity)
            .collect(),
    })
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
        return Ok(vec![NetEvent::Chunk(chunk)]);
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
        events.push(NetEvent::Chunk(chunk));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_protocol::PacketSet;

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
}
