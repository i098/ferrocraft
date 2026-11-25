//! The packets `EntityPlayerSP.onUpdate`, `onUpdateWalkingPlayer` and `updateEntityActionState` send each tick
//! (including while the camera is another entity), and the `handlePlayerMove` reply to S08.
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
const PLAYER: u8 = 0x03;
const POS: u8 = 0x04;
const LOOK: u8 = 0x05;
const POSE: u8 = 0x06;
const STEER: u8 = 0x0C;
const ENTITY_ACTION: u8 = 0x0B;
pub(super) const OWN_ID: i32 = 1;
pub(super) const OTHER_ID: i32 = 7;

/// A server that accepts the login, sends every packet queued on the returned sender, and reports the
/// id of every packet the client sends after the login.
pub(super) fn server() -> (u16, Receiver<Vec<u8>>, Sender<Vec<u8>>) {
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
pub(super) fn pump_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < end, "server packet never applied");
        app.pump_network();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Packets arrive in order, so a chat marks the end of everything the client sent before it.
pub(super) fn sent_frames(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
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

pub(super) fn send(out: &Sender<Vec<u8>>, p: cb::Packet) {
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

pub(super) fn joined_app(port: u16, out: &Sender<Vec<u8>>, rx: &Receiver<Vec<u8>>) -> App {
    let mut app = join_app(port, out);
    sent_so_far(&mut app, rx); // drop what the client sent while joining
    app
}

/// A client that has applied `JoinGame` and a spawned mob; what it sent since is still unread.
pub(super) fn join_app(port: u16, out: &Sender<Vec<u8>>) -> App {
    let mut app = App::new(Args {
        username: "Ferro".into(),
        auth: None,
        assets_dir: None,
        server: None,
        game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
        ..Default::default()
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
    app
}

#[test]
fn endermite_mob_emits_portal_particles_each_tick_and_other_mobs_do_not() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.tick_once();
    assert_eq!(app.particles.len(), 0, "kind-100 mob makes none");
    let mut mite = spawn_mob();
    mite.entity_id = VarInt(OTHER_ID + 1);
    mite.kind = 67;
    send(&out, mite.into());
    pump_until(&mut app, |a| a.game.entities.get(OTHER_ID + 1).is_some());
    app.tick_once();
    assert_eq!(app.particles.len(), 2);
    app.tick_once();
    assert_eq!(app.particles.len(), 4, "two more every tick");
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
    assert_eq!(ids, [POS]);
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

/// A player standing on a block in a loaded chunk, with the screen closed and no key held.
pub(super) fn idle_app(port: u16, out: &Sender<Vec<u8>>, rx: &Receiver<Vec<u8>>) -> App {
    let mut app = joined_app(port, out, rx);
    let mut chunk = world::chunk::Chunk::new(0, 0);
    chunk.set_block(0, 63, 0, 1 << 4);
    app.game.store.insert(chunk);
    app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
    let m = metrics!(app);
    app.ui.close(&m);
    app.input.set_screen_open(false);
    app
}

/// `onUpdateWalkingPlayer`: the packet type follows what changed since the last position and look sent.
#[test]
fn movement_packet_type_follows_what_changed() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    let ids = |app: &mut App| tick(app, &rx).0;
    assert_eq!(ids(&mut app), [POS], "first tick reports the position");
    for n in 0..20 {
        assert_eq!(ids(&mut app), [PLAYER], "still tick {n}");
    }
    assert_eq!(ids(&mut app), [POS], "the 20-tick refresh");
    assert_eq!(ids(&mut app), [PLAYER]);
    app.game.player.as_mut().unwrap().yaw = 30.0;
    assert_eq!(ids(&mut app), [LOOK]);
    app.input.on_key(KeyCode::KeyW, true, None);
    assert_eq!(ids(&mut app), [POS], "walking");
    app.game.player.as_mut().unwrap().yaw = 60.0;
    assert_eq!(ids(&mut app), [POSE]);
}

/// `onUpdateWalkingPlayer` sends the sprint change before the sneak change.
#[test]
fn sprint_goes_before_sneak() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    app.game.player.as_mut().unwrap().sprinting = true;
    app.send_state();
    let actions: Vec<_> = sent_frames(&mut app, &rx)
        .iter()
        .map(|f| (f[0], f[2]))
        .collect();
    assert_eq!(actions, [(ENTITY_ACTION, 3), (ENTITY_ACTION, 0)]);
}

/// While riding `onUpdate` sends the look and the steering, never an `EntityAction`.
#[test]
fn riding_sends_no_entity_action() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    app.game.vehicle = Some(OTHER_ID);
    let (ids, _) = tick(&mut app, &rx);
    assert_eq!(ids, [LOOK, STEER]);
}

/// Jar `bew.t_`/`p` (EntityPlayerSP.onUpdate and onUpdateWalkingPlayer): riding sends no `C0B`; after
/// the dismount the sneak state that changed meanwhile goes out then.
#[test]
fn sneak_released_while_riding_sends_stop_sneak_after_dismount() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    let actions = |app: &mut App| -> Vec<(u8, u8)> {
        let frames = sent_frames(app, &rx);
        let pick = |f: &Vec<u8>| (f[0], f[f.len() - 2]);
        frames
            .iter()
            .filter(|f| f[0] == ENTITY_ACTION || f[0] == STEER)
            .map(pick)
            .collect()
    };
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    app.tick_once();
    assert_eq!(
        actions(&mut app),
        [(ENTITY_ACTION, 0)],
        "start sneak on foot"
    );
    app.game.vehicle = Some(OTHER_ID);
    app.input.on_key(KeyCode::ShiftLeft, false, None);
    app.tick_once();
    let riding = actions(&mut app);
    assert!(
        riding.iter().all(|a| a.0 == STEER),
        "no C0B while mounted: {riding:?}"
    );
    app.game.vehicle = None;
    app.tick_once();
    assert_eq!(
        actions(&mut app),
        [(ENTITY_ACTION, 1)],
        "stop sneak after the dismount"
    );
}

