//! Player inventory and open containers of Minecraft 1.8.9, ported from vanilla `Container`.
//!
//! The client predicts each click with the same `slotClick` code as the server, then sends
//! `ClickWindow` with the stack that `slotClick` returned. The server answers with
//! `ConfirmTransaction` and corrects any slot that differs with `SetSlot`.
//! Window 0 is `ContainerPlayer`: 0 craft result, 1-4 craft grid, 5-8 armor (helmet first),
//! 9-35 main, 36-44 hotbar. Every other window is its own slots, then the 27 main slots, then
//! the 9 hotbar slots, which are the same stacks as window 0 slots 9-44.
//! Not ported: crafting and smelting recipes (the server sends the result slot), and the
//! brewing, enchanting, anvil, beacon, villager and horse windows.

pub mod items;

pub use ferrocraft_protocol::codec::{ItemStack, Slot};
pub use ferrocraft_protocol::nbt::{Compound, Nbt, Tag};
use items::Item;

/// Slot id of a click outside the window: drop the cursor stack.
pub const OUTSIDE: i16 = -999;
pub const PLAYER_SLOTS: usize = 45;
const PLAYER_PART: usize = 36;

/// Container type, from the `OpenWindow` type string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Player,
    /// `minecraft:chest` and `minecraft:container`: rows of 9 (1 to 6).
    Chest(u8),
    Crafting,
    Furnace,
    /// `minecraft:dispenser` and `minecraft:dropper`.
    Dispenser,
    Hopper,
}

impl Kind {
    /// `None` for window types this client does not draw yet.
    pub fn from_open(window_type: &str, slots: u8) -> Option<Kind> {
        Some(match window_type {
            "minecraft:chest" | "minecraft:container" => Kind::Chest(slots / 9),
            "minecraft:crafting_table" => Kind::Crafting,
            "minecraft:furnace" => Kind::Furnace,
            "minecraft:dispenser" | "minecraft:dropper" => Kind::Dispenser,
            "minecraft:hopper" => Kind::Hopper,
            _ => return None,
        })
    }

    /// Slots of the container itself, before the player part.
    pub fn own_slots(self) -> usize {
        match self {
            Kind::Player => 9,
            Kind::Chest(rows) => usize::from(rows) * 9,
            Kind::Crafting => 10,
            Kind::Furnace => 3,
            Kind::Dispenser => 9,
            Kind::Hopper => 5,
        }
    }

    /// Index of the crafting result slot, if any.
    fn result_slot(self) -> Option<usize> {
        matches!(self, Kind::Player | Kind::Crafting).then_some(0)
    }
}

/// A container window opened by the server.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: u8,
    pub kind: Kind,
    /// Title as plain text (the caller flattens the chat JSON of `OpenWindow`).
    pub title: String,
    pub own: Vec<Slot>,
    /// `WindowProperty` values (furnace: 0 burn time left, 1 burn time total, 2 cook progress, 3 cook total).
    pub props: [i16; 4],
    transaction: i16,
}

/// What `ClickWindow` (0x0E) carries.
#[derive(Clone, Debug, PartialEq)]
pub struct Click {
    pub window_id: u8,
    pub slot: i16,
    pub button: i8,
    pub action: i16,
    pub mode: i8,
    pub clicked_item: Slot,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Drag {
    /// `dragEvent`: 0 none, 1 adding slots, 2 done.
    event: u8,
    /// `dragMode`: 0 split evenly, 1 one each, 2 full stacks (creative).
    mode: u8,
    slots: Vec<usize>,
}

/// `InventoryPlayer` plus `ContainerPlayer` and the open container of `EntityPlayer`.
#[derive(Clone, Debug)]
pub struct Inventory {
    /// Window 0 slots.
    pub player: Vec<Slot>,
    /// The stack on the mouse cursor (`InventoryPlayer.itemStack`).
    pub cursor: Slot,
    /// Selected hotbar slot, 0 to 8.
    pub held: u8,
    pub open: Option<Window>,
    pub creative: bool,
    player_transaction: i16,
    drag: Drag,
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory {
            player: vec![None; PLAYER_SLOTS],
            cursor: None,
            held: 0,
            open: None,
            creative: false,
            player_transaction: 0,
            drag: Drag::default(),
        }
    }
}

