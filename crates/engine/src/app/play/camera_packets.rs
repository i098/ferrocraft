//! `EntityPlayerSP.onUpdateWalkingPlayer` and `updateEntityActionState` while the camera is another entity,
//! and the `handlePlayerMove` reply to S08.
use crate::app::App;
use crate::args::Args;
use crate::link::Link;
use ferrocraft_net_client::Session;
use ferrocraft_protocol::codec::{Metadata, VarInt};
use ferrocraft_protocol::frame::{FrameDecoder, FrameEncoder};
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::{login, PacketSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const CHAT: u8 = 0x01;
const POSE: u8 = 0x06;
const ENTITY_ACTION: u8 = 0x0B;
const OWN_ID: i32 = 1;
const OTHER_ID: i32 = 7;

/// A server that accepts the login, sends every packet queued on the returned sender, and reports the
/// id of every packet the client sends after the login.
fn server() -> (u16, Receiver<Vec<u8>>, Sender<Vec<u8>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let (tx, rx) = channel();
    let (out_tx, out_rx) = channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf);
        let w = s.try_clone().unwrap();
        std::thread::spawn(move || write_packets(w, out_rx));
        read_ids(s, tx);
    });
    (port, rx, out_tx)
}

/// Sends the login success, then every queued packet.
fn write_packets(mut w: impl Write, out_rx: Receiver<Vec<u8>>) {
    let mut enc = FrameEncoder::default();
    let login = login::clientbound::LoginSuccess {
        uuid: "00000000-0000-0000-0000-000000000001".into(),
        username: "Ferro".into(),
    };
    let mut out = Vec::new();
    enc.encode(&login::clientbound::Packet::from(login).encode(), &mut out)
        .unwrap();
    w.write_all(&out).unwrap();
    while let Ok(p) = out_rx.recv() {
        let mut out = Vec::new();
        enc.encode(&p, &mut out).unwrap();
        w.write_all(&out).unwrap();
    }
}

/// Forwards every frame the client sends.
fn read_ids(mut s: impl Read, tx: Sender<Vec<u8>>) {
    let mut buf = [0u8; 4096];
    let mut dec = FrameDecoder::default();
    while let Ok(n @ 1..) = s.read(&mut buf) {
        dec.feed(&buf[..n]);
        while let Ok(Some(f)) = dec.next_frame() {
            // Id 0 is the login start that the first read may have left behind.
            if f[0] != 0 {
                let _ = tx.send(f.to_vec());
            }
        }
    }
}

/// Pumps the network until `done` holds.
fn pump_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < end, "server packet never applied");
        app.pump_network();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Packets arrive in order, so a chat marks the end of everything the client sent before it.
fn sent_frames(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
    app.link.session().unwrap().send_chat("end").unwrap();
    let mut frames = Vec::new();
    loop {
        let f = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if f[0] == CHAT {
            return frames;
        }
        frames.push(f);
    }
}

