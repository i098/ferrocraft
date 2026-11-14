//! `GuiIngame.renderGameOverlay`: the in-game HUD in vanilla draw order.

use crate::bossbar::BossStatus;
use crate::canvas::GuiCanvas;
use crate::chat::{self, Chat};
use crate::debug;
use crate::inventory::{icons::Icons, tooltip};
use crate::overlay::{self, Portal};
use crate::scoreboard::Scoreboard;
use crate::screens::TextField;
use crate::status::{self, Status, StatusState};
use crate::tablist::{Clock, TabList};
use crate::title::TitleState;
use crate::toolkit::{Gui, ICONS, WIDGETS};
use render::Blend;
use world::inventory::{items, ItemStack, Slot};

const VIGNETTE: &str = "textures/misc/vignette.png";

/// HUD state that lives across frames and advances on the 20 TPS clock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    pub chat: Chat,
    /// `updateCounter`: ticks since the HUD was made; chat fade reads it.
    pub update_counter: i32,
    /// Hotbar slot 0..9 (`inventory.currentItem`).
    pub selected_slot: i32,
    pub status_state: StatusState,
    pub tablist: TabList,
    pub scoreboard: Scoreboard,
    pub title: TitleState,
    pub boss: BossStatus,
    pub portal: Portal,
    record: String,
    record_up_for: i32,
    /// `1 - prevVignetteBrightness`, so the derived default starts the vignette at 1.
    vignette_lag: f32,
    /// `highlightingItemStack` and `remainingHighlightTicks`.
    highlight: Option<ItemStack>,
    highlight_ticks: i32,
}

/// The hotbar's items (`renderHotbarItem`).
#[derive(Clone, Copy, Debug)]
pub struct HotbarItems<'a> {
    pub slots: [&'a Slot; 9],
    pub icons: &'a Icons,
    /// `ItemStack.animationsToGo` of each slot (the pick-up pop).
    pub bob: [i32; 9],
}

/// Per-frame inputs of the overlay.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame<'a> {
    pub partial_ticks: f32,
    /// The text being typed when the chat screen is open.
    pub chat_input: Option<&'a TextField>,
    /// F3 left and right columns when the debug screen is shown.
    pub debug: Option<(&'a [String], &'a [String])>,
    /// `reducedDebugInfo`: F3 keeps the crosshair.
    pub reduced_debug: bool,
    /// `Minecraft.isFancyGraphicsEnabled()`.
    pub fancy: bool,
    /// `thePlayer.getBrightness(partialTicks)`, 0..=1.
    pub brightness: f32,
    /// `WorldBorder` warning of `renderVignette`, 0..=1: how deep the player is inside the warning band.
    pub border_warning: f32,
    /// The camera is the player (not spectating another entity): the hotbar is drawn.
    pub player_view: bool,
    /// Health, food, armor, air and XP; `None` hides them.
    pub status: Option<&'a Status>,
    pub hotbar: Option<HotbarItems<'a>>,
    /// The player list key is held.
    pub player_list: bool,
    /// `Minecraft.getTime`: the clock of the tab list heart animation.
    pub now_ms: i64,
    /// The local player's name: it picks the team color sidebar.
    pub local_player: &'a str,
    /// `gameSettings.heldItemTooltips`.
    pub held_item_tooltips: bool,
    /// `perspective == 0` and a pumpkin on the head: the blur overlay.
    pub helmet_pumpkin: bool,
    /// The Nausea effect is active: it replaces the portal swirl.
    pub nausea: bool,
}

impl Hud {
    /// `updateTick`.
    pub fn tick(&mut self) {
        if self.record_up_for > 0 {
            self.record_up_for -= 1;
        }
        self.title.tick();
        self.update_counter += 1;
    }

    /// The held-item part of `updateTick`: call once per tick with the selected hotbar stack.
    pub fn update_held(&mut self, held: &Slot) {
        match (held, &self.highlight) {
            (None, _) => self.highlight_ticks = 0,
            (Some(s), Some(h))
                if s.id == h.id
                    && s.nbt == h.nbt
                    && (items::item(s.id).max_damage > 0 || s.damage == h.damage) =>
            {
                if self.highlight_ticks > 0 {
                    self.highlight_ticks -= 1;
                }
            }
            _ => self.highlight_ticks = 40,
        }
        self.highlight = held.clone();
    }

    /// `setRecordPlaying(message, false)`: the action bar text for 3 seconds.
    pub fn set_action_bar(&mut self, text: String) {
        self.record = text;
        self.record_up_for = 60;
    }

    /// `BossStatus.setBossStatus`: call each frame a boss is in view.
    pub fn set_boss(&mut self, name: String, ratio: f32) {
        self.boss.set(name, ratio);
    }