/// `C0C` carries `moveForward` after `onLivingUpdate`: 1.0 from the key times 0.98.
#[test]
fn steer_forward_is_scaled_by_0_98() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.input.on_key(KeyCode::KeyW, true, None);
    app.game.vehicle = Some(OTHER_ID);
    app.tick_once();
    let frames = sent_frames(&mut app, &rx);
    let steer = frames.iter().find(|f| f[0] == STEER).expect("C0C sent");
    let forward = f32::from_be_bytes(steer[5..9].try_into().unwrap());
    assert_eq!(forward, 0.98_f32);
}

/// `handlePlayerMove` echoes `C06` without touching the sent position, so the next tick reports it.
#[test]
fn packet_after_a_teleport_reports_the_new_position() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    tick(&mut app, &rx);
    assert_eq!(tick(&mut app, &rx).0, [PLAYER]);
    let tp = cb::PlayerPositionAndLook {
        x: 5.5,
        y: 64.0,
        z: 0.5,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    };
    send(&out, tp.into());
    pump_until(&mut app, |a| a.game.player.as_ref().unwrap().pos[0] == 5.5);
    assert_eq!(sent_so_far(&mut app, &rx), [POSE], "the echo");
    assert_eq!(tick(&mut app, &rx).0, [POS]);
}

/// `ClientChunkCache.hasChunk` always returns true, so `onUpdate` sends over an unloaded chunk.
#[test]
fn packets_are_sent_over_an_unloaded_chunk() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.player.as_mut().unwrap().pos = [100.5, 64.0, 100.5];
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    let ids = |app: &mut App| tick(app, &rx).0;
    assert_eq!(
        ids(&mut app),
        [ENTITY_ACTION, POS],
        "start sneak, then the position"
    );
    for n in 0..3 {
        assert_eq!(ids(&mut app), [PLAYER], "still tick {n}");
    }
    app.game.player.as_mut().unwrap().yaw = 30.0;
    assert_eq!(ids(&mut app), [LOOK]);
}

/// `onDeathUpdate` calls `setDead()` at `deathTime == 20` and `World.updateEntities` skips the player
/// from the next tick on, so a dead rider sends no look, steer or horse jump either.
#[test]
fn dead_rider_sends_nothing_after_twenty_death_ticks() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.vehicle = Some(OTHER_ID);
    app.game.health = 0.0;
    for _ in 0..21 {
        app.tick_once();
    }
    sent_so_far(&mut app, &rx);
    app.game.horse_boost = Some(50);
    assert!(app.game.player_removed());
    let (ids, _) = tick(&mut app, &rx);
    assert_eq!(ids, Vec::<u8>::new());
}

/// Issue #706: vanilla sends C03-C06 in each of the 20 updates up to and including the one that calls
/// `setDead()` (`EntityPlayerSP.onUpdate` runs `onUpdateWalkingPlayer` after `super.onUpdate()`), then none.
#[test]
fn dead_player_sends_movement_for_twenty_ticks_only() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.health = 0.0;
    let moves = (0..30)
        .map(|_| {
            tick(&mut app, &rx)
                .0
                .iter()
                .filter(|i| (3..=6).contains(*i))
                .count()
        })
        .sum::<usize>();
    assert_eq!(moves, 20);
}
