//! Item use, rod swing, the inventory key and the join packets, checked as the bytes the client sends.
use super::camera_packets::{idle_app, pump_until, send, sent_frames, server, OWN_ID};
use crate::app::App;
use ferrocraft_protocol::play::clientbound as cb;
use std::sync::mpsc::Receiver;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use world::inventory::ItemStack;

const USE_ITEM: u8 = 0x08;
const SWING: u8 = 0x0A;
const RELEASE: u8 = 0x07;

fn hold(app: &mut App, id: i16) {
    let slot = 36 + usize::from(app.game.inv.held);
    app.game.inv.player[slot] = Some(ItemStack {
        id,
        count: 1,
        damage: 0,
        nbt: None,
    });
}

/// The first click only regains the mouse grab (`setIngameFocus`), so click twice.
fn hold_right_button(app: &mut App) {
    let click = |pressed| WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        },
        button: MouseButton::Right,
    };
    for pressed in [true, false, true] {
        app.input.on_window_event(&click(pressed));
    }
}

fn ids(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<u8> {
    sent_frames(app, rx).iter().map(|f| f[0]).collect()
}

/// `EntityPlayerSP.isUsingItem` stays true while `itemInUseCount` is below 0, until S19 status 9.
#[test]
fn eating_holds_the_use_until_status_9() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.food = 10;
    hold(&mut app, 297);
    hold_right_button(&mut app);
    ids(&mut app, &rx);
    let mut c08 = 0;
    for _ in 0..40 {
        app.tick_once();
        c08 += ids(&mut app, &rx)
            .iter()
            .filter(|&&i| i == USE_ITEM)
            .count();
    }
    assert_eq!(c08, 1, "one C08 for the item, none after 32 ticks");
    assert!(app.in_use, "still in use at tick 40");
    assert!(app.use_ticks < 0);

    send(
        &out,
        cb::EntityStatus {
            entity_id: OWN_ID,
            status: 9,
        }
        .into(),
    );
    pump_until(&mut app, |a| a.game.finished_use);
    app.tick_once();
    assert!(!app.in_use, "status 9 ends the use");
    ids(&mut app, &rx);
    for _ in 0..4 {
        app.tick_once();
    }
    let sent = ids(&mut app, &rx);
    assert_eq!(sent.iter().filter(|&&i| i == USE_ITEM).count(), 1);
    assert!(!sent.contains(&RELEASE));
}

/// `ItemFishingRod.onItemRightClick` swings for the cast and the reel: C08, then C0A.
#[test]
fn rod_click_sends_use_then_swing_and_swings_the_arm() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold(&mut app, 346);
    ids(&mut app, &rx);
    app.use_item();
    assert_eq!(ids(&mut app, &rx), [USE_ITEM, SWING]);
    app.tick_once();
    app.tick_once(); // the first tick moves the swing counter from -1 to 0
    assert!(app.game.swing.active(), "the first-person arm swings");
    ids(&mut app, &rx);
    hold(&mut app, 267);
    app.use_item();
    assert_eq!(ids(&mut app, &rx), [USE_ITEM], "a sword does not swing");
}
