//! `GuiIngame.renderGameOverlay`: the in-game HUD in vanilla draw order.

use crate::chat::{self, Chat};
use crate::debug;
use crate::toolkit::{Gui, ICONS, WIDGETS};
use render::Blend;

const VIGNETTE: &str = "textures/misc/vignette.png";

/// HUD state that lives across frames and advances on the 20 TPS clock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    pub chat: Chat,
    /// `updateCounter`: ticks since the HUD was made; chat fade reads it.
    pub update_counter: i32,
    /// Hotbar slot 0..9 (`inventory.currentItem`).
    pub selected_slot: i32,
    record: String,
    record_up_for: i32,
    /// `prevVignetteBrightness`, eased toward the light level each frame.
    vignette: f32,
}

/// Per-frame inputs of the overlay.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame<'a> {
    pub partial_ticks: f32,
    /// The text being typed when the chat screen is open.
    pub chat_input: Option<&'a str>,
    /// F3 left and right columns when the debug screen is shown.
    pub debug: Option<(&'a [String], &'a [String])>,
    /// `reducedDebugInfo`: F3 keeps the crosshair.
    pub reduced_debug: bool,
    /// `Minecraft.isFancyGraphicsEnabled()`.
    pub fancy: bool,
    /// `thePlayer.getBrightness(partialTicks)`, 0..=1.
    pub brightness: f32,
    /// The camera is the player (not spectating another entity): the hotbar is drawn.
    pub player_view: bool,
}

impl Hud {
    /// `updateTick`.
    pub fn tick(&mut self) {
        if self.record_up_for > 0 {
            self.record_up_for -= 1;
        }
        self.update_counter += 1;
    }

    /// `setRecordPlaying(message, false)`: the action bar text for 3 seconds.
    pub fn set_action_bar(&mut self, text: String) {
        self.record = text;
        self.record_up_for = 60;
    }

    /// `renderGameOverlay`.
    pub fn draw(&mut self, gui: &mut Gui, f: &Frame) {
        let (w, h) = (gui.scale.width, gui.scale.height);
        gui.set_blend(Blend::Alpha);
        if f.fancy {
            self.draw_vignette(gui, f.brightness);
        }
        if f.player_view {
            gui.set_color(0xFFFF_FFFF);
            gui.blit(WIDGETS, w / 2 - 91, h - 22, 0, 0, 182, 22);
            let x = w / 2 - 91 - 1 + self.selected_slot * 20;
            gui.blit(WIDGETS, x, h - 22 - 1, 0, 22, 24, 22);
        }
        gui.set_color(0xFFFF_FFFF);
        if f.debug.is_none() || f.reduced_debug {
            gui.set_blend(Blend::Invert);
            gui.blit(ICONS, w / 2 - 7, h / 2 - 7, 0, 0, 16, 16);
        }
        gui.set_blend(Blend::Alpha);
        if let Some((left, right)) = f.debug {
            debug::draw(gui, left, right);
        }
        if self.record_up_for > 0 {
            let t = self.record_up_for as f32 - f.partial_ticks;
            let alpha = ((t * 255.0 / 20.0) as i32).min(255);
            if alpha > 8 {
                gui.push();
                gui.translate((w / 2) as f32, (h - 68) as f32);
                let x = -gui.width(&self.record) / 2;
                gui.text_plain(
                    &self.record,
                    x as f32,
                    -4.0,
                    0xFFFFFF | (alpha as u32) << 24,
                );
                gui.pop();
            }
        }
        let open = f.chat_input.is_some();
        gui.push();
        gui.translate(0.0, (h - 48) as f32);
        self.chat.draw(gui, self.update_counter, open);
        gui.pop();
        if let Some(text) = f.chat_input {
            chat::draw_input(gui, text, self.update_counter);
        }
        gui.set_color(0xFFFF_FFFF);
    }

    /// `renderVignette`: darkens the screen edges more the darker the player's spot is.
    fn draw_vignette(&mut self, gui: &mut Gui, brightness: f32) {
        let level = (1.0 - brightness).clamp(0.0, 1.0);
        self.vignette += (level - self.vignette) * 0.01;
        let v = (self.vignette.clamp(0.0, 1.0) * 255.0) as u32;
        let (w, h) = (gui.scale.width as f32, gui.scale.height as f32);
        gui.set_blend(Blend::Darken);
        gui.blit_tinted(
            VIGNETTE,
            0.0,
            0.0,
            w,
            h,
            [0.0, 0.0, 1.0, 1.0],
            0xFF00_0000 | (v * 0x010101),
        );
        gui.set_blend(Blend::Alpha);
    }
}

/// Top-left of hotbar slot `i`'s item (`renderHotbarItem`).
pub fn hotbar_item_pos(gui: &Gui, i: i32) -> (i32, i32) {
    (
        gui.scale.width / 2 - 90 + i * 20 + 2,
        gui.scale.height - 16 - 3,
    )
}