fn info(stack: &ItemStack) -> &'static Item {
    items::item(stack.id)
}

/// `ItemStack.isStackable`.
fn stackable(s: &ItemStack) -> bool {
    let i = info(s);
    i.max_stack > 1 && !(i.max_damage > 0 && s.damage > 0)
}

/// `ItemStack.isItemEqual`: same item and damage.
fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.id == b.id && a.damage == b.damage
}

/// Same item, same damage when the item has subtypes, same tag: what `mergeItemStack` joins.
fn mergeable(a: &ItemStack, b: &ItemStack) -> bool {
    a.id == b.id && (!info(a).subtypes || a.damage == b.damage) && a.nbt == b.nbt
}

pub fn max_stack(s: &ItemStack) -> i32 {
    i32::from(info(s).max_stack)
}

fn count(slot: &Slot) -> i32 {
    slot.as_ref().map_or(0, |s| i32::from(s.count))
}

fn with_count(s: &ItemStack, n: i32) -> ItemStack {
    ItemStack {
        count: n as i8,
        ..s.clone()
    }
}

/// `Slot.decrStackSize` on a slot that holds a stack: takes up to `n` items.
fn take(slot: &mut Slot, n: i32) -> Slot {
    let s = slot.as_mut()?;
    let n = n.min(i32::from(s.count));
    s.count -= n as i8;
    let taken = with_count(s, n);
    if s.count <= 0 {
        *slot = None;
    }
    Some(taken)
}

impl Inventory {
    /// The window that clicks act on: the open container, else window 0.
    pub fn window_id(&self) -> u8 {
        self.open.as_ref().map_or(0, |w| w.id)
    }

    pub fn kind(&self) -> Kind {
        self.open.as_ref().map_or(Kind::Player, |w| w.kind)
    }

    /// Number of slots in the active window.
    pub fn slot_count(&self) -> usize {
        match &self.open {
            Some(w) => w.own.len() + PLAYER_PART,
            None => PLAYER_SLOTS,
        }
    }

    /// Slot `i` of the active window.
    pub fn slot(&self, i: usize) -> &Slot {
        match &self.open {
            Some(w) if i < w.own.len() => &w.own[i],
            Some(w) => &self.player[9 + i - w.own.len()],
            None => &self.player[i],
        }
    }

    pub fn slot_mut(&mut self, i: usize) -> &mut Slot {
        let own = self.open.as_ref().map_or(0, |w| w.own.len());
        match &mut self.open {
            Some(w) if i < own => &mut w.own[i],
            Some(_) => &mut self.player[9 + i - own],
            None => &mut self.player[i],
        }
    }

    /// The stack in the selected hotbar slot.
    pub fn held_stack(&self) -> &Slot {
        &self.player[36 + usize::from(self.held)]
    }

    /// `S2D OpenWindow`. Unknown types return false; the caller closes them with `CloseWindow`.
    pub fn open_window(&mut self, id: u8, window_type: &str, title: String, slots: u8) -> bool {
        self.drag = Drag::default();
        let Some(kind) = Kind::from_open(window_type, slots) else {
            return false;
        };
        let own = kind.own_slots();
        self.open = Some(Window {
            id,
            kind,
            title,
            own: vec![None; own],
            props: [0; 4],
            transaction: 0,
        });
        true
    }

    /// `S2E CloseWindow` and the local close (`EntityPlayerSP.closeScreenAndDropStack`): the
    /// cursor stack is dropped (the server drops it too). Closing the player inventory also
    /// empties its craft grid (`ContainerPlayer.onContainerClosed`).
    pub fn close_window(&mut self) {
        self.cursor = None;
        self.drag = Drag::default();
        if self.open.take().is_none() {
            for s in &mut self.player[0..5] {
                *s = None;
            }
        }
    }

