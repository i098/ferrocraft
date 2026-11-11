//! A fake local server sends a `PlayerListItem` with a `textures` property, then `SpawnPlayer`. The
//! skin comes from a local HTTP file (`FERROCRAFT_SKIN_HOST`) and two players draw offscreen: the
//! skinned one and one without `textures`. `SKIN_PROOF_PNG` writes the frame.
//!
//! No vanilla server sends `textures` in offline mode, so this test plays the server itself.

use ferrocraft_net_client::{fetch_skin, parse_textures, NetEvent, Session};
use ferrocraft_protocol::codec::{Metadata, Opt, Uuid, VarInt};
use ferrocraft_protocol::frame::FrameEncoder;
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::play::{PlayerListAction, PlayerListAdd, ProfileProperty};
use ferrocraft_protocol::{login, PacketSet};
use render::{decode_png, encode_png, Camera, Class, EntityDraw, Renderer};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use world::entities::{Entities, Kind};

const SKINNED: u128 = 0x1111;
const PLAIN: u128 = 0x2222;

fn b64(s: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in s.as_bytes().chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=c.len() {
            out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out + &"=".repeat((3 - s.len() % 3) % 3)
}

/// Serves `body` to every request on a free port; every request is logged and counted.
fn http_skin(body: Vec<u8>, hits: Arc<AtomicUsize>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap_or(0);
            hits.fetch_add(1, Ordering::SeqCst);
            eprintln!(
                "http: {}",
                String::from_utf8_lossy(&buf[..n])
                    .lines()
                    .next()
                    .unwrap_or("")
            );
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    port
}

fn fake_server(textures: String) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // One connection per join: the test joins twice.
        for _ in 0..2 {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 256];
            let _ = s.read(&mut buf); // handshake and login start
            let mut enc = FrameEncoder::default();
            let mut send = |p: &dyn Fn() -> Vec<u8>| {
                let mut out = Vec::new();
                enc.encode(&p(), &mut out).unwrap();
                s.write_all(&out).unwrap();
            };
            send(&|| {
                login::clientbound::Packet::from(login::clientbound::LoginSuccess {
                    uuid: "00000000-0000-0000-0000-000000000001".into(),
                    username: "Ferro".into(),
                })
                .encode()
            });
            send(&|| {
                cb::Packet::from(cb::JoinGame {
                    entity_id: 1,
                    gamemode: 0,
                    dimension: 0,
                    difficulty: 0,
                    max_players: 4,
                    level_type: "flat".into(),
                    reduced_debug_info: false,
                })
                .encode()
            });
            let add = |uuid: u128, name: &str, props: Vec<ProfileProperty>| {
                (
                    Uuid(uuid),
                    PlayerListAdd {
                        name: name.into(),
                        properties: props,
                        gamemode: VarInt(0),
                        ping: VarInt(0),
                        display_name: Opt(None),
                    },
                )
            };
            let prop = ProfileProperty {
                name: "textures".into(),
                value: textures.clone(),
                signature: Opt(None),
            };
            send(&|| {
                cb::Packet::from(cb::PlayerListItem {
                    action: PlayerListAction::Add(vec![
                        add(SKINNED, "Skinned", vec![prop.clone()]),
                        add(PLAIN, "Plain", vec![]),
                    ]),
                })
                .encode()
            });
            for (id, uuid, x) in [(2, SKINNED, 9 * 32), (3, PLAIN, 11 * 32)] {
                send(&|| {
                    cb::Packet::from(cb::SpawnPlayer {
                        entity_id: VarInt(id),
                        uuid: Uuid(uuid),
                        x,
                        y: 64 * 32,
                        z: 10 * 32,
                        yaw: 0,
                        pitch: 0,
                        current_item: 0,
                        metadata: Metadata(vec![]),
                    })
                    .encode()
                });
            }
            while s.read(&mut buf).is_ok_and(|n| n > 0) {} // hold the connection until the client closes
        }
    });
    port
}

