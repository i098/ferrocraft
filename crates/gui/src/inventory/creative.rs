//! `GuiContainerCreative` and `ContainerCreative` of 1.8.9: the creative tabs, the item grid
//! with its scrollbar, the search field, the survival inventory tab and the destroy slot.
//!
//! Creative clicks do not send `ClickWindow`: the client edits the player inventory itself and
//! sends `CreativeInventoryAction` for each changed slot, as `CreativeCrafting` does.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::icons::Icons;
use super::layout::{self, LABEL};
use super::{modal, tooltip, Canvas, Key, Request, OUTSIDE};
use world::inventory::items::{self, Tab};
use world::inventory::{
    self as inv, Compound, Inventory, ItemStack, Kind, Nbt, Slot, Tag, PLAYER_SLOTS,
};

pub const WIDTH: i32 = 195;
pub const HEIGHT: i32 = 136;
const SEARCH: usize = 5;
const INVENTORY: usize = 11;
/// Grid slots of the item tabs (`InventoryBasic` of 45), before the 9 hotbar slots.
const GRID: usize = 45;
/// The destroy-item slot of the inventory tab, after the 45 player slots.
const BIN: usize = PLAYER_SLOTS;
const TABS: &str = "textures/gui/container/creative_inventory/tabs.png";
const SEARCH_MAX: usize = 15;
/// `GuiContainerCreative.selectedTabIndex` is static: the screen reopens on the last tab.
static LAST_TAB: AtomicUsize = AtomicUsize::new(0);
static EMPTY: Slot = None;

/// What a key press did in the creative screen.
pub enum KeyUse {
    Used,
    /// A hotbar key on the search tab: the screen tries the hotbar swap, else types the key.
    Hotbar,
    Pass,
}

pub struct Creative {
    tab: usize,
    /// `ContainerCreative.itemList`.
    list: Vec<ItemStack>,
    /// The 45 grid slots shown at the current scroll.
    grid: Vec<Slot>,
    slots: Vec<(i32, i32)>,
    scroll: f32,
    scrolling: bool,
    search: String,
    /// `field_147057_D`: a click happened, so the next key on the search tab clears the field.
    clear_search: bool,
    /// `searchField.cursorCounter`.
    ticks: u32,
    pub advanced: bool,
}

fn tab(i: usize) -> &'static Tab {
    &items::tabs()[i]
}

fn column(i: usize) -> i32 {
    (i % 6) as i32
}

fn top_row(i: usize) -> bool {
    i < 6
}

/// `func_147049_a` hit box origin of a tab, relative to the window.
fn tab_origin(i: usize) -> (i32, i32) {
    let col = column(i);
    let x = match col {
        5 => WIDTH - 28 + 2,
        c => 28 * c + c,
    };
    (x, if top_row(i) { -32 } else { HEIGHT })
}

fn entry_stack(&(id, damage, ench): &(i16, i16, Option<(i16, i16)>)) -> ItemStack {
    let nbt = ench.map(|(e, lvl)| {
        let entry: Compound = vec![
            ("id".into(), Tag::Short(e)),
            ("lvl".into(), Tag::Short(lvl)),
        ];
        Nbt {
            name: String::new(),
            root: vec![(
                "StoredEnchantments".into(),
                Tag::List(vec![Tag::Compound(entry)]),
            )],
        }
    });
    ItemStack {
        id,
        count: 1,
        damage,
        nbt,
    }
}

fn with_count(s: &ItemStack, n: i32) -> ItemStack {
    ItemStack {
        count: n as i8,
        ..s.clone()
    }
}

fn strip_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// `ChatAllowedCharacters.isAllowedCharacter`.
fn allowed(c: char) -> bool {
    c != '\u{a7}' && c >= ' ' && c != '\u{7f}'
}

