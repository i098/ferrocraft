//! Item use, rod swing, the inventory key and the join packets, checked as the bytes the client sends.
use super::camera_packets::{idle_app, join_app, pump_until, send, sent_frames, server, OWN_ID};
use crate::app::App;
use ferrocraft_protocol::play::clientbound as cb;
use std::sync::mpsc::Receiver;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use winit::keyboard::KeyCode;
use world::inventory::ItemStack;

const USE_ITEM: u8 = 0x08;
const SWING: u8 = 0x0A;
const RELEASE: u8 = 0x07;
const ENTITY_ACTION: u8 = 0x0B;
const CLIENT_SETTINGS: u8 = 0x15;
const CLIENT_STATUS: u8 = 0x16;
const PLUGIN_MESSAGE: u8 = 0x17;

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

/// `Minecraft.runTick`: the inventory key sends C16 action 2 before the screen opens; on a horse
/// only the horse inventory request goes out.
#[test]
fn inventory_key_sends_the_status_then_opens() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    ids(&mut app, &rx);
    app.input.on_key(KeyCode::KeyE, true, None);
    app.handle_actions();
    let frames = sent_frames(&mut app, &rx);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0][..], [CLIENT_STATUS, 2]);
    assert!(app.container.is_some());
}

#[test]
fn inventory_key_on_a_horse_sends_only_the_horse_request() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    app.game.vehicle = Some(super::camera_packets::OTHER_ID);
    ids(&mut app, &rx);
    app.input.on_key(KeyCode::KeyE, true, None);
    app.handle_actions();
    let frames = sent_frames(&mut app, &rx);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0][..], [ENTITY_ACTION, OWN_ID as u8, 6, 0]);
    assert!(app.container.is_none());
}

/// `handleJoinGame`: `syncClientSettings` (C15), then `MC|Brand` (C17).
#[test]
fn join_sends_client_settings_before_the_brand() {
    let (port, rx, out) = server();
    let mut app = join_app(port, &out);
    let sent = ids(&mut app, &rx);
    let at = |id| sent.iter().position(|&i| i == id).unwrap();
    assert_eq!(sent.iter().filter(|&&i| i == PLUGIN_MESSAGE).count(), 1);
    assert!(at(CLIENT_SETTINGS) < at(PLUGIN_MESSAGE), "{sent:?}");
}

/// Issue #715: vanilla 1.8.9 sends C15 on join, settings save and language change, not on a resource pack reload
/// (`refreshResources` -> `apply_settings`).
#[test]
fn apply_settings_sends_no_client_settings_but_save_and_language_send_one() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    ids(&mut app, &rx);
    app.apply_settings();
    let reload = ids(&mut app, &rx);
    assert!(!reload.contains(&CLIENT_SETTINGS), "{reload:?}");
    for effect in [
        gui::screens::Effect::SettingsSaved,
        gui::screens::Effect::LanguageChanged,
    ] {
        app.run_effects(vec![effect]);
        let sent = ids(&mut app, &rx);
        assert_eq!(
            sent.iter().filter(|&&i| i == CLIENT_SETTINGS).count(),
            1,
            "{sent:?}"
        );
    }
}

/// Issue #721: F3+H, F3+F and F3+P end in `saveOptions`, which sends C15.
#[test]
fn f3_option_combos_send_client_settings() {
    use input_audio::DebugCombo;
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    ids(&mut app, &rx);
    for combo in [
        DebugCombo::AdvancedTooltips,
        DebugCombo::RenderDistance,
        DebugCombo::PauseOnLostFocus,
    ] {
        app.debug_combo(combo);
        let sent = ids(&mut app, &rx);
        assert_eq!(
            sent.iter().filter(|&&i| i == CLIENT_SETTINGS).count(),
            1,
            "{combo:?} {sent:?}"
        );
    }
}
