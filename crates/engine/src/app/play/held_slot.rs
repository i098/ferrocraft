//! `handleSelectSlot` and `syncCurrentPlayItem`: an S09 slot echoes as one C09 on the next tick, and
//! a slot outside 0 to 8 does nothing.
use super::camera_packets::{joined_app, pump_until, send, sent_frames, server};
use crate::app::App;
use ferrocraft_protocol::play::clientbound as cb;
use std::sync::mpsc::Receiver;

const HELD_ITEM_CHANGE: u8 = 0x09;

/// The C09 frames among everything the client sent; the tick also sends movement.
fn held_frames(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut frames = sent_frames(app, rx);
    frames.retain(|f| f[0] == HELD_ITEM_CHANGE);
    frames
}

fn s09(slot: i8) -> cb::Packet {
    cb::HeldItemChange { slot }.into()
}

#[test]
fn server_slot_echoes_once_on_the_next_tick() {
    let (port, rx, out) = server();
    let mut app = joined_app(port, &out, &rx);
    app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
    send(&out, s09(3));
    pump_until(&mut app, |a| a.game.inv.held == 3);
    assert!(
        held_frames(&mut app, &rx).is_empty(),
        "no packet before the tick"
    );
    app.tick_once();
    let frames = held_frames(&mut app, &rx);
    assert_eq!(frames, [vec![HELD_ITEM_CHANGE, 0, 3]]);
    app.tick_once();
    assert!(held_frames(&mut app, &rx).is_empty());
}

#[test]
fn out_of_range_server_slots_change_nothing() {
    let (port, rx, out) = server();
    let mut app = joined_app(port, &out, &rx);
    app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
    send(&out, s09(2));
    pump_until(&mut app, |a| a.game.inv.held == 2);
    app.tick_once();
    sent_frames(&mut app, &rx);
    app.game.held_replaced = false;
    send(&out, s09(9));
    send(&out, s09(-1));
    send(&out, cb::ServerDifficulty { difficulty: 2 }.into());
    pump_until(&mut app, |a| a.ui.difficulty == Some(2));
    assert_eq!(app.game.inv.held, 2);
    assert!(!app.game.held_replaced);
    app.tick_once();
    assert!(held_frames(&mut app, &rx).is_empty());
}