    /// `S2F SetSlot`. Window -1 slot -1 is the cursor; window 0 is always the player.
    pub fn set_slot(&mut self, window_id: i8, slot: i16, item: Slot) {
        if window_id == -1 && slot == -1 {
            self.cursor = item;
            return;
        }
        let Ok(i) = usize::try_from(slot) else { return };
        if window_id == 0 {
            if let Some(s) = self.player.get_mut(i) {
                *s = item;
            }
        } else if self
            .open
            .as_ref()
            .is_some_and(|w| i8::try_from(w.id) == Ok(window_id))
            && i < self.slot_count()
        {
            *self.slot_mut(i) = item;
        }
    }

    /// `S30 WindowItems`.
    pub fn set_items(&mut self, window_id: u8, items: Vec<Slot>) {
        if window_id == 0 {
            for (s, item) in self.player.iter_mut().zip(items) {
                *s = item;
            }
        } else if self.window_id() == window_id {
            let n = self.slot_count();
            for (i, item) in items.into_iter().take(n).enumerate() {
                *self.slot_mut(i) = item;
            }
        }
    }

    /// `S31 WindowProperty`.
    pub fn set_property(&mut self, window_id: u8, property: i16, value: i16) {
        if let Some(w) = self.open.as_mut().filter(|w| w.id == window_id) {
            if let Some(p) = w.props.get_mut(property as usize) {
                *p = value;
            }
        }
    }

    /// `Container.getNextTransactionID`.
    fn next_action(&mut self) -> i16 {
        let t = match &mut self.open {
            Some(w) => &mut w.transaction,
            None => &mut self.player_transaction,
        };
        *t = t.wrapping_add(1);
        *t
    }

    /// `PlayerControllerMP.windowClick`: predict the click locally and return the packet.
    pub fn click(&mut self, slot: i16, button: i8, mode: i8) -> Click {
        let action = self.next_action();
        let clicked_item = self.slot_click(i32::from(slot), i32::from(button), mode);
        Click {
            window_id: self.window_id(),
            slot,
            button,
            action,
            mode,
            clicked_item,
        }
    }

    // ---- per-slot rules: `Slot.isItemValid`, `getItemStackLimit`, `Container.canMergeSlot` ----

    pub fn slot_accepts(&self, i: usize, s: &ItemStack) -> bool {
        match (self.kind(), i) {
            (Kind::Player | Kind::Crafting, 0) | (Kind::Furnace, 2) => false,
            (Kind::Player, 5..=8) => armor_fits(s, i - 5),
            (Kind::Furnace, 1) => items::is_fuel(s.id, s.damage) || s.id == items::BUCKET,
            _ => true,
        }
    }

    pub fn slot_limit(&self, i: usize, s: &ItemStack) -> i32 {
        match (self.kind(), i) {
            (Kind::Player, 5..=8) => 1,
            (Kind::Furnace, 1) if s.id == items::BUCKET => 1,
            _ => 64,
        }
    }

    fn can_merge(&self, i: usize) -> bool {
        self.kind().result_slot() != Some(i)
    }

    /// `Slot.onPickupFromSlot`: taking a crafting result uses one item from each grid slot.
    /// shortcut: container items (buckets, bottles) are not given back; the server's SetSlot fixes them.
    fn on_take(&mut self, i: usize) {
        let grid = match self.kind() {
            Kind::Player => 1..5,
            Kind::Crafting => 1..10,
            _ => return,
        };
        if i != 0 {
            return;
        }
        for g in grid {
            take(self.slot_mut(g), 1);
        }
    }

