//! `handleInventoryMenuConfirm`: S32 is answered with C0F only for the player menu and the open window.
use super::camera_packets::{joined_app, pump_until, send, sent_frames, server, OTHER_ID};
use ferrocraft_protocol::codec::VarInt;
use ferrocraft_protocol::play::clientbound as cb;

const CONFIRM: u8 = 0x0F;

fn s32(window_id: u8, action: i16, accepted: bool) -> cb::Packet {
    cb::ConfirmTransaction {
        window_id,
        action,
        accepted,
    }
    .into()
}

#[test]
fn rejected_transaction_gets_c0f_only_for_window_0_and_the_open_window() {
    let (port, rx, out) = server();
    let mut app = joined_app(port, &out, &rx);
    send(&out, s32(5, 1, false)); // a foreign id: no reply
    send(&out, s32(0, 2, false));
    send(&out, s32(0, 3, true)); // accepted: no reply
    send(
        &out,
        cb::OpenWindow {
            window_id: 5,
            window_type: "minecraft:chest".into(),
            title: "{\"text\":\"Chest\"}".into(),
            slots: 27,
            horse_entity_id: None,
        }
        .into(),
    );
    send(&out, s32(5, 4, false));
    send(&out, s32(6, 5, false)); // another foreign id
                                  // Packets apply in order, so the camera change marks the end of the batch.
    send(
        &out,
        cb::Camera {
            camera_id: VarInt(OTHER_ID),
        }
        .into(),
    );
    pump_until(&mut app, |a| a.game.camera == Some(OTHER_ID));
    let frames = sent_frames(&mut app, &rx);
    // C0F: window id, action (i16), accepted (true).
    assert_eq!(
        frames,
        [vec![CONFIRM, 0, 0, 2, 1], vec![CONFIRM, 5, 0, 4, 1]]
    );
}
