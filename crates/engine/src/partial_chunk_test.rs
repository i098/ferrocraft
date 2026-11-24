//! S21 `continuous: false`: a fake local server sends a full chunk (stone, sky light 15), then a partial update
//! for section 0 only (dirt, sky light 3), then a partial update for a chunk never loaded. `Session` decodes
//! them and `Game` applies them: section 0 is replaced with its light, section 1 keeps both, the unloaded
//! chunk stays absent.

use crate::game::Game;
use ferrocraft_net_client::Session;
use ferrocraft_protocol::codec::Bytes;
use ferrocraft_protocol::frame::FrameEncoder;
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::{login, PacketSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const STONE: u16 = 1 << 4;
const DIRT: u16 = 3 << 4;

fn payload(sections: usize, state: u16, sky: u8, biomes: bool) -> Vec<u8> {
    let (mut b, mut bl, mut sl) = (vec![], vec![], vec![]);
    for _ in 0..sections {
        for _ in 0..4096 {
            b.extend_from_slice(&state.to_le_bytes());
        }
        bl.extend(std::iter::repeat_n(0u8, 2048));
        sl.extend(std::iter::repeat_n(sky | sky << 4, 2048));
    }
    b.extend(bl);
    b.extend(sl);
    if biomes {
        b.extend(std::iter::repeat_n(1u8, 256));
    }
    b
}

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
        // Chunk (0,0): sections 0 and 1, stone, sky 15.
        let mut chunk = |x, z, continuous, bitmask, data| {
            send(
                cb::Packet::from(cb::ChunkData {
                    x,
                    z,
                    continuous,
                    bitmask,
                    data: Bytes(data),
                })
                .encode(),
            )
        };
        chunk(0, 0, true, 0b11, payload(2, STONE, 15, true));
        chunk(0, 0, false, 0b01, payload(1, DIRT, 3, false));
        chunk(5, 5, false, 0b01, payload(1, DIRT, 3, false));
        // Sentinel so the client knows all three were processed.
        chunk(9, 9, true, 0b01, payload(1, STONE, 15, true));
        while s.read(&mut buf).is_ok_and(|n| n > 0) {}
    });
    port
}

#[test]
fn partial_chunk_update_replaces_only_its_section_and_light() {
    let port = fake_server();
    let mut s = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).expect("login");
    let mut g = Game::new();
    let end = Instant::now() + Duration::from_secs(10);
    while g.store.get(9, 9).is_none() {
        assert!(Instant::now() < end, "chunks did not arrive");
        for e in s.poll() {
            g.apply(e);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let c = g.store.get(0, 0).expect("chunk loaded");
    assert_eq!(c.block(1, 2, 3), DIRT, "updated section's blocks");
    assert_eq!(c.block(1, 16 + 2, 3), STONE, "untouched section's blocks");
    assert_eq!(
        c.light().section(0, true).unwrap()[0],
        0x33,
        "updated section's light"
    );
    assert_eq!(
        c.light().section(1, true).unwrap()[0],
        0xFF,
        "untouched section's light"
    );
    assert!(
        g.store.get(5, 5).is_none(),
        "partial update must not create a chunk"
    );
}