    /// `Container.slotClick` of 1.8.9; returns the stack that `ClickWindow` carries.
    pub fn slot_click(&mut self, slot_id: i32, button: i32, mode: i8) -> Slot {
        match mode {
            5 => self.drag_click(slot_id, button),
            _ if self.drag.event != 0 => self.drag = Drag::default(),
            0 | 1 if button == 0 || button == 1 => return self.pickup_click(slot_id, button, mode),
            2 if (0..9).contains(&button) => {
                if let Some(i) = self.window_slot(slot_id) {
                    self.swap_hotbar(i, button as usize);
                }
            }
            3 => self.creative_pick(slot_id),
            4 => self.drop_click(slot_id, button),
            6 => {
                if let Some(i) = self.window_slot(slot_id) {
                    self.collect(i, button);
                }
            }
            _ => {}
        }
        None
    }

    /// Mode 3 (middle click) in creative: a full stack of the slot's item on the cursor.
    fn creative_pick(&mut self, slot_id: i32) {
        let Some(i) = self.window_slot(slot_id) else {
            return;
        };
        if !self.creative || self.cursor.is_some() {
            return;
        }
        if let Some(s) = self.slot(i) {
            self.cursor = Some(with_count(s, max_stack(s)));
        }
    }

    /// Mode 4 (drop key): button 0 drops one item of the slot, button 1 the whole stack.
    fn drop_click(&mut self, slot_id: i32, button: i32) {
        let Some(i) = self.window_slot(slot_id) else {
            return;
        };
        if self.cursor.is_some() {
            return;
        }
        if let Some(n) = self.slot(i).as_ref().map(|s| i32::from(s.count)) {
            take(self.slot_mut(i), if button == 0 { 1 } else { n });
            self.on_take(i);
        }
    }

    /// `slot_id` as a slot index of the open window, if it is one.
    fn window_slot(&self, slot_id: i32) -> Option<usize> {
        usize::try_from(slot_id)
            .ok()
            .filter(|&i| i < self.slot_count())
    }

    /// Modes 0 (click) and 1 (shift click) with button 0 or 1.
    fn pickup_click(&mut self, slot_id: i32, button: i32, mode: i8) -> Slot {
        if slot_id == i32::from(OUTSIDE) {
            if button == 0 {
                self.cursor = None;
            } else {
                take(&mut self.cursor, 1);
            }
            return None;
        }
        let i = self.window_slot(slot_id)?;
        if mode == 0 {
            let result = self.slot(i).clone();
            self.pick_or_place(i, button);
            return result;
        }
        let moved = self.transfer(i)?;
        if self.slot(i).as_ref().is_some_and(|s| s.id == moved.id) {
            self.slot_click(slot_id, button, 1);
        }
        Some(moved)
    }

    /// Mode 0 with button 0 (whole stack) or 1 (half / one).
    fn pick_or_place(&mut self, i: usize, button: i32) {
        let cursor = self.cursor.take();
        let in_slot = self.slot(i).clone();
        match (in_slot, cursor) {
            (None, None) => {}
            (None, Some(mut c)) => {
                if self.slot_accepts(i, &c) {
                    let k = (if button == 0 { i32::from(c.count) } else { 1 })
                        .min(self.slot_limit(i, &c));
                    if i32::from(c.count) >= k {
                        *self.slot_mut(i) = Some(with_count(&c, k));
                        c.count -= k as i8;
                    }
                }
                self.cursor = (c.count > 0).then_some(c);
            }
            (Some(s), None) => {
                let n = if button == 0 {
                    i32::from(s.count)
                } else {
                    (i32::from(s.count) + 1) / 2
                };
                self.cursor = take(self.slot_mut(i), n);
                self.on_take(i);
            }
            (Some(s), Some(c)) if self.slot_accepts(i, &c) => self.place_onto(i, s, c, button),
            (Some(s), Some(c)) => self.take_onto_cursor(i, s, c),
        }
    }

