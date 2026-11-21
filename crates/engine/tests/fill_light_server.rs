//! #778 scene (copy of #693 harness)
//! #693 scene: a 7-deep water body under a clear sky. A fake local server sends one `ChunkData` with sky light
//! attenuated 1 per water block (not what the renderer's own flood gives, so the frames show which light is used). `Session` decodes it, the renderer
//! uploads it, and the eye 3 blocks under the surface reads the sent light, not a flood. `LIGHT_PROOF_DIR`
//! writes the frames.

use ferrocraft_net_client::{NetEvent, Session};
use ferrocraft_protocol::frame::FrameEncoder;
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::{login, PacketSet};
use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const STONE: u16 = 1 << 4;

const GROUND: usize = 40; // stone y 0..=39, open sky above

fn block(y: usize) -> u16 {
    if y < GROUND {
        STONE
    } else {
        0
    }
}

fn sky(y: usize) -> u8 {
    if y < GROUND {
        0
    } else {
        15
    }
}

fn chunk_payload() -> Vec<u8> {
    let (mut blocks, mut block_light, mut sky_light) = (vec![], vec![], vec![]);
    for s in 0..4usize {
        for i in 0..4096usize {
            blocks.extend_from_slice(&block(s * 16 + (i >> 8)).to_le_bytes());
        }
        block_light.extend(std::iter::repeat_n(0u8, 2048));
        for i in 0..2048usize {
            let v = sky(s * 16 + ((i * 2) >> 8));
            sky_light.push(v | v << 4);
        }
    }
    let mut d = blocks;
    d.extend(block_light);
    d.extend(sky_light);
    d.extend(std::iter::repeat_n(1u8, 256));
    d
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
        send(
            cb::Packet::from(cb::ChunkData {
                x: 0,
                z: 0,
                continuous: true,
                bitmask: 0b1111,
                data: ferrocraft_protocol::codec::Bytes(chunk_payload()),
            })
            .encode(),
        );
        while s.read(&mut buf).is_ok_and(|n| n > 0) {}
    });
    port
}

/// A `world` chunk as the engine hands it to the renderer; `sent` says whether its server light is passed on.
struct Plain<'a>(&'a world::chunk::Chunk, bool);
impl ChunkBlocks for Plain<'_> {
    fn light_section(&self, s: usize, sky: bool) -> Option<&[u8; 2048]> {
        self.1.then(|| self.0.light().section(s, sky)).flatten()
    }
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        0
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        self.0.block(x, y, z)
    }
}

fn mean(rgba: &[u8]) -> f64 {
    rgba.chunks(4)
        .map(|p| (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0)
        .sum::<f64>()
        / (rgba.len() / 4) as f64
}

/// A fill that digs an open pit must not leave the sent (stale, dark) light on the chunk: the lit frame must
/// match a flood-only render of the same final blocks.
#[test]
fn fill_does_not_darken_the_scene() {
    let port = fake_server();
    let mut s = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).expect("login");
    let end = Instant::now() + Duration::from_secs(10);
    let mut chunk = loop {
        assert!(Instant::now() < end, "chunk did not arrive");
        if let Some(c) = s.poll().into_iter().find_map(|e| match e {
            NetEvent::Chunk(c) => Some(c),
            _ => None,
        }) {
            break c;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let cam = Camera {
        pos: [8.5, 37.5, 8.5],
        yaw: 0.0,
        pitch: 0.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let mut r = Renderer::new_offscreen(400, 300, None);
    r.upload_chunk(&Plain(&chunk, true));
    let before = r.render_to_rgba(&cam, &[]).2;

    // /fill 4 30 4 12 39 12 air
    for y in 30..40 {
        for z in 4..13 {
            for x in 4..13 {
                chunk.set_block(x, y, z, 0);
            }
        }
    }
    r.upload_chunk(&Plain(&chunk, true));
    let after = r.render_to_rgba(&cam, &[]).2;
    r.upload_chunk(&Plain(&chunk, false));
    let reference = r.render_to_rgba(&cam, &[]).2;
    let (mb, ma, mr) = (mean(&before), mean(&after), mean(&reference));
    eprintln!("mean brightness before {mb:.2} after-fill {ma:.2} flood-reference {mr:.2}");
    if let Some(dir) = std::env::var_os("LIGHT_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(&dir.join("fill_before.png"), 400, 300, &before).unwrap();
        encode_png(&dir.join("fill_after.png"), 400, 300, &after).unwrap();
        encode_png(&dir.join("fill_reference.png"), 400, 300, &reference).unwrap();
    }
    assert!(
        ma >= mr * 0.95,
        "scene went dark after fill: {ma:.2} vs reference {mr:.2}"
    );
}
