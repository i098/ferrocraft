//! `LocalClientPlayerEntity.sendMovementToServer` and `tickMovement` while the camera is another entity.
use crate::app::App;
use crate::args::Args;
use crate::link::Link;
use ferrocraft_net_client::Session;
use ferrocraft_protocol::frame::{FrameDecoder, FrameEncoder};
use ferrocraft_protocol::{login, PacketSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use winit::keyboard::KeyCode;

const CHAT: u8 = 0x01;
const POSITION_PACKETS: [u8; 4] = [0x03, 0x04, 0x05, 0x06];
const ENTITY_ACTION: u8 = 0x0B;

/// A server that accepts the login and reports the id of every packet the client sends after it.
fn server() -> (u16, Receiver<u8>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf);
        let mut out = Vec::new();
        FrameEncoder::default()
            .encode(
                &login::clientbound::Packet::from(login::clientbound::LoginSuccess {
                    uuid: "00000000-0000-0000-0000-000000000001".into(),
                    username: "Ferro".into(),
                })
                .encode(),
                &mut out,
            )
            .unwrap();
        s.write_all(&out).unwrap();
        let mut dec = FrameDecoder::default();
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            dec.feed(&buf[..n]);
            while let Ok(Some(f)) = dec.next_frame() {
                // Id 0 is the login start that the first read may have left behind.
                if f[0] != 0 {
                    let _ = tx.send(f[0]);
                }
            }
        }
    });
    (port, rx)
}

/// Runs one tick with `W` and sneak held and returns the packet ids sent, plus the x/z the player moved.
fn one_tick(camera: Option<i32>) -> (Vec<u8>, [f64; 2]) {
    let (port, rx) = server();
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
    let m = metrics!(app);
    app.ui.close(&m);
    let mut chunk = world::chunk::Chunk::new(0, 0);
    chunk.set_block(0, 63, 0, 1 << 4);
    app.game.store.insert(chunk);
    app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
    app.game.camera = camera;
    app.input.set_screen_open(false);
    app.input.on_key(KeyCode::KeyW, true, None);
    app.input.on_key(KeyCode::ShiftLeft, true, None);
    assert!(!app.ui.is_open(), "screen open");
    app.tick_once();
    // Packets arrive in order, so the chat marks the end of this tick's packets.
    app.link.session().unwrap().send_chat("end").unwrap();
    let mut ids = Vec::new();
    loop {
        let id = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if id == CHAT {
            break;
        }
        ids.push(id);
    }
    let p = app.game.player.as_ref().unwrap();
    (ids, [p.pos[0] - 0.5, p.pos[2] - 0.5])
}

#[test]
fn camera_on_another_entity_sends_no_position_packet_and_does_not_move() {
    let (ids, moved) = one_tick(Some(7));
    assert_eq!(ids, [ENTITY_ACTION], "only the sneak packet");
    assert_eq!(moved, [0.0, 0.0]);
}

#[test]
fn camera_on_the_local_player_sends_the_pose_and_moves() {
    let (ids, moved) = one_tick(None);
    assert_eq!(ids, [ENTITY_ACTION, POSITION_PACKETS[3]]);
    assert_ne!(moved, [0.0, 0.0]);
}