/// `Item.getCreativeTab` as a tab index, or for an enchanted book with one enchantment the
/// first tab that lists that enchantment's book.
fn item_tab(s: &ItemStack) -> Option<usize> {
    let real = |t: &&Tab| usize::from(t.index) != SEARCH && usize::from(t.index) != INVENTORY;
    if s.id == items::ENCHANTED_BOOK {
        let Some(Tag::List(l)) = s
            .nbt
            .as_ref()
            .and_then(|n| tooltip::get(&n.root, "StoredEnchantments"))
        else {
            return None;
        };
        let [Tag::Compound(e)] = l.as_slice() else {
            return None;
        };
        let Some(Tag::Short(id)) = tooltip::get(e, "id") else {
            return None;
        };
        return items::tabs()
            .iter()
            .filter(real)
            .find(|t| {
                t.entries
                    .iter()
                    .any(|(i, _, en)| *i == s.id && en.is_some_and(|(e, _)| e == *id))
            })
            .map(|t| usize::from(t.index));
    }
    items::tabs()
        .iter()
        .filter(real)
        .find(|t| t.entries.iter().any(|(i, _, _)| *i == s.id))
        .map(|t| usize::from(t.index))
}

impl Creative {
    pub fn new() -> Creative {
        let mut c = Creative {
            tab: usize::MAX,
            list: Vec::new(),
            grid: vec![None; GRID],
            slots: Vec::new(),
            scroll: 0.0,
            scrolling: false,
            search: String::new(),
            clear_search: false,
            ticks: 0,
            advanced: false,
        };
        c.select(LAST_TAB.load(Ordering::Relaxed));
        c
    }

    pub fn slots(&self) -> &[(i32, i32)] {
        &self.slots
    }

    pub fn stack<'a>(&'a self, inv: &'a Inventory, i: usize) -> &'a Slot {
        match (self.tab, i) {
            (INVENTORY, BIN) => &EMPTY,
            (INVENTORY, i) => &inv.player[i],
            (_, i) if i < GRID => &self.grid[i],
            (_, i) => &inv.player[36 + i - GRID],
        }
    }

    /// `ContainerCreative.canMergeSlot`: only slots below y 90 (hotbar row, destroy slot).
    pub fn can_merge(&self, i: usize) -> bool {
        self.slots.get(i).is_some_and(|&(_, y)| y > 90)
    }

    /// `ContainerCreative.canDragIntoSlot`: player inventory slots, never the grid or bin.
    pub fn can_drag_into(&self, i: usize) -> bool {
        match self.tab {
            INVENTORY => i < BIN,
            _ => i >= GRID,
        }
    }

    pub fn valid(&self, inv: &Inventory, i: usize, s: &ItemStack) -> bool {
        self.tab != INVENTORY || i == BIN || inv.slot_accepts(i, s)
    }

    pub fn limit(&self, inv: &Inventory, i: usize, s: &ItemStack) -> i32 {
        if self.tab == INVENTORY && i < BIN {
            inv.slot_limit(i, s)
        } else {
            64
        }
    }

    /// Which `IInventory` a slot belongs to, for the shift double click.
    pub fn part(&self, i: usize) -> u8 {
        match (self.tab, i) {
            (INVENTORY, 0) => 0,
            (INVENTORY, 1..=4) => 1,
            (INVENTORY, BIN) => 3,
            (INVENTORY, _) => 2,
            (_, i) => u8::from(i >= GRID) + 3,
        }
    }