fn sent_so_far(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<u8> {
    sent_frames(app, rx).iter().map(|f| f[0]).collect()
}

/// Runs one tick with `W` and sneak held and returns the packet ids sent, plus the x/z the player moved.
fn tick(app: &mut App, rx: &Receiver<Vec<u8>>) -> (Vec<u8>, [f64; 2]) {
    let before = app.game.player.as_ref().unwrap().pos;
    app.tick_once();
    let ids = sent_so_far(app, rx);
    let p = app.game.player.as_ref().unwrap();
    (ids, [p.pos[0] - before[0], p.pos[2] - before[2]])
}

fn send(out: &Sender<Vec<u8>>, p: cb::Packet) {
    out.send(p.encode()).unwrap();
}

fn spawn_mob() -> cb::SpawnMob {
    cb::SpawnMob {
        entity_id: VarInt(OTHER_ID),
        kind: 100,
        x: 0,
        y: 0,
        z: 0,
        yaw: 0,
        pitch: 0,
        head_pitch: 0,
        velocity_x: 0,
        velocity_y: 0,
        velocity_z: 0,
        metadata: Metadata(vec![]),
    }
}

/// Puts the player on a block with the screen closed and `W` and sneak held.
fn hold_forward(app: &mut App) {
    app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
    let m = metrics!(app);
    app.ui.close(&m);
    app.input.set_screen_open(false);
    app.input.on_key(KeyCode::KeyW, true, None);
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    assert!(!app.ui.is_open(), "screen open");
}

fn joined_app(port: u16, out: &Sender<Vec<u8>>, rx: &Receiver<Vec<u8>>) -> App {
    let mut app = App::new(Args {
        username: "Ferro".into(),
        auth: None,
        assets_dir: None,
        server: None,
        game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
    });
    let session = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).unwrap();
    app.link = Link::Live(session);
    app.ui.in_world = true;
    let join = cb::JoinGame {
        entity_id: OWN_ID,
        gamemode: 0,
        dimension: 0,
        difficulty: 0,
        max_players: 4,
        level_type: "flat".into(),
        reduced_debug_info: false,
    };
    send(out, join.into());
    send(out, spawn_mob().into());
    pump_until(&mut app, |a| a.game.entities.get(OTHER_ID).is_some());
    assert_eq!(app.game.entity_id, OWN_ID);
    sent_so_far(&mut app, rx); // drop what the client sent while joining
    app
}

#[test]
fn camera_packet_stops_and_resumes_movement_packets() {
    let (port, rx, out) = server();
    let mut app = joined_app(port, &out, &rx);
    let mut chunk = world::chunk::Chunk::new(0, 0);
    chunk.set_block(0, 63, 0, 1 << 4);
    app.game.store.insert(chunk);
    hold_forward(&mut app);

    let camera = |id| {
        cb::Camera {
            camera_id: VarInt(id),
        }
        .into()
    };
    send(&out, camera(OTHER_ID));
    pump_until(&mut app, |a| a.game.camera == Some(OTHER_ID));
    let (ids, moved) = tick(&mut app, &rx);
    assert_eq!(ids, [ENTITY_ACTION], "only the sneak packet");
    assert_eq!(moved, [0.0, 0.0]);

    send(&out, camera(OWN_ID));
    pump_until(&mut app, |a| a.game.camera.is_none());
    let (ids, moved) = tick(&mut app, &rx);
    assert_eq!(ids, [POSE]);
    assert_ne!(moved, [0.0, 0.0]);
}

#[test]
fn position_packet_keeps_ground_state_and_echoes_wrapped_yaw_off_ground() {
    let (port, rx, out) = server();
    let mut app = joined_app(port, &out, &rx);
    let mut chunk = world::chunk::Chunk::new(0, 0);
    chunk.set_block(0, 63, 0, 1 << 4);
    app.game.store.insert(chunk);
    hold_forward(&mut app);
    let p = app.game.player.as_mut().unwrap();
    p.on_ground = true;
    p.vel = [0.1, 0.2, 0.3];
    let vx = p.vel[0];
    let pos_look = |flags| {
        cb::PlayerPositionAndLook {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            yaw: 725.0,
            pitch: 10.0,
            flags,
        }
        .into()
    };
    send(&out, pos_look(0b1)); // x relative
    pump_until(&mut app, |a| a.game.player.as_ref().unwrap().yaw == 5.0);
    let frames = sent_frames(&mut app, &rx);
    let ids: Vec<u8> = frames.iter().map(|f| f[0]).collect();
    assert_eq!(ids, [POSE], "exactly one C06 echo");
    let f = &frames[0];
    let yaw = f32::from_be_bytes(f[25..29].try_into().unwrap());
    assert_eq!(yaw, 5.0);
    assert_eq!(f[33], 0, "echo has onGround = false");
    let p = app.game.player.as_ref().unwrap();
    assert!(p.on_ground, "the player keeps its own flag");
    assert_eq!(p.vel[0], vx, "relative x keeps its velocity");
    assert_eq!((p.vel[1], p.vel[2]), (0.0, 0.0));
}