    /// `renderGameOverlay`.
    pub fn draw(&mut self, gui: &mut Gui, f: &Frame) {
        let (w, h) = (gui.scale.width, gui.scale.height);
        gui.set_blend(Blend::Alpha);
        if f.fancy {
            self.draw_vignette(gui, f.brightness, f.border_warning);
        }
        if f.helmet_pumpkin {
            overlay::draw_pumpkin(gui);
        }
        let portal = self.portal.lerp(f.partial_ticks);
        if !f.nausea && portal > 0.0 {
            overlay::draw_portal(gui, portal, self.update_counter);
        }
        if f.player_view {
            self.draw_hotbar(gui, f);
        }
        gui.set_color(0xFFFF_FFFF);
        let spectator = f.status.is_some_and(|s| s.spectator);
        if (f.debug.is_none() || f.reduced_debug) && !spectator {
            gui.set_blend(Blend::Invert);
            gui.blit(ICONS, w / 2 - 7, h / 2 - 7, 0, 0, 16, 16);
        }
        gui.set_blend(Blend::Alpha);
        self.boss.draw(gui);
        let survival = f.status.is_some_and(Status::survival);
        if let Some(s) = f.status {
            self.draw_status(gui, s);
        }
        if f.held_item_tooltips && !spectator {
            self.draw_selected_item(gui, survival);
        }
        if let Some((left, right)) = f.debug {
            debug::draw(gui, left, right);
        }
        self.draw_record(gui, f.partial_ticks);
        self.title.draw(gui, f.partial_ticks);
        self.scoreboard.draw_sidebar(gui, f.local_player);
        let open = f.chat_input.is_some();
        gui.push();
        gui.translate(0.0, (h - 48) as f32);
        self.chat.draw(gui, self.update_counter, open);
        gui.pop();
        self.tablist.set_visible(f.player_list, f.now_ms);
        if f.player_list {
            let clock = Clock {
                now_ms: f.now_ms,
                ticks: self.update_counter,
            };
            self.tablist.draw(gui, &self.scoreboard, clock);
        }
        if let Some(field) = f.chat_input {
            chat::draw_input(gui, field);
        }
        gui.set_color(0xFFFF_FFFF);
    }

    fn draw_hotbar(&self, gui: &mut Gui, f: &Frame) {
        let (w, h) = (gui.scale.width, gui.scale.height);
        gui.set_color(0xFFFF_FFFF);
        gui.blit(WIDGETS, w / 2 - 91, h - 22, 0, 0, 182, 22);
        let x = w / 2 - 91 - 1 + self.selected_slot * 20;
        gui.blit(WIDGETS, x, h - 22 - 1, 0, 22, 24, 22);
        if let Some(items) = &f.hotbar {
            draw_hotbar_items(gui, items, f.partial_ticks);
        }
    }

    fn draw_status(&mut self, gui: &mut Gui, s: &Status) {
        if s.survival() {
            status::draw_stats(gui, s, &mut self.status_state, self.update_counter);
        }
        match s.horse_jump {
            Some(p) => status::draw_jump(gui, p),
            None if s.survival() => status::draw_xp(gui, s),
            None => {}
        }
    }

    fn draw_record(&self, gui: &mut Gui, partial_ticks: f32) {
        if self.record_up_for <= 0 {
            return;
        }
        let (w, h) = (gui.scale.width, gui.scale.height);
        let t = self.record_up_for as f32 - partial_ticks;
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

    /// `renderSelectedItem`: the held item's name above the hotbar, fading out.
    fn draw_selected_item(&self, gui: &mut Gui, survival: bool) {
        let Some(s) = self.highlight.as_ref().filter(|_| self.highlight_ticks > 0) else {
            return;
        };
        let (name, custom) = tooltip::name(s);
        let name = if custom {
            format!("\u{a7}o{name}")
        } else {
            name
        };
        let i = (gui.scale.width - gui.width(&name)) / 2;
        let j = gui.scale.height - 59 + if survival { 0 } else { 14 };
        let k = ((self.highlight_ticks as f32 * 256.0 / 10.0) as u32).min(255);
        if k > 0 {
            gui.text(&name, i as f32, j as f32, 0xFF_FFFF | k << 24);
        }
    }

    /// `renderVignette`: darkens the screen edges more the darker the player's spot is.
    fn draw_vignette(&mut self, gui: &mut Gui, brightness: f32, border_warning: f32) {
        let level = brightness.clamp(0.0, 1.0);
        self.vignette_lag += (level - self.vignette_lag) * 0.01;
        let v = ((1.0 - self.vignette_lag).clamp(0.0, 1.0) * 255.0) as u32;
        // Inside the border's warning band the vignette is cyan: `color(0, f, f, 1)`.
        let tint = if border_warning > 0.0 {
            let c = (border_warning.min(1.0) * 255.0) as u32;
            0xFF00_0000 | (c << 8) | c
        } else {
            0xFF00_0000 | (v * 0x010101)
        };
        let (w, h) = (gui.scale.width as f32, gui.scale.height as f32);
        gui.set_blend(Blend::Darken);
        gui.blit_tinted(VIGNETTE, 0.0, 0.0, w, h, [0.0, 0.0, 1.0, 1.0], tint);
        gui.set_blend(Blend::Alpha);
    }
}

/// `renderHotbarItem` for all nine slots: the pop squash, the icon and the count.
fn draw_hotbar_items(gui: &mut Gui, items: &HotbarItems, partial_ticks: f32) {
    for (i, slot) in items.slots.iter().enumerate() {
        let Some(s) = slot else { continue };
        let (x, y) = hotbar_item_pos(gui, i as i32);
        let f = items.bob[i] as f32 - partial_ticks;
        if f > 0.0 {
            let f1 = 1.0 + f / 5.0;
            let (cx, cy) = ((x + 8) as f32, (y + 12) as f32);
            gui.push();
            gui.translate(cx, cy);
            gui.scale(1.0 / f1, (f1 + 1.0) / 2.0);
            gui.translate(-cx, -cy);
        }
        let mut c = GuiCanvas::new(gui);
        items.icons.draw(&mut c, s, x, y);
        if f > 0.0 {
            c.gui.pop();
        }
        items.icons.overlay(&mut c, s, x, y, None);
    }
}

/// Top-left of hotbar slot `i`'s item (`renderHotbarItem`).
pub fn hotbar_item_pos(gui: &Gui, i: i32) -> (i32, i32) {
    (
        gui.scale.width / 2 - 90 + i * 20 + 2,
        gui.scale.height - 16 - 3,
    )
}
