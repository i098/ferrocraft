//! What the anvil, enchanting table, brewing stand, beacon and villager screens draw and answer
//! besides the slots: `GuiRepair`, `GuiEnchantment`, `GuiBrewingStand`, `GuiBeacon` and
//! `GuiMerchant` of 1.8.9.

use super::{modal, tooltip, Icons, Key, Mods, Request, LABEL};
use crate::screens::{key, Canvas, Metrics, TextField};
use render::JavaRandom;
use world::effects;
use world::inventory::{items, merchant, Inventory, ItemStack, Kind, Slot};

const SGA: &str = "textures/font/ascii_sga.png";
const ENCHANT: &str = "textures/gui/container/enchanting_table.png";
const BEACON: &str = "textures/gui/container/beacon.png";
const VILLAGER: &str = "textures/gui/container/villager.png";
const ANVIL: &str = "textures/gui/container/anvil.png";
const BREWING: &str = "textures/gui/container/brewing_stand.png";
const INVENTORY: &str = "textures/gui/container/inventory.png";
/// `0xFF6060`: the anvil cost text when the player cannot pay.
const RED: u32 = 0xFFFF_6060;
const GREEN: u32 = 0xFF80_FF20;

/// `EnchantmentNameParts`: the words of the table's standard galactic phrases.
const WORDS: &str = "the elder scrolls klaatu berata niktu xyzzy bless curse light darkness fire air earth water hot dry cold wet ignite snuff embiggen twist shorten stretch fiddle destroy imbue galvanize enchant free limited range of towards inside sphere cube self other ball mental physical grow shrink demon elemental spirit animal creature beast humanoid undead fresh stale";
/// Width of the letters A to Z in `ascii_sga.png`, from its glyph columns (`FontRenderer.readFontTexture`).
const SGA_WIDTHS: [i32; 26] = [
    6, 6, 3, 6, 6, 6, 4, 6, 2, 2, 6, 4, 6, 5, 5, 4, 6, 5, 3, 6, 6, 6, 6, 6, 4, 6,
];

/// `PotionEffect` ids of the beacon: tiers 1 to 3 give the primary power, tier 4 the secondary.
const TIERS: [&[i8]; 4] = [&[1, 3], &[11, 8], &[5], &[10]];
const REGENERATION: i8 = 10;
/// The four payment items drawn over the beacon window: emerald, diamond, gold, iron.
const PAYMENT: [i16; 4] = [388, 264, 266, 265];

/// Screen state beyond the slots: the anvil's name field.
pub struct Extras {
    name: TextField,
    /// Slot 0 of the anvil at the last tick (`GuiRepair.sendSlotContents`).
    seen: Slot,
}

impl Extras {
    pub fn new() -> Self {
        // `GuiRepair.initGui`: no background, white text, at most 40 characters.
        let mut name = TextField::new(62, 24, 103, 12);
        name.background = false;
        name.color = 0xFF_FFFF;
        name.max_len = 40;
        Extras { name, seen: None }
    }

    /// `GuiRepair.updateScreen` and `sendSlotContents`: a new stack in slot 0 fills the field and
    /// names the item.
    pub fn tick(&mut self, inv: &Inventory, m: &dyn Metrics) -> Vec<Request> {
        if inv.kind() != Kind::Anvil {
            return Vec::new();
        }
        self.name.tick();
        let now = inv.slot(0).clone();
        if now == self.seen {
            return Vec::new();
        }
        self.seen = now;
        let text = self
            .seen
            .as_ref()
            .map_or(String::new(), |s| tooltip::name(s).0);
        self.name.set(&text, m);
        self.name.set_focused(self.seen.is_some());
        if self.seen.is_some() {
            vec![self.rename(inv)]
        } else {
            Vec::new()
        }
    }

