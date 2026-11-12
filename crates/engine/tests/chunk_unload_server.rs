//! A fake local server streams 200 chunk columns while the "player" walks, unloading each column
//! 12 columns behind with a continuous bitmask-0 ChunkData (the vanilla 1.8.9 unload packet).
//! The client store must stay bounded instead of growing with every column ever sent (issue #167).

use ferrocraft_net_client::NetEvent;
use ferrocraft_net_client::Session;
use ferrocraft_protocol::codec::Bytes;
use ferrocraft_protocol::frame::FrameEncoder;
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::{login, PacketSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use world::chunk::ChunkStore;

const COLUMNS: i32 = 200;
const WINDOW: i32 = 12;

fn fake_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 256];
        let _ = s.read(&mut buf);
        let mut enc = FrameEncoder::default();
        let mut send = |p: Vec<u8>| {
            let mut out = Vec::new();
            enc.encode(&p, &mut out).unwrap();
            s.write_all(&out).unwrap();
        };
        send(
            login::clientbound::Packet::from(login::clientbound::LoginSuccess {
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                username: "Ferro".into(),
            })
            .encode(),
        );
        send(
            cb::Packet::from(cb::JoinGame {
                entity_id: 1,
                gamemode: 0,
                dimension: 0,
                difficulty: 0,
                max_players: 4,
                level_type: "flat".into(),
                reduced_debug_info: false,
            })
            .encode(),
        );
        for x in 0..COLUMNS {
            // one section: blocks, block light, sky light, then 256 biome bytes
            let data = vec![0u8; 8192 + 2048 + 2048 + 256];
            send(
                cb::Packet::from(cb::ChunkData {
                    x,
                    z: 0,
                    continuous: true,
                    bitmask: 1,
                    data: Bytes(data),
                })
                .encode(),
            );
            if x >= WINDOW {
                send(
                    cb::Packet::from(cb::ChunkData {
                        x: x - WINDOW,
                        z: 0,
                        continuous: true,
                        bitmask: 0,
                        data: Bytes(vec![]),
                    })
                    .encode(),
                );
            }
        }
        while s.read(&mut buf).is_ok_and(|n| n > 0) {}
    });
    port
}

#[test]
fn server_unload_packets_keep_the_chunk_store_bounded() {
    let port = fake_server();
    let mut session = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).expect("login");
    let mut store = ChunkStore::default();
    let mut peak = 0;
    let end = Instant::now() + Duration::from_secs(20);
    while store.len() > WINDOW as usize || peak == 0 || store.get(COLUMNS - 1, 0).is_none() {
        assert!(
            Instant::now() < end,
            "stream did not finish, store={}",
            store.len()
        );
        for e in session.poll() {
            match e {
                NetEvent::Chunk(c) => store.insert(*c),
                NetEvent::UnloadChunk(x, z) => {
                    store.remove(x, z);
                }
                _ => {}
            }
            peak = peak.max(store.len());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(peak <= WINDOW as usize + 1, "peak {peak}");
    assert_eq!(store.len(), WINDOW as usize);
    assert!(store.get(0, 0).is_none());
}
