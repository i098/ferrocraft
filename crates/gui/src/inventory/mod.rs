//! Container screens of 1.8.9: `GuiContainer` and its player inventory, chest, crafting table,
//! furnace, dispenser and hopper screens, plus `GuiContainerCreative`.
//!
//! The engine owns a [`world::inventory::Inventory`] and one [`ContainerScreen`] while a
//! container is open, feeds it mouse and key events in GUI units, and sends the returned
//! [`Request`]s to the server.

mod creative;
pub mod icons;
pub mod layout;
pub(crate) mod tooltip;

use crate::screens::blit256 as modal;
pub use crate::screens::{Canvas, Metrics};
use creative::Creative;
use icons::Icons;
use layout::{layout, Layout, LABEL};
use world::effects::{self, Effects};
use world::inventory::{self as inv, Click, Inventory, ItemStack, Kind, Slot, OUTSIDE};

/// What the screen asks the engine to send.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// `C0E ClickWindow`.
    Click(Click),
    /// `C10 CreativeInventoryAction`; slot -1 drops the stack.
    Creative { slot: i16, item: Slot },
    /// `C0D CloseWindow`; the engine closes the screen.
    Close { window_id: u8 },
}

/// Keys a container screen reacts to, already mapped from the player's key bindings. The
/// character the key types travels beside it, for the creative search field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    /// The inventory key (E).
    Inventory,
    /// The drop key (Q).
    Drop,
    /// A hotbar key (1-9), 0-based.
    Hotbar(u8),
    /// The pick block binding when it is a key.
    PickBlock,
    /// The chat key (T): opens the creative search tab.
    Chat,
    Backspace,
    /// A key with no binding here; only its typed character matters.
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
}

/// Middle mouse: the default pick block binding (`-98 + 100`).
const PICK_BUTTON: u8 = 2;
/// `0x80FFFFFF`: hovered-slot highlight and drag preview.
const HIGHLIGHT: u32 = 0x80FF_FFFF;
const YELLOW: &str = "\u{a7}e";

/// `GuiContainer.dragSplitting` state.
#[derive(Clone, Debug, PartialEq)]
struct Split {
    button: u8,
    /// `dragSplittingLimit`: 0 even, 1 one each, 2 full stacks.
    limit: u8,
    slots: Vec<usize>,
}

pub struct ContainerScreen {
    width: i32,
    height: i32,
    layout: Layout,
    /// `theSlot`: the slot under the mouse at the last draw.
    hovered: Option<usize>,
    split: Option<Split>,
    ignore_up: bool,
    double_click: bool,
    last_click: Option<(Option<usize>, u64, u8)>,
    shift_clicked: Slot,
    creative: Option<Creative>,
    /// `PlayerInventoryScreen.hasStatusEffect`: the window sits right of the effect list.
    effects_shown: bool,
}

/// What `GuiContainer.drawScreen` reads besides the cursor position.
#[derive(Clone, Copy)]
pub struct InventoryView<'a> {
    pub inv: &'a Inventory,
    pub icons: &'a Icons,
    pub advanced_tooltips: bool,
    pub effects: &'a Effects,
}

/// `PlayerInventoryScreen.drawStatusEffects`: one 140x32 panel per effect, left of the window.
fn draw_effects(c: &mut dyn Canvas, effects: &Effects, x: i32, top: i32) {
    const TEXTURE: &str = "textures/gui/container/inventory.png";
    let n = effects.iter().count() as i32;
    let step = if n > 5 { 132 / (n - 1) } else { 33 };
    for (i, e) in effects.iter().enumerate() {
        let y = top + step * i as i32;
        modal(c, TEXTURE, x, y, 0, 166, 140, 32);
        if let Some(icon) = effects::icon(e.id) {
            modal(
                c,
                TEXTURE,
                x + 6,
                y + 7,
                icon % 8 * 18,
                198 + icon / 8 * 18,
                18,
                18,
            );
        }
        let suffix = [" II", " III", " IV"].get((e.amplifier as usize).wrapping_sub(1));
        let name = format!(
            "{}{}",
            effects::name(e.id).unwrap_or(""),
            suffix.unwrap_or(&"")
        );
        c.text(&name, (x + 28) as f32, (y + 6) as f32, 0xFFFF_FFFF, true);
        c.text(
            &effects::duration_string(e),
            (x + 28) as f32,
            (y + 16) as f32,
            0xFF7F_7F7F,
            true,
        );
    }
}