    /// `GuiRepair.renameItem`: `MC|ItemName` with the typed name; the default name is sent empty.
    fn rename(&self, inv: &Inventory) -> Request {
        let mut s = self.name.text.clone();
        if let Some(st) = inv.slot(0) {
            let (n, custom) = tooltip::name(st);
            if !custom && s == n {
                s.clear();
            }
        }
        Request::Plugin {
            channel: "MC|ItemName",
            data: string_payload(&s),
        }
    }

    /// `GuiRepair.keyTyped`: a focused name field takes the key. `Some` when it did.
    pub fn key(
        &mut self,
        inv: &Inventory,
        k: Key,
        typed: Option<char>,
        mods: Mods,
        m: &dyn Metrics,
    ) -> Option<Vec<Request>> {
        if inv.kind() != Kind::Anvil || !self.name.focused || k == Key::Escape {
            return None;
        }
        let (code, ch) = if k == Key::Backspace {
            (key::BACK, '\0')
        } else {
            (0, typed?)
        };
        let mods = crate::screens::Mods {
            ctrl: mods.ctrl,
            shift: mods.shift,
        };
        self.name
            .key(code, ch, mods, m, &mut String::new())
            .then(|| vec![self.rename(inv)])
    }

    /// `GuiScreen.mouseClicked` on the buttons of the window; `(x, y)` from its top-left.
    /// `Some` when a button took the click.
    pub fn click(
        &mut self,
        inv: &mut Inventory,
        x: i32,
        y: i32,
        button: u8,
    ) -> Option<Vec<Request>> {
        if button != 0 {
            return None;
        }
        match inv.kind() {
            Kind::Enchanting => click_enchant(inv, x, y),
            Kind::Merchant => click_merchant(inv, x, y),
            Kind::Beacon => click_beacon(inv, x, y),
            _ => None,
        }
    }

    /// `drawGuiContainerBackgroundLayer` beyond the plain texture; `(l, t)` is the window's corner.
    pub fn draw_back(&self, c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) {
        match inv.kind() {
            Kind::Anvil => anvil_back(c, inv, l, t),
            Kind::Brewing => brewing_back(c, inv, l, t),
            Kind::Enchanting => self.enchant_back(c, inv, l, t, mx, my),
            Kind::Merchant => merchant_back(c, inv, l, t, mx, my),
            Kind::Beacon => beacon_back(c, inv, l, t, mx, my),
            _ => {}
        }
    }

    /// `GuiEnchantment.drawGuiContainerBackgroundLayer`: the three option buttons with their
    /// standard galactic phrase.
    // shortcut: the open book (`EnchantmentTableBookModel`) is a 3D model and waits for the entity renderer.
    fn enchant_back(&self, c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) {
        let Some(w) = inv.open.as_ref() else { return };
        let lapis = w.own[1].as_ref().map_or(0, |s| i32::from(s.count));
        let mut rng = JavaRandom::new(i64::from(w.props[3]));
        for i in 0..3 {
            let (x, y) = (l + 60, t + 14 + 19 * i);
            let level = i32::from(w.props[i as usize]);
            if level == 0 {
                modal(c, ENCHANT, x, y, 0, 185, 108, 19);
                continue;
            }
            let number = level.to_string();
            let wrap = 86 - c.width(&number);
            let phrase = phrase(&mut rng);
            let (text, number_color) = if (lapis < i + 1 || inv.xp_level < level) && !inv.creative {
                modal(c, ENCHANT, x, y, 0, 185, 108, 19);
                modal(c, ENCHANT, x + 1, y + 1, 16 * i, 239, 16, 16);
                ((0x68_5E4A & 0xFE_FEFE) >> 1, 0x40_7F10)
            } else {
                let hover = mx >= x && my >= y && mx < x + 108 && my < y + 19;
                modal(c, ENCHANT, x, y, 0, if hover { 204 } else { 166 }, 108, 19);
                modal(c, ENCHANT, x + 1, y + 1, 16 * i, 223, 16, 16);
                (if hover { 0xFF_FF80 } else { 0x68_5E4A }, 0x80_FF20)
            };
            galactic(c, &phrase, x + 20, y + 2, wrap, text);
            let nx = x + 20 + 86 - c.width(&number);
            c.text(
                &number,
                nx as f32,
                (y + 9) as f32,
                0xFF00_0000 | number_color,
                true,
            );
        }
    }

