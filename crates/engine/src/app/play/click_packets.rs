//! Left and right clicks run in the tick, checked as the bytes the client sends.
use super::camera_packets::{idle_app, sent_frames, server};
use crate::app::App;
use std::sync::mpsc::Receiver;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use input_audio::Action;
use world::entities::EntityEvent;
use world::inventory::ItemStack;

const USE_ENTITY: u8 = 0x02;
const USE_ITEM: u8 = 0x08;
const SWING: u8 = 0x0A;
/// Ids of the packets that clicks send; the tick also sends movement.
const CLICK_IDS: [u8; 3] = [USE_ENTITY, USE_ITEM, SWING];
/// C02 action (`ATTACK`) is the second byte after the entity id VarInt.
const ATTACK: u8 = 1;

pub(super) fn hold(app: &mut App, id: i16) {
    let slot = 36 + usize::from(app.game.inv.held);
    app.game.inv.player[slot] = Some(ItemStack {
        id,
        count: 1,
        damage: 0,
        nbt: None,
    });
}

/// Yaw 0 faces +z: the mob stands about three blocks ahead of the eye.
pub(super) fn spawn_ahead(app: &mut App, id: i32, kind: u8) {
    app.game.entities.apply(EntityEvent::SpawnMob {
        id,
        kind,
        pos: [16, 2048, 96],
        yaw: 0,
        pitch: 0,
        head_yaw: 0,
        vel: [0; 3],
        flags: None,
        name: None,
        health: None,
        size: Default::default(),
    });
}

pub(super) fn hold_right_button(app: &mut App) {
    hold_button(app, MouseButton::Right);
}

/// The first click only regains the mouse grab (`setIngameFocus`), so click twice.
fn hold_button(app: &mut App, button: MouseButton) {
    let click = |pressed| WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        },
        button,
    };
    for pressed in [true, false, true] {
        app.input.on_window_event(&click(pressed));
    }
}

/// The packets the clicks sent, as `(id, C02 action)`; the action is 255 for other packets.
pub(super) fn clicks(app: &mut App, rx: &Receiver<Vec<u8>>) -> Vec<(u8, u8)> {
    sent_frames(app, rx)
        .iter()
        .filter(|f| CLICK_IDS.contains(&f[0]))
        .map(|f| (f[0], if f[0] == USE_ENTITY { f[2] } else { 255 }))
        .collect()
}

/// `Minecraft.runTick`: the attack click runs first (C0A, C02), then the use click (C08).
#[test]
fn attack_and_use_in_one_tick_send_the_attack_before_the_use() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold(&mut app, 267);
    spawn_ahead(&mut app, 9, 54);
    clicks(&mut app, &rx);
    app.attack_presses = 1;
    app.use_presses = 1;
    app.tick_once();
    let sent = clicks(&mut app, &rx);
    let at = |p| sent.iter().position(|s| *s == p).unwrap();
    assert!(
        at((SWING, 255)) < at((USE_ENTITY, ATTACK)),
        "swing before attack"
    );
    assert!(at((USE_ENTITY, ATTACK)) < at((USE_ITEM, 255)), "{sent:?}");
}

/// `rightClickDelayTimer` is set to 4 by the click and counts down at the start of each tick: the held
/// button repeats at T, T+4 and T+8.
#[test]
fn held_right_click_repeats_every_four_ticks() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold(&mut app, 1);
    hold_right_button(&mut app);
    clicks(&mut app, &rx);
    app.use_presses = 1;
    let mut uses = vec![];
    for tick in 0..9 {
        app.tick_once();
        if clicks(&mut app, &rx).contains(&(USE_ITEM, 255)) {
            uses.push(tick);
        }
    }
    assert_eq!(uses, [0, 4, 8]);
}

fn release(app: &mut App, button: MouseButton) {
    app.input.on_window_event(&WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Released,
        button,
    });
}

/// `Minecraft.runTick` counts `leftClickCounter` down every tick and zeroes it when the button is up,
/// also while an item is in use: a miss, three ticks of blocking with the button up, then a click attacks.
#[test]
fn the_miss_cooldown_runs_while_an_item_is_in_use() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold(&mut app, 267);
    hold_button(&mut app, MouseButton::Left);
    clicks(&mut app, &rx);
    app.attack_presses = 1;
    app.tick_once();
    assert_eq!(clicks(&mut app, &rx), [(SWING, 255)], "the miss swings");

    hold_right_button(&mut app);
    app.in_use = true;
    app.use_ticks = 72_000;
    release(&mut app, MouseButton::Left);
    for _ in 0..3 {
        app.tick_once();
    }
    release(&mut app, MouseButton::Right);
    app.end_use();
    spawn_ahead(&mut app, 9, 54);
    clicks(&mut app, &rx);
    app.attack_presses = 1;
    app.tick_once();
    assert_eq!(
        clicks(&mut app, &rx),
        [(SWING, 255), (USE_ENTITY, ATTACK)],
        "the click attacks"
    );
}

/// `ItemFrameEntity.interact` always returns true: the click sends C02 INTERACT_AT and INTERACT, then
/// `Minecraft.doUse` stops, so a sword starts no block and no C08 goes out.
#[test]
fn a_click_on_an_item_frame_sends_no_use_item() {
    const INTERACT: u8 = 0;
    const INTERACT_AT: u8 = 2;
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold(&mut app, 267);
    // A frame facing south, centred at (0.5, 65.5, 2.5): on the line of sight.
    app.game.entities.apply(EntityEvent::SpawnObject {
        id: 9,
        kind: 71,
        pos: [16, 2096, 80],
        yaw: 0,
        pitch: 0,
        vel: [0; 3],
        data: 0,
    });
    clicks(&mut app, &rx);
    app.use_presses = 1;
    app.tick_once();
    assert_eq!(
        clicks(&mut app, &rx),
        [(USE_ENTITY, INTERACT_AT), (USE_ENTITY, INTERACT)]
    );
    assert!(!app.in_use);
}

/// `Minecraft.runTick` skips the drop key for a spectator and `sendUseItem` returns false before any
/// packet.
#[test]
fn a_spectator_sends_no_drop_and_no_air_use() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.gamemode = 3;
    hold(&mut app, 267);
    sent_frames(&mut app, &rx);
    app.game_action(Action::Drop { whole_stack: false });
    app.game_action(Action::Drop { whole_stack: true });
    app.use_item();
    assert!(sent_frames(&mut app, &rx).is_empty());
    assert!(!app.in_use);
}