    /// The cursor stack `c` over a stack `s` that slot `i` accepts: add to it or swap.
    fn place_onto(&mut self, i: usize, s: ItemStack, mut c: ItemStack, button: i32) {
        if same_item(&s, &c) && s.nbt == c.nbt {
            let mut k = if button == 0 { i32::from(c.count) } else { 1 };
            k = k.min(self.slot_limit(i, &c) - i32::from(s.count));
            k = k.min(max_stack(&c) - i32::from(s.count));
            c.count -= k as i8;
            if let Some(slot) = self.slot_mut(i) {
                slot.count += k as i8;
            }
            self.cursor = (c.count != 0).then_some(c);
        } else if i32::from(c.count) <= self.slot_limit(i, &c) {
            *self.slot_mut(i) = Some(c);
            self.cursor = Some(s);
        } else {
            self.cursor = Some(c);
        }
    }

    /// The cursor stack `c` over a stack `s` in an output slot: take all of `s` if it fits.
    fn take_onto_cursor(&mut self, i: usize, s: ItemStack, mut c: ItemStack) {
        if s.id == c.id
            && max_stack(&c) > 1
            && (!info(&s).subtypes || s.damage == c.damage)
            && s.nbt == c.nbt
        {
            let n = i32::from(s.count);
            if n > 0 && n + i32::from(c.count) <= max_stack(&c) {
                c.count += n as i8;
                take(self.slot_mut(i), n);
                self.on_take(i);
            }
        }
        self.cursor = Some(c);
    }

    /// Mode 2: swap slot `i` with hotbar slot `key` (number keys over a slot).
    fn swap_hotbar(&mut self, i: usize, key: usize) {
        let own_player = self.open.as_ref().map_or(i >= 5, |w| i >= w.own.len());
        let hot = self.player[36 + key].clone();
        let hot_valid = hot
            .as_ref()
            .is_none_or(|h| own_player && self.slot_accepts(i, h));
        let first_empty = (!hot_valid).then(|| self.first_empty()).flatten();
        let ok = hot_valid || first_empty.is_some();
        match self.slot(i).clone() {
            Some(s) if ok => {
                self.player[36 + key] = Some(s);
                if hot_valid {
                    *self.slot_mut(i) = hot;
                    self.on_take(i);
                } else if let Some(h) = hot {
                    self.add_to_inventory(h);
                    *self.slot_mut(i) = None;
                    self.on_take(i);
                }
            }
            None => {
                if let Some(h) = hot.filter(|h| self.slot_accepts(i, h)) {
                    self.player[36 + key] = None;
                    *self.slot_mut(i) = Some(h);
                }
            }
            Some(_) => {}
        }
    }

    /// `InventoryPlayer.getFirstEmptyStack`, as a window 0 slot: hotbar first, then main.
    fn first_empty(&self) -> Option<usize> {
        (36..45).chain(9..36).find(|&j| self.player[j].is_none())
    }

    /// `InventoryPlayer.addItemStackToInventory`: fill matching stacks, then empty slots.
    fn add_to_inventory(&mut self, mut s: ItemStack) {
        if stackable(&s) {
            for j in (36..45).chain(9..36) {
                if let Some(t) = self.player[j].as_mut() {
                    if t.id == s.id
                        && (!info(&s).subtypes || t.damage == s.damage)
                        && t.nbt == s.nbt
                    {
                        let room = (max_stack(t) - i32::from(t.count))
                            .max(0)
                            .min(i32::from(s.count));
                        t.count += room as i8;
                        s.count -= room as i8;
                        if s.count == 0 {
                            return;
                        }
                    }
                }
            }
        }
        if let Some(j) = self.first_empty() {
            self.player[j] = Some(s);
        }
    }

    /// Mode 6: double click collects matching stacks onto the cursor.
    fn collect(&mut self, i: usize, button: i32) {
        let Some(mut c) = self.cursor.take() else {
            return;
        };
        if self.slot(i).is_none() {
            let n = self.slot_count();
            for pass in 0..2 {
                let order: Box<dyn Iterator<Item = usize>> = if button == 0 {
                    Box::new(0..n)
                } else {
                    Box::new((0..n).rev())
                };
                for j in order {
                    if i32::from(c.count) >= max_stack(&c) {
                        break;
                    }
                    let Some(s) = self.slot(j).clone() else {
                        continue;
                    };
                    if fits(Some(&s), &c, true)
                        && self.can_merge(j)
                        && (pass != 0 || i32::from(s.count) != max_stack(&s))
                    {
                        let k = (max_stack(&c) - i32::from(c.count)).min(i32::from(s.count));
                        take(self.slot_mut(j), k);
                        c.count += k as i8;
                        self.on_take(j);
                    }
                }
            }
        }
        self.cursor = Some(c);
    }