    /// `drawGuiContainerForegroundLayer`, from the window's corner.
    pub fn draw_fore(&self, c: &mut dyn Canvas, inv: &Inventory, icons: &Icons) {
        let Some(w) = inv.open.as_ref() else { return };
        if inv.kind() == Kind::Anvil {
            self.name.draw(c);
            anvil_cost(c, inv, i32::from(w.props[0]));
        }
        if inv.kind() == Kind::Beacon {
            c.text(
                "Primary Power",
                (62 - c.width("Primary Power") / 2) as f32,
                10.0,
                0xFFE0_E0E0,
                true,
            );
            c.text(
                "Secondary Power",
                (169 - c.width("Secondary Power") / 2) as f32,
                10.0,
                0xFFE0_E0E0,
                true,
            );
            for (i, id) in PAYMENT.iter().enumerate() {
                let s = ItemStack {
                    id: *id,
                    count: 1,
                    damage: 0,
                    nbt: None,
                };
                icons.draw(c, &s, 42 + 24 * i as i32, 109);
            }
        }
    }

    /// The hover text of the enchanting table options and the beacon buttons, at the mouse.
    #[allow(clippy::too_many_arguments)]
    pub fn hover(
        &self,
        c: &mut dyn Canvas,
        inv: &Inventory,
        l: i32,
        t: i32,
        mx: i32,
        my: i32,
        sw: i32,
        sh: i32,
    ) {
        let lines = match inv.kind() {
            Kind::Enchanting => enchant_hover(inv, l, t, mx, my),
            Kind::Beacon => beacon_hover(inv, l, t, mx, my),
            _ => return,
        };
        if let Some(lines) = lines {
            tooltip::draw(c, &lines, mx, my, sw, sh);
        }
    }
}

fn inside(x: i32, y: i32, bx: i32, by: i32, w: i32, h: i32) -> bool {
    x >= bx && y >= by && x < bx + w && y < by + h
}

fn click_enchant(inv: &Inventory, x: i32, y: i32) -> Option<Vec<Request>> {
    let i = (0..3).find(|&i| inside(x, y, 60, 14 + 19 * i, 108, 19))?;
    let sent = inv.can_enchant(i as usize).then(|| Request::Enchant {
        window_id: inv.window_id(),
        button: i as i8,
    });
    Some(sent.into_iter().collect())
}

fn click_merchant(inv: &mut Inventory, x: i32, y: i32) -> Option<Vec<Request>> {
    let (len, at) = (
        inv.open.as_ref()?.recipes.len(),
        inv.open.as_ref()?.recipe_index,
    );
    let next = if inside(x, y, 147, 23, 12, 19) && at + 1 < len {
        at + 1
    } else if inside(x, y, 17, 23, 12, 19) && at > 0 {
        at - 1
    } else {
        return None;
    };
    inv.select_recipe(next);
    Some(vec![Request::Plugin {
        channel: "MC|TrSel",
        data: merchant::select_payload(next),
    }])
}

fn click_beacon(inv: &mut Inventory, x: i32, y: i32) -> Option<Vec<Request>> {
    let b = beacon_buttons(inv)
        .into_iter()
        .find(|b| b.enabled && inside(x, y, b.x, b.y, 22, 22))?;
    let w = inv.open.as_mut()?;
    match b.act {
        Act::Power { id, tier } => {
            w.props[if tier < 3 { 1 } else { 2 }] = i16::from(id);
            Some(Vec::new())
        }
        Act::Confirm => {
            let mut data = i32::from(w.props[1]).to_be_bytes().to_vec();
            data.extend(i32::from(w.props[2]).to_be_bytes());
            Some(vec![
                Request::Plugin {
                    channel: "MC|Beacon",
                    data,
                },
                Request::Close { window_id: w.id },
            ])
        }
        Act::Cancel => Some(vec![Request::Close { window_id: w.id }]),
    }
}

