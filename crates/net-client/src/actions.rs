//! Inventory, item use and settings packets the client sends.

use crate::{send, Session};
use ferrocraft_protocol::codec::Position;
use ferrocraft_protocol::play::serverbound as sb;
use std::io;
use world::inventory::Slot;

impl Session {
    /// C0E: `item` is the stack the click took, as `Container.slotClick` returned it.
    pub fn click_window(
        &mut self,
        window: u8,
        slot: i16,
        button: i8,
        action: i16,
        mode: i8,
        item: &Slot,
    ) -> io::Result<()> {
        let p = sb::ClickWindow {
            window_id: window,
            slot,
            button,
            action,
            mode,
            clicked_item: item.clone(),
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C0F: the answer to a rejected `Transaction`.
    pub fn confirm_transaction(
        &mut self,
        window: u8,
        action: i16,
        accepted: bool,
    ) -> io::Result<()> {
        let p = sb::ConfirmTransaction {
            window_id: window,
            action,
            accepted,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C0D.
    pub fn close_window(&mut self, window: u8) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::CloseWindow { window_id: window }),
        )
    }

    /// C10: sets a window 0 slot in creative mode; slot -1 drops `item`.
    pub fn creative_slot(&mut self, slot: i16, item: &Slot) -> io::Result<()> {
        let p = sb::CreativeInventoryAction {
            slot,
            item: item.clone(),
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C09: the selected hotbar slot, 0 to 8.
    pub fn held_item_change(&mut self, slot: u8) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::HeldItemChange {
                slot: i16::from(slot),
            }),
        )
    }

    /// C08: pos `[-1, -1, -1]` and face 255 use the held item in the air.
    pub fn place_block(
        &mut self,
        pos: [i32; 3],
        face: u8,
        held: &Slot,
        cursor: [u8; 3],
    ) -> io::Result<()> {
        let [x, y, z] = pos;
        let [cursor_x, cursor_y, cursor_z] = cursor;
        let p = sb::PlayerBlockPlacement {
            location: Position { x, y, z },
            direction: face,
            held_item: held.clone(),
            cursor_x,
            cursor_y,
            cursor_z,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C07 status 3 (whole stack) or 4 (one item) at the origin, face down, as vanilla sends it.
    pub fn drop_item(&mut self, whole_stack: bool) -> io::Result<()> {
        self.send_dig(if whole_stack { 3 } else { 4 }, [0; 3], 0)
    }

    /// C07 status 5: stops drawing a bow or eating.
    pub fn release_use_item(&mut self) -> io::Result<()> {
        self.send_dig(5, [0; 3], 0)
    }

    /// C15: resent after the options change.
    pub fn client_settings(
        &mut self,
        locale: &str,
        view_distance: i8,
        chat_mode: i8,
        chat_colors: bool,
        skin_parts: u8,
    ) -> io::Result<()> {
        let p = sb::ClientSettings {
            locale: locale.into(),
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C11: press button `button` (0 to 2) of the enchanting table.
    pub fn enchant_item(&mut self, window: u8, button: i8) -> io::Result<()> {
        let p = sb::EnchantItem {
            window_id: window,
            enchantment: button,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C17: a custom payload (`MC|ItemName`, `MC|Beacon`, `MC|TrSel`).
    pub fn plugin_message(&mut self, channel: &str, data: Vec<u8>) -> io::Result<()> {
        let p = sb::PluginMessage {
            channel: channel.to_string(),
            data: ferrocraft_protocol::codec::Rest(data),
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C12: the four sign lines as plain text components (`ChatComponentText`).
    pub fn update_sign(&mut self, pos: [i32; 3], lines: &[String; 4]) -> io::Result<()> {
        let [x, y, z] = pos;
        let p = sb::UpdateSign {
            location: Position { x, y, z },
            lines: lines
                .clone()
                .map(|l| serde_json::json!({ "text": l }).to_string()),
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// C13: `flags` 2 is flying.
    pub fn abilities(&mut self, flags: u8, fly_speed: f32, walk_speed: f32) -> io::Result<()> {
        let p = sb::PlayerAbilities {
            flags,
            flying_speed: fly_speed,
            walking_speed: walk_speed,
        };
        send(&self.writer, sb::Packet::from(p))
    }
}