    /// Mode 5: painting a stack over slots (`dragEvent` 0 start, 1 add slot, 2 end).
    fn drag_click(&mut self, slot_id: i32, button: i32) {
        let last = self.drag.event;
        self.drag.event = (button & 3) as u8;
        if (last != 1 || self.drag.event != 2) && last != self.drag.event {
            self.drag = Drag::default();
            return;
        }
        let Some(c) = self.cursor.clone() else {
            self.drag = Drag::default();
            return;
        };
        match self.drag.event {
            0 => self.drag_start(button),
            1 => self.drag_add(slot_id, &c),
            2 => self.drag_end(&c),
            _ => self.drag = Drag::default(),
        }
    }

    /// Drag event 0: the drag mode sits in bits 2 and 3 of the button.
    fn drag_start(&mut self, button: i32) {
        let mode = ((button >> 2) & 3) as u8;
        self.drag = if mode == 0 || mode == 1 || (mode == 2 && self.creative) {
            Drag {
                event: 1,
                mode,
                slots: Vec::new(),
            }
        } else {
            Drag::default()
        };
    }

    /// Drag event 1: paint slot `slot_id` if the cursor stack `c` can still go there.
    fn drag_add(&mut self, slot_id: i32, c: &ItemStack) {
        let Some(i) = self.window_slot(slot_id) else {
            return;
        };
        if fits(self.slot(i).as_ref(), c, true)
            && self.slot_accepts(i, c)
            && i32::from(c.count) > self.drag.slots.len() as i32
            && !self.drag.slots.contains(&i)
        {
            self.drag.slots.push(i);
        }
    }

    /// Drag event 2: share the cursor stack `c` over the painted slots.
    fn drag_end(&mut self, c: &ItemStack) {
        let drag = std::mem::take(&mut self.drag);
        if drag.slots.is_empty() {
            return;
        }
        let mut left = i32::from(c.count);
        for &i in &drag.slots {
            if !(fits(self.slot(i).as_ref(), c, true)
                && self.slot_accepts(i, c)
                && i32::from(c.count) >= drag.slots.len() as i32)
            {
                continue;
            }
            let had = count(self.slot(i));
            let mut n = drag_share(drag.mode, c, drag.slots.len()) + had;
            n = n.min(max_stack(c)).min(self.slot_limit(i, c));
            left -= n - had;
            *self.slot_mut(i) = Some(with_count(c, n));
        }
        self.cursor = (left > 0).then(|| with_count(c, left));
    }

    /// `Container.mergeItemStack` over window slots `start..end`.
    fn merge(&mut self, s: &mut ItemStack, start: usize, end: usize, reverse: bool) -> bool {
        let order: Vec<usize> = if reverse {
            (start..end).rev().collect()
        } else {
            (start..end).collect()
        };
        let max = max_stack(s);
        let mut moved = false;
        if stackable(s) {
            for &i in &order {
                if s.count <= 0 {
                    break;
                }
                let Some(t) = self.slot_mut(i).as_mut() else {
                    continue;
                };
                if mergeable(t, s) {
                    let joined = i32::from(t.count) + i32::from(s.count);
                    if joined <= max {
                        s.count = 0;
                        t.count = joined as i8;
                        moved = true;
                    } else if i32::from(t.count) < max {
                        s.count -= (max - i32::from(t.count)) as i8;
                        t.count = max as i8;
                        moved = true;
                    }
                }
            }
        }
        if s.count > 0 {
            if let Some(&i) = order.iter().find(|&&i| self.slot(i).is_none()) {
                *self.slot_mut(i) = Some(s.clone());
                s.count = 0;
                moved = true;
            }
        }
        moved
    }