fn anvil_back(c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32) {
    let has = |i: usize| inv.slot(i).is_some();
    modal(
        c,
        ANVIL,
        l + 59,
        t + 20,
        0,
        166 + if has(0) { 0 } else { 16 },
        110,
        16,
    );
    if (has(0) || has(1)) && !has(2) {
        modal(c, ANVIL, l + 99, t + 56, 176, 0, 28, 21);
    }
}

fn brewing_back(c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32) {
    let Some(w) = inv.open.as_ref() else { return };
    let k = i32::from(w.props[0]);
    if k <= 0 {
        return;
    }
    let n = (28.0 * (1.0 - k as f32 / 400.0)) as i32;
    if n > 0 {
        modal(c, BREWING, l + 97, t + 16, 176, 0, 9, n);
    }
    let n = [29, 24, 20, 16, 11, 6, 0][(k / 2 % 7) as usize];
    if n > 0 {
        modal(c, BREWING, l + 65, t + 14 + 29 - n, 185, 29 - n, 12, n);
    }
}

fn merchant_back(c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) {
    let Some(w) = inv.open.as_ref() else { return };
    if w.recipes
        .get(w.recipe_index)
        .is_some_and(merchant::Recipe::disabled)
    {
        modal(c, VILLAGER, l + 83, t + 21, 212, 0, 28, 21);
        modal(c, VILLAGER, l + 83, t + 51, 212, 0, 28, 21);
    }
    let (len, at) = (w.recipes.len(), w.recipe_index);
    for (x, forward, enabled) in [(147, true, at + 1 < len), (17, false, at > 0)] {
        let hover = inside(mx, my, l + x, t + 23, 12, 19);
        let u = 176
            + if !enabled {
                23
            } else if hover {
                46
            } else {
                0
            };
        modal(
            c,
            VILLAGER,
            l + x,
            t + 23,
            u,
            if forward { 0 } else { 19 },
            12,
            19,
        );
    }
}

fn beacon_back(c: &mut dyn Canvas, inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) {
    for b in beacon_buttons(inv) {
        let hover = inside(mx, my, l + b.x, t + b.y, 22, 22);
        let u = if !b.enabled {
            44
        } else if b.selected {
            22
        } else if hover {
            66
        } else {
            0
        };
        modal(c, BEACON, l + b.x, t + b.y, u, 219, 22, 22);
        let (x, y) = (l + b.x + 2, t + b.y + 2);
        match b.act {
            Act::Power { id, .. } => {
                if let Some(i) = effects::icon(id) {
                    modal(c, INVENTORY, x, y, i % 8 * 18, 198 + i / 8 * 18, 18, 18);
                }
            }
            Act::Confirm => modal(c, BEACON, x, y, 90, 219, 18, 18),
            Act::Cancel => modal(c, BEACON, x, y, 112, 219, 18, 18),
        }
    }
}

fn enchant_hover(inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) -> Option<Vec<String>> {
    let w = inv.open.as_ref()?;
    let i = (0..3)
        .find(|&i| inside(mx - l, my - t, 60, 14 + 19 * i, 108, 17) && w.props[i as usize] > 0)?;
    let level = i32::from(w.props[i as usize]);
    let mut lines = Vec::new();
    let clue = items::enchantment(w.props[4 + i as usize])
        .and_then(|_| tooltip::enchantment_name(w.props[4 + i as usize], w.props[7 + i as usize]));
    if let Some(name) = &clue {
        lines.push(format!("\u{a7}f\u{a7}o{name} . . . ?"));
    }
    if !inv.creative {
        if clue.is_some() {
            lines.push(String::new());
        }
        let lapis = w.own[1].as_ref().map_or(0, |s| i32::from(s.count));
        enchant_cost_lines(&mut lines, inv.xp_level, level, i + 1, lapis);
    }
    Some(lines)
}

