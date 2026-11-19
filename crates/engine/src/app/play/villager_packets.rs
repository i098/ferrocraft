//! A right click that opens a villager window is replayed on the window (`Mouse.setGrabbed(false)`
//! in `displayGuiScreen`): half a stack lands on the cursor. Checked as the bytes the client sends.
use super::camera_packets::{idle_app, pump_until, send, sent_frames, server};
use super::click_packets::hold_right_button;
use crate::app::App;
use ferrocraft_protocol::codec::{ItemStack, Slot, Vec16};
use ferrocraft_protocol::play::clientbound as cb;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

const CLICK_WINDOW: u8 = 0x0E;

fn emeralds(count: i8) -> Slot {
    Some(ItemStack {
        id: 388,
        count,
        damage: 0,
        nbt: None,
    })
}

/// The villager window opens with 5 emeralds in trade slot 0, the pointer over that slot.
fn open_villager(app: &mut App, out: &std::sync::mpsc::Sender<Vec<u8>>) {
    // Trade slot 0 is at (36, 53) in a 176x166 window; centre the window so that slot sits at (0, 0).
    app.ui.width = 104;
    app.ui.height = 60;
    send(
        out,
        cb::OpenWindow {
            window_id: 3,
            window_type: "minecraft:villager".into(),
            title: "{\"text\":\"Villager\"}".into(),
            slots: 3,
            horse_entity_id: None,
        }
        .into(),
    );
    let mut items = vec![None; 39];
    items[0] = emeralds(5);
    send(
        out,
        cb::WindowItems {
            window_id: 3,
            slots: Vec16(items),
        }
        .into(),
    );
    pump_until(app, |a| {
        a.container.is_some() && a.game.inv.slot(0).as_ref().is_some()
    });
    app.tick_once();
}

fn click_frames(app: &mut App, rx: &std::sync::mpsc::Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
    sent_frames(app, rx)
        .into_iter()
        .filter(|f| f[0] == CLICK_WINDOW)
        .collect()
}

#[test]
fn right_click_held_while_the_villager_window_opens_picks_up_half_the_stack() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold_right_button(&mut app);
    open_villager(&mut app, &out);
    let clicks = click_frames(&mut app, &rx);
    // window 3, slot 0, button 1 (right), mode 0 (pick up)
    assert_eq!(clicks.len(), 1, "{clicks:?}");
    let f = &clicks[0];
    assert_eq!(
        (f[1], i16::from_be_bytes([f[2], f[3]]), f[4], f[7]),
        (3, 0, 1, 0)
    );
    assert_eq!(app.game.inv.slot(0).as_ref().unwrap().count, 2);
    assert_eq!(app.game.inv.cursor.as_ref().unwrap().count, 3);
}

#[test]
fn right_click_released_before_the_window_opens_clicks_nothing() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold_right_button(&mut app);
    app.input.on_window_event(&WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Released,
        button: MouseButton::Right,
    });
    open_villager(&mut app, &out);
    assert!(click_frames(&mut app, &rx).is_empty());
    assert_eq!(app.game.inv.slot(0).as_ref().unwrap().count, 5);
    assert!(app.game.inv.cursor.is_none());
}

#[test]
fn right_button_lost_to_alt_tab_is_not_replayed_on_the_next_window() {
    let (port, rx, out) = server();
    let mut app = idle_app(port, &out, &rx);
    hold_right_button(&mut app);
    app.input.on_window_event(&WindowEvent::Focused(false));
    open_villager(&mut app, &out);
    assert!(click_frames(&mut app, &rx).is_empty());
    assert_eq!(app.game.inv.slot(0).as_ref().unwrap().count, 5);
}