    /// `transferStackInSlot` (shift click) of each container; returns the stack before the move.
    fn transfer(&mut self, i: usize) -> Slot {
        let original = self.slot(i).clone()?;
        let mut s = original.clone();
        let kind = self.kind();
        if !self.transfer_merge(kind, i, &mut s) {
            return None;
        }
        *self.slot_mut(i) = (s.count > 0).then(|| s.clone());
        // chests return the stack even when part of it stayed; the others return nothing then
        let partial_ok = matches!(kind, Kind::Chest(_) | Kind::Hopper);
        if s.count == original.count && !partial_ok {
            return None;
        }
        if kind.result_slot() == Some(i) {
            self.on_take(i);
        }
        Some(original)
    }

    /// The `mergeItemStack` calls of each container's `transferStackInSlot` for slot `i`.
    fn transfer_merge(&mut self, kind: Kind, i: usize, s: &mut ItemStack) -> bool {
        match kind {
            Kind::Player => self.transfer_player(i, s),
            Kind::Crafting => match i {
                0 => self.merge(s, 10, 46, true),
                10..=36 => self.merge(s, 37, 46, false),
                37..=45 => self.merge(s, 10, 37, false),
                _ => self.merge(s, 10, 46, false),
            },
            Kind::Furnace => self.transfer_furnace(i, s),
            Kind::Chest(_) | Kind::Dispenser | Kind::Hopper => {
                let n = self.slot_count();
                let own = n - PLAYER_PART;
                if i < own {
                    self.merge(s, own, n, true)
                } else {
                    self.merge(s, 0, own, false)
                }
            }
        }
    }

    /// `ContainerFurnace.transferStackInSlot`: smeltables go to the input, fuel to the fuel slot.
    fn transfer_furnace(&mut self, i: usize, s: &mut ItemStack) -> bool {
        match i {
            2 => self.merge(s, 3, 39, true),
            0 | 1 => self.merge(s, 3, 39, false),
            _ if items::smelts(s.id, s.damage) => self.merge(s, 0, 1, false),
            _ if items::is_fuel(s.id, s.damage) => self.merge(s, 1, 2, false),
            3..=29 => self.merge(s, 30, 39, false),
            _ => self.merge(s, 3, 30, false),
        }
    }

    /// `ContainerPlayer.transferStackInSlot`: armor goes to its empty armor slot first.
    fn transfer_player(&mut self, i: usize, s: &mut ItemStack) -> bool {
        match i {
            0 => self.merge(s, 9, 45, true),
            1..=8 => self.merge(s, 9, 45, false),
            _ => match items::item(s.id).armor {
                Some(a) if self.player[5 + usize::from(a)].is_none() => {
                    let a = 5 + usize::from(a);
                    self.merge(s, a, a + 1, false)
                }
                _ if i < 36 => self.merge(s, 36, 45, false),
                _ => self.merge(s, 9, 36, false),
            },
        }
    }
}

/// `ItemArmor.armorType` fit for armor slot `k` (0 helmet); pumpkins and skulls fit the helmet.
fn armor_fits(s: &ItemStack, k: usize) -> bool {
    match items::item(s.id).armor {
        Some(a) => usize::from(a) == k,
        None => k == 0 && (s.id == items::PUMPKIN || s.id == items::SKULL),
    }
}

/// `Container.canAddItemToSlot`.
pub fn fits(slot: Option<&ItemStack>, s: &ItemStack, size_matters: bool) -> bool {
    match slot {
        None => true,
        Some(t) => {
            same_item(s, t)
                && t.nbt == s.nbt
                && i32::from(t.count) + if size_matters { 0 } else { i32::from(s.count) }
                    <= max_stack(s)
        }
    }
}