fn enchant_cost_lines(lines: &mut Vec<String>, xp: i32, level: i32, n: i32, lapis: i32) {
    if xp < level {
        lines.push(format!("\u{a7}cLevel Requirement: {level}"));
        return;
    }
    let color = if lapis >= n { "\u{a7}7" } else { "\u{a7}c" };
    lines.push(format!("{color}{n} Lapis Lazuli"));
    let s = if n == 1 {
        "1 Enchantment Level".into()
    } else {
        format!("{n} Enchantment Levels")
    };
    lines.push(format!("\u{a7}7{s}"));
}

fn beacon_hover(inv: &Inventory, l: i32, t: i32, mx: i32, my: i32) -> Option<Vec<String>> {
    let b = beacon_buttons(inv)
        .into_iter()
        .find(|b| inside(mx, my, l + b.x, t + b.y, 22, 22))?;
    Some(vec![match b.act {
        Act::Power { id, tier } => {
            let name = effects::name(id).unwrap_or("");
            if tier >= 3 && id != REGENERATION {
                format!("{name} II")
            } else {
                name.to_string()
            }
        }
        Act::Confirm => "Done".into(),
        Act::Cancel => "Cancel".into(),
    }])
}

/// `GuiRepair.drawGuiContainerForegroundLayer`: the title and the cost of the repair.
fn anvil_cost(c: &mut dyn Canvas, inv: &Inventory, cost: i32) {
    c.text("Repair & Name", 60.0, 6.0, LABEL, false);
    if cost <= 0 {
        return;
    }
    let (mut color, mut shown, mut text) = (GREEN, true, format!("Repair Cost: {cost}"));
    if cost >= 40 && !inv.creative {
        text = "Too Expensive!".into();
        color = RED;
    } else if inv.slot(2).is_none() {
        shown = false;
    } else if !inv.anvil_can_take() {
        color = RED;
    }
    if shown {
        let dark = 0xFF00_0000 | ((color & 0x00FC_FCFC) >> 2);
        let x = (176 - 8 - c.width(&text)) as f32;
        for (dx, dy) in [(0.0, 1.0), (1.0, 0.0), (1.0, 1.0)] {
            c.text(&text, x + dx, 67.0 + dy, dark, false);
        }
        c.text(&text, x, 67.0, color, false);
    }
}

#[derive(Clone, Copy)]
enum Act {
    Power { id: i8, tier: u8 },
    Confirm,
    Cancel,
}

struct Btn {
    x: i32,
    y: i32,
    act: Act,
    enabled: bool,
    selected: bool,
}

/// `GuiBeacon.updateScreen`: the done and cancel buttons and the power buttons the beacon
/// level allows.
fn beacon_buttons(inv: &Inventory) -> Vec<Btn> {
    let Some(w) = inv.open.as_ref() else {
        return Vec::new();
    };
    let (levels, primary, secondary) = (i32::from(w.props[0]), w.props[1], w.props[2]);
    let power = |x: i32, y: i32, id: i8, tier: u8, chosen: i16| Btn {
        x,
        y,
        act: Act::Power { id, tier },
        enabled: i32::from(tier) < levels,
        selected: i32::from(tier) < levels && i16::from(id) == chosen,
    };
    let mut out = vec![
        Btn {
            x: 164,
            y: 107,
            act: Act::Confirm,
            enabled: w.own[0].is_some() && primary > 0,
            selected: false,
        },
        Btn {
            x: 190,
            y: 107,
            act: Act::Cancel,
            enabled: true,
            selected: false,
        },
    ];
    for (tier, ids) in TIERS.iter().take(3).enumerate() {
        let k = ids.len() as i32;
        let width = k * 22 + (k - 1) * 2;
        for (m, &id) in ids.iter().enumerate() {
            out.push(power(
                76 + m as i32 * 24 - width / 2,
                22 + tier as i32 * 25,
                id,
                tier as u8,
                primary,
            ));
        }
    }
    let k = TIERS[3].len() as i32 + 1;
    let width = k * 22 + (k - 1) * 2;
    for (m, &id) in TIERS[3].iter().enumerate() {
        out.push(power(167 + m as i32 * 24 - width / 2, 47, id, 3, secondary));
    }
    if primary > 0 {
        out.push(power(
            167 + (k - 1) * 24 - width / 2,
            47,
            primary as i8,
            3,
            secondary,
        ));
    }
    out
}