#[test]
fn textures_property_skins_the_spawned_player() {
    // A checkerboard skin: red/blue 4x4 texel cells.
    let mut skin = Vec::new();
    for y in 0..64 {
        for x in 0..64 {
            let on = (x / 4 + y / 4) % 2 == 0;
            skin.extend_from_slice(&[if on { 255 } else { 0 }, 0, if on { 0 } else { 255 }, 255]);
        }
    }
    let dir = std::env::temp_dir().join(format!("ferrocraft-skin-proof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("skin.png");
    encode_png(&png, 64, 64, &skin).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let http = http_skin(std::fs::read(&png).unwrap(), hits.clone());

    std::env::set_var("FERROCRAFT_SKIN_HOST", "127.0.0.1");
    let url = format!("http://127.0.0.1:{http}/texture/abc");
    let json = format!(r#"{{"textures":{{"SKIN":{{"url":"{url}"}}}}}}"#);
    let server = fake_server(b64(&json));

    let join = || {
        let mut s = Session::connect(&format!("127.0.0.1:{server}"), "Ferro", None).expect("login");
        let mut entities = Entities::default();
        let end = Instant::now() + Duration::from_secs(10);
        while entities.iter().filter(|e| e.kind == Kind::Player).count() < 2 {
            assert!(Instant::now() < end, "players did not spawn");
            for e in s.poll() {
                if let NetEvent::Entity(ev) = e {
                    entities.apply(ev);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        entities
    };
    let entities = join();
    let players: Vec<_> = entities.iter().filter(|e| e.kind == Kind::Player).collect();
    let skinned = players.iter().find(|e| e.uuid == Some(SKINNED)).unwrap();
    let plain = players.iter().find(|e| e.uuid == Some(PLAIN)).unwrap();
    assert!(skinned.textures.is_some(), "textures reach the entity");
    assert!(plain.textures.is_none());

    let info = parse_textures(skinned.textures.as_deref().unwrap()).expect("trusted url");
    let cache = dir.join("cache");
    let bytes = fetch_skin(&info.url, &cache).expect("download");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the first join downloads once"
    );
    assert_eq!(cache.read_dir().unwrap().count(), 1, "cached");
    eprintln!(
        "join 1: http requests = {}, cache files = 1",
        hits.load(Ordering::SeqCst)
    );

    // Second join: the same `textures` property, the skin now comes from the cache.
    let again = join();
    let second = again.iter().find(|e| e.uuid == Some(SKINNED)).unwrap();
    let info2 = parse_textures(second.textures.as_deref().unwrap()).unwrap();
    assert_eq!(fetch_skin(&info2.url, &cache).as_deref(), Some(&bytes[..]));
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the second join makes no request"
    );
    eprintln!(
        "join 2: http requests = {} (cache hit)",
        hits.load(Ordering::SeqCst)
    );
    let img = decode_png(&bytes).unwrap();

    let mut r = Renderer::new_offscreen(400, 300, None);
    r.set_player_skin(SKINNED, &img);
    let draw = |x: f64, skin: Option<u128>| EntityDraw {
        class: Class::Player { slim: info.slim },
        pos: [x, 64.0, 10.0],
        yaw: 180.0,
        head_yaw: 180.0,
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
        name: None,
        skin,
        burning: true,
        eye_height: 1.62,
        always_name: false,
        pointed: false,
        ridden: false,
        child: false,
    };
    let at = |e: &world::entities::Entity| e.pos[0];
    r.set_entities(&[draw(at(skinned), Some(SKINNED)), draw(at(plain), None)]);
    let cam = Camera {
        pos: [10.0, 65.5, 14.0],
        yaw: 180.0,
        pitch: 0.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let (w, h, px) = r.render_to_rgba(&cam, &[]);
    let blue = px.chunks(4).filter(|p| p[2] > 200 && p[1] < 50).count();
    assert!(blue > 500, "the skin is drawn: {blue} blue px");
    if let Some(path) = std::env::var_os("SKIN_PROOF_PNG") {
        encode_png(std::path::Path::new(&path), w, h, &px).unwrap();
    }
    let _ = std::fs::remove_dir_all(dir);
}