/// `Container.computeStackSize` without the existing count; `GuiContainer` previews drags with it.
pub fn drag_share(mode: u8, s: &ItemStack, slots: usize) -> i32 {
    match mode {
        0 => (f32::from(s.count) / slots as f32).floor() as i32,
        1 => 1,
        _ => max_stack(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(id: i16, count: i8) -> Slot {
        Some(ItemStack {
            id,
            count,
            damage: 0,
            nbt: None,
        })
    }

    #[test]
    fn right_click_takes_half_rounded_up_and_reports_the_slot_before() {
        let mut inv = Inventory::default();
        inv.player[36] = stack(1, 5);
        let c = inv.click(36, 1, 0);
        assert_eq!((c.action, c.clicked_item), (1, stack(1, 5)));
        assert_eq!(
            (inv.cursor.clone(), inv.player[36].clone()),
            (stack(1, 3), stack(1, 2))
        );
    }

    #[test]
    fn placing_on_a_full_neighbor_swaps_and_partial_merges_cap_at_max() {
        let mut inv = Inventory::default();
        inv.player[9] = stack(1, 60);
        inv.cursor = stack(1, 10);
        inv.click(9, 0, 0);
        assert_eq!(
            (inv.player[9].clone(), inv.cursor.clone()),
            (stack(1, 64), stack(1, 6))
        );
        inv.player[10] = stack(4, 1);
        inv.click(10, 0, 0);
        assert_eq!(
            (inv.player[10].clone(), inv.cursor.clone()),
            (stack(1, 6), stack(4, 1))
        );
    }

    #[test]
    fn shift_click_moves_main_to_hotbar_and_armor_to_its_slot() {
        let mut inv = Inventory::default();
        inv.player[40] = stack(1, 30);
        inv.player[9] = stack(1, 50);
        let c = inv.click(9, 0, 1);
        assert_eq!(c.clicked_item, stack(1, 50));
        assert_eq!(
            (
                inv.player[40].clone(),
                inv.player[36].clone(),
                inv.player[9].clone()
            ),
            (stack(1, 64), stack(1, 16), None)
        );
        inv.player[20] = stack(items::DIAMOND_CHESTPLATE, 1);
        inv.click(20, 0, 1);
        assert_eq!(inv.player[6], stack(items::DIAMOND_CHESTPLATE, 1));
    }

    #[test]
    fn chest_shift_click_fills_the_player_part_from_the_end() {
        let mut inv = Inventory::default();
        assert!(inv.open_window(3, "minecraft:chest", "{}".into(), 27));
        inv.slot_mut(0).clone_from(&stack(1, 64));
        inv.click(0, 0, 1);
        // last window slot is hotbar 8 = window 0 slot 44
        assert_eq!(inv.player[44], stack(1, 64));
        assert_eq!(inv.slot(0), &None);
    }

    #[test]
    fn even_drag_splits_and_keeps_the_remainder_on_the_cursor() {
        let mut inv = Inventory {
            cursor: stack(1, 10),
            ..Inventory::default()
        };
        inv.click(-999, 0, 5);
        for s in [9, 10, 11] {
            inv.click(s, 1, 5);
        }
        inv.click(-999, 2, 5);
        assert_eq!(inv.player[9..12], [stack(1, 3), stack(1, 3), stack(1, 3)]);
        assert_eq!(inv.cursor, stack(1, 1));
    }

    #[test]
    fn number_key_swaps_with_the_hotbar_and_double_click_collects() {
        let mut inv = Inventory::default();
        inv.player[9] = stack(1, 5);
        inv.click(9, 2, 2);
        assert_eq!(
            (inv.player[38].clone(), inv.player[9].clone()),
            (stack(1, 5), None)
        );
        inv.player[20] = stack(1, 7);
        inv.cursor = stack(1, 1);
        inv.click(21, 0, 6);
        assert_eq!(
            (
                inv.cursor.clone(),
                inv.player[38].clone(),
                inv.player[20].clone()
            ),
            (stack(1, 13), None, None)
        );
    }
}