    pub fn empty_icon(&self, i: usize) -> Option<&'static str> {
        (self.tab == INVENTORY && i < BIN)
            .then(|| layout::empty_slot_icon(Kind::Player, i))
            .flatten()
    }

    pub fn tab_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..items::tabs().len()).find(|&i| {
            let (tx, ty) = tab_origin(i);
            x >= tx && x <= tx + 28 && y >= ty && y <= ty + 32
        })
    }

    /// `ContainerCreative.func_148328_e` with the tab's scrollbar flag.
    fn needs_scrollbars(&self) -> bool {
        self.tab != INVENTORY && tab(self.tab).scrollbar && self.list.len() > GRID
    }

    pub fn scrollbar_down(&mut self, x: i32, y: i32) -> bool {
        let on = (175..189).contains(&x) && (18..130).contains(&y);
        if on {
            self.scrolling = self.needs_scrollbars();
        }
        on
    }

    pub fn scrollbar_up(&mut self) {
        self.scrolling = false;
    }

    pub fn scrolling(&self, _y: i32) -> bool {
        self.scrolling
    }

    /// The drag of the scrollbar, from `drawScreen`.
    pub fn update_scroll(&mut self, _inv: &Inventory, y: i32) {
        if self.scrolling {
            self.scroll = (((y - 18) as f32 - 7.5) / (112.0 - 15.0)).clamp(0.0, 1.0);
            self.scroll_to();
        }
    }

    /// `handleMouseInput`: the wheel moves one row of the list.
    pub fn wheel(&mut self, _inv: &Inventory, delta: i32) {
        if delta != 0 && self.needs_scrollbars() {
            let rows = (self.list.len() / 9) as f64 - 5.0;
            let step = f64::from(delta.signum()) / rows;
            self.scroll = ((f64::from(self.scroll) - step) as f32).clamp(0.0, 1.0);
            self.scroll_to();
        }
    }

    /// `ContainerCreative.scrollTo`.
    fn scroll_to(&mut self) {
        let rows = (self.list.len() as i32 + 9 - 1) / 9 - 5;
        let first = ((f64::from(self.scroll * rows as f32) + 0.5) as i32).max(0);
        for r in 0..5 {
            for c in 0..9 {
                let i = c + (r + first) * 9;
                self.grid[(c + r * 9) as usize] = usize::try_from(i)
                    .ok()
                    .and_then(|i| self.list.get(i))
                    .cloned();
            }
        }
    }

    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    pub fn set_tab(&mut self, _inv: &Inventory, i: usize) {
        self.select(i);
    }

    /// `setCurrentCreativeTab`.
    fn select(&mut self, i: usize) {
        let i = i.min(items::tabs().len() - 1);
        self.tab = i;
        LAST_TAB.store(i, Ordering::Relaxed);
        self.list = tab(i).entries.iter().map(entry_stack).collect();
        self.slots = if i == INVENTORY {
            let mut s: Vec<(i32, i32)> = (0..PLAYER_SLOTS as i32)
                .map(|j| match j {
                    0..=4 => (-2000, -2000),
                    5..=8 => (9 + (j - 5) / 2 * 54, 6 + (j - 5) % 2 * 27),
                    36.. => (9 + (j - 9) % 9 * 18, 112),
                    _ => (9 + (j - 9) % 9 * 18, 54 + (j - 9) / 9 * 18),
                })
                .collect();
            s.push((173, 112));
            s
        } else {
            let grid = (0..GRID as i32).map(|j| (9 + j % 9 * 18, 18 + j / 9 * 18));
            grid.chain((0..9).map(|k| (9 + k * 18, 112))).collect()
        };
        if i == SEARCH {
            self.search.clear();
            self.update_search();
        }
        self.scroll = 0.0;
        self.scroll_to();
    }

    /// `updateCreativeSearch`: the stacks whose tooltip contains the search text.
    fn update_search(&mut self) {
        let needle = self.search.to_lowercase();
        self.list = tab(SEARCH)
            .entries
            .iter()
            .map(entry_stack)
            .filter(|s| {
                tooltip::lines(s, self.advanced)
                    .iter()
                    .any(|l| strip_codes(l).to_lowercase().contains(&needle))
            })
            .collect();
        self.scroll = 0.0;
        self.scroll_to();
    }

    /// `GuiTextField.textboxKeyTyped` for the search field, which always has focus.
    pub fn type_text(&mut self, key: Option<Key>, typed: Option<char>) -> bool {
        if key == Some(Key::Backspace) {
            self.search.pop();
        } else if let Some(c) = typed.filter(|&c| allowed(c)) {
            if self.search.chars().count() < SEARCH_MAX {
                self.search.push(c);
            }
        } else {
            return false;
        }
        self.update_search();
        true
    }

    /// `GuiContainerCreative.keyTyped`.
    pub fn key(&mut self, key: Key, typed: Option<char>) -> KeyUse {
        if self.tab != SEARCH {
            if key == Key::Chat {
                self.select(SEARCH);
                return KeyUse::Used;
            }
            return KeyUse::Pass;
        }
        if self.clear_search {
            self.clear_search = false;
            self.search.clear();
        }
        if matches!(key, Key::Hotbar(_)) {
            return KeyUse::Hotbar;
        }
        if self.type_text(Some(key), typed) {
            KeyUse::Used
        } else {
            KeyUse::Pass
        }
    }

    /// `CreativeCrafting.sendSlotContents` for every player slot that changed.
    fn send_changes(before: &[Slot], inv: &Inventory, out: &mut Vec<Request>) {
        for (j, (a, b)) in before.iter().zip(&inv.player).enumerate() {
            if a != b {
                out.push(Request::Creative {
                    slot: j as i16,
                    item: b.clone(),
                });
            }
        }
    }

    fn throw(item: ItemStack, out: &mut Vec<Request>) {
        out.push(Request::Creative {
            slot: -1,
            item: Some(item),
        });
    }

    /// `GuiContainerCreative.handleMouseClick`.
    pub fn click(
        &mut self,
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mode: i8,
        out: &mut Vec<Request>,
    ) {
        self.clear_search = true;
        let shift = mode == 1;
        let mode = if id == OUTSIDE && mode == 0 { 4 } else { mode };
        match (self.tab == INVENTORY, slot) {
            (true, Some(BIN)) => Self::click_bin(inv, shift, out),
            (true, _) => Self::click_inventory_tab(inv, slot, id, button, mode, out),
            (false, _) if mode == 5 => Self::click_hotbar_row(inv, slot, id, button, mode, out),
            (false, None) => Self::click_outside(inv, button, out),
            (false, Some(s)) if s < GRID => self.click_grid(inv, s, button, mode, shift, out),
            (false, Some(_)) => Self::click_hotbar_row(inv, slot, id, button, mode, out),
        }
    }

    /// A click outside the window: left drops the cursor stack, right drops one item.
    fn click_outside(inv: &mut Inventory, button: u8, out: &mut Vec<Request>) {
        let Some(mut c) = inv.cursor.take() else {
            return;
        };
        match button {
            0 => Self::throw(c, out),
            1 => {
                Self::throw(with_count(&c, 1), out);
                c.count -= 1;
                inv.cursor = (c.count > 0).then_some(c);
            }
            _ => inv.cursor = Some(c),
        }
    }

    /// The destroy slot: shift clears the whole player inventory, else the cursor.
    fn click_bin(inv: &mut Inventory, shift: bool, out: &mut Vec<Request>) {
        if !shift {
            inv.cursor = None;
            return;
        }
        for (j, slot) in inv.player.iter_mut().enumerate().take(PLAYER_SLOTS) {
            out.push(Request::Creative {
                slot: j as i16,
                item: None,
            });
            *slot = None;
        }
    }

    /// The survival inventory tab: drops leave as `C10` with slot -1; other clicks run
    /// `slotClick` on the player window and send each changed slot.
    fn click_inventory_tab(
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mode: i8,
        out: &mut Vec<Request>,
    ) {
        let held = slot.and_then(|s| Some((s, inv.player[s].clone()?)));
        if let (4, Some((s, st))) = (mode, held) {
            let all = inv::max_stack(&st);
            let n = (if button == 0 { 1 } else { all }).min(i32::from(st.count));
            let left = i32::from(st.count) - n;
            Self::throw(with_count(&st, n), out);
            inv.player[s] = (left > 0).then(|| with_count(&st, left));
        } else if let Some(c) = inv.cursor.take_if(|_| mode == 4) {
            Self::throw(c, out);
        } else {
            let before = inv.player.clone();
            inv.slot_click(
                slot.map_or(i32::from(id), |s| s as i32),
                i32::from(button),
                mode,
            );
            Self::send_changes(&before, inv, out);
        }
    }

    /// A click on the item grid of a creative tab.
    fn click_grid(
        &self,
        inv: &mut Inventory,
        s: usize,
        button: u8,
        mode: i8,
        shift: bool,
        out: &mut Vec<Request>,
    ) {
        let Some(p) = self.grid[s].clone() else {
            if !matches!(mode, 2..=4) {
                inv.cursor = None;
            }
            return;
        };
        let full = with_count(&p, inv::max_stack(&p));
        match mode {
            2 if button < 9 => {
                let before = inv.player.clone();
                inv.player[36 + usize::from(button)] = Some(full);
                Self::send_changes(&before, inv, out);
            }
            2 => {}
            3 => {
                if inv.cursor.is_none() {
                    inv.cursor = Some(full);
                }
            }
            4 => Self::throw(if button == 0 { with_count(&p, 1) } else { full }, out),
            _ => Self::grid_pickup(inv, p, full, button, shift),
        }
    }

    /// A pickup click on a grid item: a matching cursor stack grows or shrinks by one,
    /// an empty cursor takes the item (shift: a full stack), anything else clears the cursor.
    fn grid_pickup(inv: &mut Inventory, p: ItemStack, full: ItemStack, button: u8, shift: bool) {
        let Some(c) = inv.cursor.as_mut() else {
            inv.cursor = Some(if shift { full } else { p });
            return;
        };
        if c.id != p.id || c.damage != p.damage || !Self::grid_adjust(c, full.count, button, shift)
        {
            inv.cursor = None;
        }
    }

    /// Grows or shrinks a matching cursor stack; false when it empties.
    fn grid_adjust(c: &mut ItemStack, full: i8, button: u8, shift: bool) -> bool {
        if button != 0 {
            c.count -= 1;
        } else if shift {
            c.count = full;
        } else if c.count < full {
            c.count += 1;
        }
        c.count > 0
    }

    /// `ContainerCreative.slotClick` on the hotbar row (screen slots 45..54 are player 36..45).
    fn click_hotbar_row(
        inv: &mut Inventory,
        slot: Option<usize>,
        id: i16,
        button: u8,
        mode: i8,
        out: &mut Vec<Request>,
    ) {
        let mapped = slot.map_or(i32::from(id), |s| (36 + s - GRID) as i32);
        match (mode, slot) {
            (1, Some(s)) => inv.player[36 + s - GRID] = None,
            (1, None) => {}
            (6, Some(s)) => Self::collect_hotbar(inv, 36 + s - GRID, button),
            (6, None) => {}
            _ => {
                inv.slot_click(mapped, i32::from(button), mode);
            }
        }
        if button & 3 == 2 {
            for (k, item) in inv.player.iter().enumerate().take(45).skip(36) {
                out.push(Request::Creative {
                    slot: k as i16,
                    item: item.clone(),
                });
            }
        } else if let Some(s) = slot {
            out.push(Request::Creative {
                slot: (s - 9) as i16,
                item: inv.player[s - 9].clone(),
            });
        }
    }

    /// `Container.slotClick` mode 6 on `ContainerCreative`: only the hotbar can merge.
    fn collect_hotbar(inv: &mut Inventory, clicked: usize, button: u8) {
        if inv.player[clicked].is_some() {
            return;
        }
        let Some(mut c) = inv.cursor.take() else {
            return;
        };
        let max = inv::max_stack(&c);
        let order: Vec<usize> = if button == 0 {
            (36..45).collect()
        } else {
            (36..45).rev().collect()
        };
        // Pass 0 skips full stacks, pass 1 takes from them too.
        for (pass, &k) in [0, 1]
            .iter()
            .flat_map(|p| order.iter().map(move |k| (*p, k)))
        {
            if i32::from(c.count) >= max {
                break;
            }
            let Some(st) = inv.player[k].as_mut() else {
                continue;
            };
            let full = i32::from(st.count) == inv::max_stack(st);
            if !inv::fits(Some(st), &c, true) || (pass == 0 && full) {
                continue;
            }
            let n = (max - i32::from(c.count)).min(i32::from(st.count));
            st.count -= n as i8;
            c.count += n as i8;
            if st.count <= 0 {
                inv.player[k] = None;
            }
        }
        inv.cursor = Some(c);
    }

    /// `func_147051_a`: one tab and its icon.
    fn draw_tab(&self, c: &mut dyn Canvas, icons: &Icons, i: usize, l: i32, t: i32) {
        let col = column(i);
        let selected = i == self.tab;
        let mut v = if selected { 32 } else { 0 };
        let mut x = match col {
            5 => l + WIDTH - 28,
            c => l + 28 * c + c,
        };
        let mut y = t;
        if top_row(i) {
            y -= 28;
        } else {
            v += 64;
            y += HEIGHT - 4;
        }
        modal(c, TABS, x, y, col * 28, v, 28, 32);
        x += 6;
        y += 8 + if top_row(i) { 1 } else { -1 };
        let (id, damage) = tab(i).icon;
        let icon = ItemStack {
            id,
            count: 1,
            damage,
            nbt: None,
        };
        icons.draw(c, &icon, x, y);
        icons.overlay(c, &icon, x, y, None);
    }

    /// `drawGuiContainerBackgroundLayer`.
    pub fn draw_background(
        &self,
        c: &mut dyn Canvas,
        _inv: &Inventory,
        icons: &Icons,
        l: i32,
        t: i32,
    ) {
        for i in (0..items::tabs().len()).filter(|&i| i != self.tab) {
            self.draw_tab(c, icons, i, l, t);
        }
        let bg = format!(
            "textures/gui/container/creative_inventory/tab_{}",
            tab(self.tab).texture
        );
        modal(c, &bg, l, t, 0, 0, WIDTH, HEIGHT);
        if self.tab == SEARCH {
            self.draw_search(c, l + 82, t + 6);
        }
        if tab(self.tab).scrollbar {
            let y = t + 18 + ((130 - 18 - 17) as f32 * self.scroll) as i32;
            modal(
                c,
                TABS,
                l + 175,
                y,
                232 + if self.needs_scrollbars() { 0 } else { 12 },
                0,
                12,
                15,
            );
        }
        self.draw_tab(c, icons, self.tab, l, t);
        // shortcut: the inventory tab's 3D player model (drawEntityOnScreen) waits for the entity renderer.
    }

    /// `GuiTextField.drawTextBox` without background, cursor at the end, focused.
    fn draw_search(&self, c: &mut dyn Canvas, x: i32, y: i32) {
        const WHITE: u32 = 0xFFFF_FFFF;
        let mut text = String::new();
        for ch in self.search.chars() {
            text.push(ch);
            if c.width(&text) > 89 {
                text.pop();
                break;
            }
        }
        let end = x + c.width(&text);
        if !text.is_empty() {
            c.text(&text, x as f32, y as f32, WHITE, true);
        }
        if (self.ticks / 6).is_multiple_of(2) {
            if self.search.chars().count() >= SEARCH_MAX {
                c.fill(end - 1, y - 1, end, y + 1 + 9, 0xFFD0_D0D0);
            } else {
                c.text("_", end as f32, y as f32, WHITE, true);
            }
        }
    }

    /// `drawGuiContainerForegroundLayer`: the tab title.
    pub fn draw_foreground(&self, c: &mut dyn Canvas) {
        let t = tab(self.tab);
        if t.title {
            c.text(t.label, 8.0, 6.0, LABEL, false);
        }
    }

    /// Tab names and the destroy slot label on hover; `x`, `y` are window relative.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_hover_text(
        &self,
        c: &mut dyn Canvas,
        x: i32,
        y: i32,
        mx: i32,
        my: i32,
        w: i32,
        h: i32,
    ) -> bool {
        let inside = |l: i32, t: i32, rw: i32, rh: i32| {
            x >= l - 1 && x < l + rw + 1 && y >= t - 1 && y < t + rh + 1
        };
        let text = (0..items::tabs().len())
            .find(|&i| {
                let (tx, ty) = tab_origin(i);
                inside(tx + 3, ty + 3, 23, 27)
            })
            .map(|i| tab(i).label)
            .or_else(|| {
                (self.tab == INVENTORY && inside(173, 112, 16, 16)).then_some("Destroy Item")
            });
        match text {
            Some(text) => {
                tooltip::draw(c, &[text.to_string()], mx, my, w, h);
                true
            }
            None => false,
        }
    }

    /// `GuiContainerCreative.renderToolTip`: the search tab names each stack's tab.
    pub fn extend_tooltip(&self, s: &ItemStack, lines: &mut Vec<String>) {
        if self.tab != SEARCH {
            return;
        }
        if let Some(t) = item_tab(s) {
            lines.insert(
                1.min(lines.len()),
                format!("\u{a7}7\u{a7}l\u{a7}9{}", tab(t).label),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone() -> ItemStack {
        ItemStack {
            id: 1,
            count: 1,
            damage: 0,
            nbt: None,
        }
    }

    #[test]
    fn grid_pick_and_drop() {
        let mut cr = Creative::new();
        cr.select(0);
        let mut inv = Inventory::default();
        inv.creative = true;
        let mut out = Vec::new();
        assert_eq!(cr.grid[0], Some(stone()));
        cr.click(&mut inv, Some(0), 0, 0, 1, &mut out);
        assert_eq!(inv.cursor, Some(with_count(&stone(), 64)));
        // Placing in the hotbar sends the slot (screen 45 is player slot 36).
        cr.click(&mut inv, Some(45), 45, 0, 0, &mut out);
        assert_eq!(
            out,
            [Request::Creative {
                slot: 36,
                item: Some(with_count(&stone(), 64))
            }]
        );
        assert_eq!(inv.cursor, None);
        // Dropping outside throws one with the right button.
        inv.cursor = Some(with_count(&stone(), 3));
        out.clear();
        cr.click(&mut inv, None, OUTSIDE, 1, 0, &mut out);
        assert_eq!(
            out,
            [Request::Creative {
                slot: -1,
                item: Some(stone())
            }]
        );
        assert_eq!(inv.cursor, Some(with_count(&stone(), 2)));
    }

    #[test]
    fn bin_shift_click_clears_all_player_slots() {
        let mut cr = Creative::new();
        cr.select(INVENTORY);
        let mut inv = Inventory::default();
        inv.creative = true;
        inv.player[3] = Some(stone());
        inv.player[44] = Some(stone());
        let mut out = Vec::new();
        cr.click(&mut inv, Some(BIN), 0, 0, 1, &mut out);
        assert_eq!(out.len(), 45);
        for (j, r) in out.iter().enumerate() {
            assert_eq!(
                *r,
                Request::Creative {
                    slot: j as i16,
                    item: None
                }
            );
        }
        assert!(inv.player.iter().all(Option::is_none));
    }

    #[test]
    fn hotbar_middle_click_resends_slots_36_to_44() {
        let mut cr = Creative::new();
        cr.select(0);
        let mut inv = Inventory::default();
        inv.creative = true;
        inv.player[37] = Some(stone());
        let mut out = Vec::new();
        cr.click(&mut inv, Some(45), 45, 2, 0, &mut out);
        let want: Vec<Request> = (36..45)
            .map(|k| Request::Creative {
                slot: k,
                item: (k == 37).then(stone),
            })
            .collect();
        assert_eq!(out, want);
    }

    #[test]
    fn search_filters_by_tooltip() {
        let mut cr = Creative::new();
        cr.select(SEARCH);
        let all = cr.list.len();
        for ch in "granite".chars() {
            cr.type_text(None, Some(ch));
        }
        assert!(cr.list.len() < all);
        assert!(cr.list.iter().all(|s| s.id == 1));
        assert_eq!(cr.list.len(), 2);
        // Scrolling: rows of 9, minus the 5 shown.
        cr.select(0);
        cr.wheel(&Inventory::default(), -1);
        assert!(cr.scroll > 0.0);
    }
}