/// `EnchantmentNameParts.generateNewRandomName`: three or four words.
fn phrase(rng: &mut JavaRandom) -> String {
    let words: Vec<&str> = WORDS.split(' ').collect();
    let n = rng.next_int(2) + 3;
    (0..n)
        .map(|_| words[rng.next_int(words.len() as i32) as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// `FontRenderer.getCharWidth` of the standard galactic font: only letters have glyphs.
fn galactic_width(c: char) -> i32 {
    match c {
        ' ' => 4,
        'a'..='z' | 'A'..='Z' => SGA_WIDTHS[(c.to_ascii_uppercase() as u8 - b'A') as usize],
        _ => 1,
    }
}

/// `FontRenderer.drawSplitString` in the standard galactic font, the first two lines, no shadow.
fn galactic(c: &mut dyn Canvas, text: &str, x: i32, y: i32, wrap: i32, rgb: u32) {
    let mut lines: Vec<String> = vec![String::new()];
    let mut used = 0;
    for word in text.split(' ') {
        let w: i32 = word.chars().map(galactic_width).sum();
        let line = lines.last_mut().expect("one line");
        if !line.is_empty() && used + 4 + w > wrap {
            lines.push(word.to_string());
            used = w;
        } else {
            if !line.is_empty() {
                line.push(' ');
                used += 4;
            }
            line.push_str(word);
            used += w;
        }
    }
    for (row, line) in lines.iter().take(2).enumerate() {
        let mut px = x as f32;
        for ch in line.chars() {
            let width = galactic_width(ch);
            if ch != ' ' && ch.is_ascii_alphabetic() {
                let (u, v) = ((ch as u32 % 16 * 8) as f32, (ch as u32 / 16 * 8) as f32);
                let w = width as f32 - 1.01;
                let uv = [u / 128.0, v / 128.0, (u + w) / 128.0, (v + 7.99) / 128.0];
                c.blit(
                    SGA,
                    px,
                    (y + 9 * row as i32) as f32,
                    w,
                    7.99,
                    uv,
                    0xFF00_0000 | rgb,
                );
            }
            px += width as f32;
        }
    }
}

/// `PacketBuffer.writeString`: the byte length as a var-int, then UTF-8.
fn string_payload(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut n = s.len();
    loop {
        let more = n >= 0x80;
        out.push((n & 0x7F) as u8 | if more { 0x80 } else { 0 });
        n >>= 7;
        if !more {
            break;
        }
    }
    out.extend(s.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_var_int_prefixed() {
        assert_eq!(string_payload("ab"), [2, b'a', b'b']);
        assert_eq!(string_payload(&"x".repeat(130))[..2], [0x82, 0x01]);
    }

    #[test]
    fn galactic_letters_have_glyph_widths_and_other_chars_one() {
        assert_eq!(galactic_width('a'), 6);
        assert_eq!(galactic_width('C'), 3);
        assert_eq!(galactic_width(' '), 4);
        assert_eq!(galactic_width('5'), 1);
    }
}