/// `GuiScreen.drawDefaultBackground` over a world.
fn dim_world(c: &mut dyn Canvas, w: i32, h: i32) {
    c.gradient(0, 0, w, h, 0xC010_1010, 0xD010_1010);
}

impl ContainerScreen {
    /// The screen for the inventory's active window; creative players get
    /// `GuiContainerCreative` instead of the survival inventory.
    pub fn new(inv: &Inventory, width: i32, height: i32) -> ContainerScreen {
        let creative = (inv.creative && inv.open.is_none()).then(Creative::new);
        ContainerScreen {
            width,
            height,
            layout: layout(inv.kind()),
            hovered: None,
            split: None,
            ignore_up: false,
            double_click: false,
            last_click: None,
            shift_clicked: None,
            creative,
            effects_shown: false,
        }
    }

    /// `setWorldAndResolution` after a window resize.
    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
    }

    /// `GuiScreen.updateScreen`, once per client tick.
    pub fn tick(&mut self) {
        if let Some(cr) = &mut self.creative {
            cr.tick();
        }
    }

    /// `GuiContainerCreative.updateScreen`: once the player leaves creative, become `GuiInventory`.
    pub fn sync_game_mode(&mut self, inv: &Inventory) {
        if self.creative.is_some() && !inv.creative {
            *self = ContainerScreen::new(inv, self.width, self.height);
        }
    }

    fn size(&self) -> (i32, i32) {
        match &self.creative {
            Some(_) => (creative::WIDTH, creative::HEIGHT),
            None => (self.layout.width, self.layout.height),
        }
    }

    fn origin(&self) -> (i32, i32) {
        let (w, h) = self.size();
        let left = if self.effects_shown {
            160 + (self.width - w - 200) / 2
        } else {
            (self.width - w) / 2
        };
        (left, (self.height - h) / 2)
    }

    /// Slot positions of what is on screen, in click-slot order.
    fn slots(&self) -> &[(i32, i32)] {
        match &self.creative {
            Some(cr) => cr.slots(),
            None => &self.layout.slots,
        }
    }

    /// `getSlotAtPosition` / `isMouseOverSlot`.
    fn slot_at(&self, mx: i32, my: i32) -> Option<usize> {
        let (l, t) = self.origin();
        let (x, y) = (mx - l, my - t);
        self.slots()
            .iter()
            .position(|&(sx, sy)| x >= sx - 1 && x < sx + 17 && y >= sy - 1 && y < sy + 17)
    }

    fn outside(&self, mx: i32, my: i32) -> bool {
        let (l, t) = self.origin();
        let (w, h) = self.size();
        mx < l || my < t || mx >= l + w || my >= t + h
    }

    /// Stack shown in screen slot `i`.
    fn stack<'a>(&'a self, inv: &'a Inventory, i: usize) -> &'a Slot {
        match &self.creative {
            Some(cr) => cr.stack(inv, i),
            None => inv.slot(i),
        }
    }

    /// `handleMouseClick`.
    fn click(
        &mut self,
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mode: i8,
        out: &mut Vec<Request>,
    ) {
        let id = slot.map_or(id, |s| s as i16);
        match &mut self.creative {
            Some(cr) => cr.click(inv, slot, id, button, mode, out),
            None => out.push(Request::Click(inv.click(id, button as i8, mode))),
        }
    }

    fn can_merge(&self, i: usize) -> bool {
        match &self.creative {
            Some(cr) => cr.can_merge(i),
            None => {
                !matches!(self.layout.texture, t if t.ends_with("inventory.png") || t.ends_with("crafting_table.png"))
                    || i != 0
            }
        }
    }

    fn can_drag_into(&self, i: usize) -> bool {
        self.creative.as_ref().is_none_or(|cr| cr.can_drag_into(i))
    }

    /// `Slot.isItemValid` for the screen slot, as the container checks it.
    fn valid(&self, inv: &Inventory, i: usize, s: &ItemStack) -> bool {
        match &self.creative {
            Some(cr) => cr.valid(inv, i, s),
            None => inv.slot_accepts(i, s),
        }
    }

    fn limit(&self, inv: &Inventory, i: usize, s: &ItemStack) -> i32 {
        match &self.creative {
            Some(cr) => cr.limit(inv, i, s),
            None => inv.slot_limit(i, s),
        }
    }

    /// `GuiContainer.mouseClicked`; `button` 0 left, 1 right, 2 middle.
    pub fn mouse_down(
        &mut self,
        inv: &mut Inventory,
        mx: i32,
        my: i32,
        button: u8,
        mods: Mods,
        now_ms: u64,
    ) -> Vec<Request> {
        let mut out = Vec::new();
        let (l, t) = self.origin();
        if let Some(cr) = &mut self.creative {
            if button == 0 && cr.tab_at(mx - l, my - t).is_some() {
                return out;
            }
            if button == 0 && cr.scrollbar_down(mx - l, my - t) {
                return out;
            }
        }
        let slot = self.slot_at(mx, my);
        self.double_click = self
            .last_click
            .is_some_and(|(s, at, b)| s == slot && now_ms.saturating_sub(at) < 250 && b == button);
        self.ignore_up = false;
        if button <= 1 || button == PICK_BUTTON {
            let id = if self.outside(mx, my) {
                Some(OUTSIDE)
            } else {
                slot.map(|s| s as i16)
            };
            if let (Some(id), None) = (id, &self.split) {
                self.press(inv, slot, id, button, mods, &mut out);
            }
        }
        self.last_click = Some((slot, now_ms, button));
        out
    }

    /// A press that is not part of a drag: an empty cursor clicks at once, a held stack
    /// starts a drag split that the release finishes.
    fn press(
        &mut self,
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mods: Mods,
        out: &mut Vec<Request>,
    ) {
        if inv.cursor.is_some() {
            self.split = Some(Split {
                button,
                limit: button.min(2),
                slots: Vec::new(),
            });
            return;
        }
        let shift = id != OUTSIDE && mods.shift;
        let mode = match () {
            _ if button == PICK_BUTTON => 3,
            _ if shift => {
                self.shift_clicked = slot.and_then(|s| self.stack(inv, s).clone());
                1
            }
            _ if id == OUTSIDE => 4,
            _ => 0,
        };
        self.click(inv, slot, id, button, mode, out);
        self.ignore_up = true;
    }

    /// `GuiContainer.mouseClickMove`: called while a button is held and the mouse moves.
    pub fn mouse_drag(&mut self, inv: &Inventory, mx: i32, my: i32) {
        let (_, t) = self.origin();
        if let Some(cr) = &mut self.creative {
            if cr.scrolling(my - t) {
                return;
            }
        }
        let Some(slot) = self.slot_at(mx, my) else {
            return;
        };
        let Some(c) = inv.cursor.as_ref() else { return };
        let Some(split) = &self.split else { return };
        if i32::from(c.count) > split.slots.len() as i32
            && !split.slots.contains(&slot)
            && inv::fits(self.stack(inv, slot).as_ref(), c, true)
            && self.valid(inv, slot, c)
            && self.can_drag_into(slot)
        {
            if let Some(split) = &mut self.split {
                split.slots.push(slot);
            }
        }
    }

    /// `GuiContainer.mouseReleased`.
    pub fn mouse_up(
        &mut self,
        inv: &mut Inventory,
        mx: i32,
        my: i32,
        button: u8,
        mods: Mods,
    ) -> Vec<Request> {
        let mut out = Vec::new();
        let (l, t) = self.origin();
        if let Some(cr) = &mut self.creative {
            cr.scrollbar_up();
            if button == 0 {
                if let Some(tab) = cr.tab_at(mx - l, my - t) {
                    cr.set_tab(inv, tab);
                    return out;
                }
            }
        }
        let slot = self.slot_at(mx, my);
        let id = if self.outside(mx, my) {
            OUTSIDE
        } else {
            slot.map_or(-1, |s| s as i16)
        };
        if let Some(s) = slot.filter(|&s| self.double_click && button == 0 && self.can_merge(s)) {
            self.double_click_up(inv, s, id, button, mods, &mut out);
        } else {
            self.release(inv, slot, id, button, mods, &mut out);
        }
        if inv.cursor.is_none() {
            self.last_click = None;
        }
        self.split = None;
        out
    }

    /// The release of a double click: collect into the cursor, or with shift move every
    /// stack like the shift-clicked one out of that inventory part.
    fn double_click_up(
        &mut self,
        inv: &mut Inventory,
        s: usize,
        id: i16,
        button: u8,
        mods: Mods,
        out: &mut Vec<Request>,
    ) {
        if !mods.shift {
            self.click(inv, Some(s), id, button, 6, out);
        } else if let Some(shifted) = self.shift_clicked.clone() {
            let targets: Vec<usize> = (0..self.slots().len())
                .filter(|&j| {
                    self.same_inventory(inv, j, s)
                        && self
                            .stack(inv, j)
                            .as_ref()
                            .is_some_and(|st| inv::fits(Some(st), &shifted, true))
                })
                .collect();
            for j in targets {
                self.click(inv, Some(j), j as i16, button, 1, out);
            }
        }
        self.double_click = false;
        self.last_click = None;
    }

    /// A plain release: finish a drag split, or click with the stack held since the press.
    fn release(
        &mut self,
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mods: Mods,
        out: &mut Vec<Request>,
    ) {
        if self.split.as_ref().is_some_and(|sp| sp.button != button) {
            self.split = None;
            self.ignore_up = true;
            return;
        }
        if self.ignore_up {
            self.ignore_up = false;
            return;
        }
        match self.split.take() {
            Some(sp) if !sp.slots.is_empty() => {
                let code = |event: u8| event & 3 | (sp.limit & 3) << 2;
                self.click(inv, None, OUTSIDE, code(0), 5, out);
                for &s in &sp.slots {
                    self.click(inv, Some(s), s as i16, code(1), 5, out);
                }
                self.click(inv, None, OUTSIDE, code(2), 5, out);
            }
            _ if inv.cursor.is_none() => {}
            _ if button == PICK_BUTTON => self.click(inv, slot, id, button, 3, out),
            _ => self.click_held(inv, slot, id, button, mods.shift, out),
        }
    }

    /// Click or shift click with the stack held since the press.
    fn click_held(
        &mut self,
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        shift: bool,
        out: &mut Vec<Request>,
    ) {
        let shift = id != OUTSIDE && shift;
        if shift {
            self.shift_clicked = slot.and_then(|s| self.stack(inv, s).clone());
        }
        self.click(inv, slot, id, button, if shift { 1 } else { 0 }, out);
    }

    /// Whether two screen slots belong to the same `IInventory` (shift double click).
    fn same_inventory(&self, inv: &Inventory, a: usize, b: usize) -> bool {
        let part = |i: usize| -> u8 {
            if let Some(cr) = &self.creative {
                return cr.part(i);
            }
            match inv.kind() {
                Kind::Player => match i {
                    0 => 0,
                    1..=4 => 1,
                    _ => 2,
                },
                Kind::Crafting => match i {
                    0 => 0,
                    1..=9 => 1,
                    _ => 2,
                },
                k => u8::from(i >= k.own_slots()) + 1,
            }
        };
        part(a) == part(b)
    }

    /// Mouse wheel; `delta` is +1 up, -1 down.
    pub fn scroll(&mut self, inv: &Inventory, delta: i32) {
        if let Some(cr) = &mut self.creative {
            cr.wheel(inv, delta);
        }
    }

    /// `GuiContainer.keyTyped`; `typed` is the character the key types, if any.
    pub fn key(
        &mut self,
        inv: &mut Inventory,
        key: Key,
        typed: Option<char>,
        mods: Mods,
    ) -> Vec<Request> {
        let mut out = Vec::new();
        if let Some(cr) = &mut self.creative {
            match cr.key(key, typed) {
                creative::KeyUse::Used => return out,
                creative::KeyUse::Hotbar => {
                    if !self.hotbar_key(inv, key, &mut out) {
                        if let Some(cr) = &mut self.creative {
                            cr.type_text(None, typed);
                        }
                    }
                    return out;
                }
                creative::KeyUse::Pass => {}
            }
        }
        if matches!(key, Key::Escape | Key::Inventory) {
            out.push(Request::Close {
                window_id: inv.window_id(),
            });
            return out;
        }
        if self.hotbar_key(inv, key, &mut out) {
            return out;
        }
        if let Some(s) = self.hovered.filter(|&s| self.stack(inv, s).is_some()) {
            match key {
                Key::PickBlock => self.click(inv, Some(s), s as i16, 0, 3, &mut out),
                Key::Drop => self.click(inv, Some(s), s as i16, u8::from(mods.ctrl), 4, &mut out),
                _ => {}
            }
        }
        out
    }

    /// `checkHotbarKeys`.
    fn hotbar_key(&mut self, inv: &mut Inventory, key: Key, out: &mut Vec<Request>) -> bool {
        let (Key::Hotbar(n), None, Some(s)) = (key, &inv.cursor, self.hovered) else {
            return false;
        };
        self.click(inv, Some(s), s as i16, n, 2, out);
        true
    }

    /// `GuiContainer.drawScreen`.
    pub fn draw(&mut self, c: &mut dyn Canvas, view: &InventoryView, mx: i32, my: i32) {
        let InventoryView {
            inv,
            icons,
            advanced_tooltips,
            effects,
        } = *view;
        // `checkStatusEffects`: only the player's own inventory and the creative screen list effects.
        self.effects_shown = !effects.is_empty() && inv.open.is_none();
        let (l, t) = self.origin();
        if let Some(cr) = &mut self.creative {
            cr.advanced = advanced_tooltips;
            cr.update_scroll(inv, my - t);
        }
        dim_world(c, self.width, self.height);
        match &self.creative {
            Some(cr) => cr.draw_background(c, inv, icons, l, t),
            None => self.draw_background(c, inv, l, t),
        }
        self.hovered = self.slot_at(mx, my);
        c.push([l as f32, t as f32], 1.0, 0.0);
        for i in 0..self.slots().len() {
            self.draw_slot(c, inv, icons, i);
        }
        if let Some(h) = self.hovered {
            let (x, y) = self.slots()[h];
            c.fill(x, y, x + 16, y + 16, HIGHLIGHT);
        }
        match &self.creative {
            Some(cr) => cr.draw_foreground(c),
            None => self.draw_foreground(c, inv),
        }
        if let Some(cursor) = &inv.cursor {
            self.draw_cursor(c, inv, icons, cursor, mx - l - 8, my - t - 8);
        }
        c.pop();
        if self.effects_shown {
            draw_effects(c, effects, l - 124, t);
        }
        if let Some(cr) = &self.creative {
            if cr.draw_hover_text(c, mx - l, my - t, mx, my, self.width, self.height) {
                return;
            }
        }
        if inv.cursor.is_none() {
            self.draw_tooltip(c, inv, mx, my, advanced_tooltips);
        }
    }

    /// The cursor stack, less what a drag split has painted into slots.
    fn draw_cursor(
        &self,
        c: &mut dyn Canvas,
        inv: &Inventory,
        icons: &Icons,
        cursor: &ItemStack,
        x: i32,
        y: i32,
    ) {
        let mut shown = cursor.clone();
        let mut text = None;
        if let Some(sp) = self.split.as_ref().filter(|sp| sp.slots.len() > 1) {
            let left = self.remnant(inv, cursor, sp);
            shown.count = left as i8;
            if left == 0 {
                text = Some(format!("{YELLOW}0"));
            }
        }
        icons.draw(c, &shown, x, y);
        icons.overlay(c, &shown, x, y, text.as_deref());
    }

    /// `renderToolTip` of the hovered stack.
    fn draw_tooltip(&self, c: &mut dyn Canvas, inv: &Inventory, mx: i32, my: i32, advanced: bool) {
        let Some(stack) = self.hovered.and_then(|h| self.stack(inv, h).as_ref()) else {
            return;
        };
        let mut lines = tooltip::lines(stack, advanced);
        if let Some(cr) = &self.creative {
            cr.extend_tooltip(stack, &mut lines);
        }
        tooltip::draw(c, &lines, mx, my, self.width, self.height);
    }

    /// `updateDragSplitting`: what stays on the cursor after the painted slots fill.
    fn remnant(&self, inv: &Inventory, cursor: &ItemStack, sp: &Split) -> i32 {
        let mut left = i32::from(cursor.count);
        for &s in &sp.slots {
            let had = self
                .stack(inv, s)
                .as_ref()
                .map_or(0, |x| i32::from(x.count));
            let n = (inv::drag_share(sp.limit, cursor, sp.slots.len()) + had)
                .min(inv::max_stack(cursor))
                .min(self.limit(inv, s, cursor));
            left -= n - had;
        }
        left
    }

    /// The stack a painted slot shows during a drag split, with the yellow cap text when
    /// the share is cut to the stack or slot limit.
    fn drag_preview(
        &self,
        inv: &Inventory,
        i: usize,
        shown: Option<&ItemStack>,
        cursor: &ItemStack,
        slots: usize,
        limit: u8,
    ) -> Option<(ItemStack, Option<String>)> {
        if !inv::fits(shown, cursor, true) || !self.can_drag_into(i) {
            return None;
        }
        let had = shown.map_or(0, |s| i32::from(s.count));
        let mut n = inv::drag_share(limit, cursor, slots) + had;
        let mut text = None;
        for cap in [inv::max_stack(cursor), self.limit(inv, i, cursor)] {
            if n > cap {
                text = Some(format!("{YELLOW}{cap}"));
                n = cap;
            }
        }
        let stack = ItemStack {
            count: n as i8,
            ..cursor.clone()
        };
        Some((stack, text))
    }

    /// `GuiContainer.drawSlot`, in window coordinates.
    fn draw_slot(&self, c: &mut dyn Canvas, inv: &Inventory, icons: &Icons, i: usize) {
        let (x, y) = self.slots()[i];
        let mut shown = self.stack(inv, i).clone();
        let mut text = None;
        let mut preview = false;
        if let (Some(sp), Some(cursor)) = (&self.split, &inv.cursor) {
            if sp.slots.contains(&i) {
                if sp.slots.len() == 1 {
                    return;
                }
                let p = self.drag_preview(inv, i, shown.as_ref(), cursor, sp.slots.len(), sp.limit);
                if let Some((stack, cap)) = p {
                    (shown, text, preview) = (Some(stack), cap, true);
                }
            }
        }
        let Some(s) = &shown else {
            let empty = match &self.creative {
                Some(cr) => cr.empty_icon(i),
                None => layout::empty_slot_icon(inv.kind(), i),
            };
            if let Some(tex) = empty {
                let uv = [0.0, 0.0, 1.0, 1.0];
                c.blit(tex, x as f32, y as f32, 16.0, 16.0, uv, 0xFFFF_FFFF);
            }
            return;
        };
        if preview {
            c.fill(x, y, x + 16, y + 16, HIGHLIGHT);
        }
        icons.draw(c, s, x, y);
        icons.overlay(c, s, x, y, text.as_deref());
    }

    /// `drawGuiContainerBackgroundLayer` of each survival screen.
    fn draw_background(&self, c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32) {
        let lay = &self.layout;
        match inv.kind() {
            Kind::Chest(rows) => {
                let rows = i32::from(rows);
                modal(c, lay.texture, l, t, 0, 0, lay.width, rows * 18 + 17);
                modal(c, lay.texture, l, t + rows * 18 + 17, 0, 126, lay.width, 96);
            }
            Kind::Furnace => {
                modal(c, lay.texture, l, t, 0, 0, lay.width, lay.height);
                let p = inv.open.as_ref().map_or([0; 4], |w| w.props);
                let p = p.map(i32::from);
                if p[0] > 0 {
                    // TileEntityFurnace.isBurning; getBurnLeftScaled(13)
                    let total = if p[1] == 0 { 200 } else { p[1] };
                    let k = p[0] * 13 / total;
                    modal(
                        c,
                        lay.texture,
                        l + 56,
                        t + 36 + 12 - k,
                        176,
                        12 - k,
                        14,
                        k + 1,
                    );
                }
                // getCookProgressScaled(24)
                let cook = if p[3] != 0 && p[2] != 0 {
                    p[2] * 24 / p[3]
                } else {
                    0
                };
                modal(c, lay.texture, l + 79, t + 34, 176, 14, cook + 1, 16);
            }
            _ => modal(c, lay.texture, l, t, 0, 0, lay.width, lay.height),
        }
        // shortcut: GuiInventory's 3D player model (drawEntityOnScreen) waits for the entity renderer
    }

    /// `drawGuiContainerForegroundLayer` of each survival screen.
    fn draw_foreground(&self, c: &mut dyn Canvas, inv: &Inventory) {
        let title = inv.open.as_ref().map_or("", |w| w.title.as_str());
        let h = self.layout.height;
        let inventory =
            |c: &mut dyn Canvas| c.text("Inventory", 8.0, (h - 96 + 2) as f32, LABEL, false);
        match inv.kind() {
            Kind::Player => c.text("Crafting", 86.0, 16.0, LABEL, false),
            Kind::Crafting => {
                c.text("Crafting", 28.0, 6.0, LABEL, false);
                inventory(c);
            }
            Kind::Furnace | Kind::Dispenser => {
                let x = self.layout.width / 2 - c.width(title) / 2;
                c.text(title, x as f32, 6.0, LABEL, false);
                inventory(c);
            }
            Kind::Chest(_) | Kind::Hopper => {
                c.text(title, 8.0, 6.0, LABEL, false);
                inventory(c);
            }
        }
    }
}
