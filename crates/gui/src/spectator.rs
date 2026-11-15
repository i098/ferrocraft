//! `SpectatorGui` and `SpectatorMenu`: the nine slot bar a spectator opens with the wheel, a number
//! key or a middle click, with its player and team categories.

use crate::scoreboard::{prefix_color, Scoreboard};
use crate::tablist::{default_skin, head_uvs, TabEntry, TabList};
use crate::toolkit::{Gui, WIDGETS};
use render::Blend;

const SPECTATOR_WIDGETS: &str = "textures/gui/spectator_widgets.png";
const SPECTATOR: u8 = 3;
/// `SpectatorGui.getHotbarAlpha`: the bar stays 5 seconds and fades over the last 2.
const SHOW_MS: i64 = 5000;
const FADE_MS: f32 = 2000.0;

/// What `SpectatorMenuItem.select` asks the client to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    None,
    /// Send `C18PacketSpectate` for this player.
    Spectate(u128),
}

#[derive(Clone, Debug, PartialEq)]
struct Category {
    prompt: &'static str,
    items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
struct TeamItem {
    name: String,
    /// The color of the prefix's last formatting code.
    color: Option<u32>,
    members: Vec<TabEntry>,
    /// A member whose face the item shows.
    head: Option<TabEntry>,
}

/// A `SpectatorMenuItem`.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    /// `EMPTY_SLOT`.
    Empty,
    Close,
    /// `PaginationMenuItem`: `back` is -1, else +1.
    Page {
        back: bool,
        enabled: bool,
    },
    /// `TeleportToPlayerCategory`.
    Players(Category),
    /// `TeleportToTeamCategory`.
    Teams(Category),
    /// `PlayerMenuItem`.
    Player(TabEntry),
    /// `TeamMenuItem`.
    Team(TeamItem),
}

static EMPTY: Item = Item::Empty;
static CLOSE: Item = Item::Close;
static SCROLL_LEFT: Item = Item::Page {
    back: true,
    enabled: true,
};
static SCROLL_RIGHT_ENABLED: Item = Item::Page {
    back: false,
    enabled: true,
};
static SCROLL_RIGHT_DISABLED: Item = Item::Page {
    back: false,
    enabled: false,
};

impl Item {
    fn enabled(&self) -> bool {
        match self {
            Item::Empty => false,
            Item::Page { enabled, .. } => *enabled,
            Item::Players(c) => !c.items.is_empty(),
            Item::Teams(c) => c.items.iter().any(Item::enabled),
            Item::Team(t) => !t.members.is_empty(),
            Item::Close | Item::Player(_) => true,
        }
    }

    /// The `u` of the item's 16 x 16 icon in `spectator_widgets.png`; faces have none.
    fn icon_u(&self) -> Option<i32> {
        match self {
            Item::Players(_) => Some(0),
            Item::Teams(_) => Some(16),
            Item::Close => Some(128),
            Item::Page { back: true, .. } => Some(144),
            Item::Page { .. } => Some(160),
            _ => None,
        }
    }

    /// `getDisplayName`.
    fn name(&self) -> &str {
        match self {
            Item::Empty => "",
            Item::Close => "Close menu",
            Item::Page { back: true, .. } => "Previous Page",
            Item::Page { .. } => "Next Page",
            Item::Players(_) => "Teleport to player",
            Item::Teams(_) => "Teleport to team member",
            Item::Player(e) => &e.name,
            Item::Team(t) => &t.name,
        }
    }
}

/// `UUID.compareTo`: the signed high long, then the signed low long.
fn uuid_order(uuid: u128) -> (i64, i64) {
    ((uuid >> 64) as i64, uuid as i64)
}

impl Category {
    /// `TeleportToPlayerCategory(Collection)`: players sorted by UUID, spectators left out.
    fn players(mut list: Vec<TabEntry>) -> Category {
        list.sort_by_key(|e| uuid_order(e.uuid));
        let items = list
            .into_iter()
            .filter(|e| e.gamemode != SPECTATOR)
            .map(Item::Player)
            .collect();
        Category {
            prompt: "Select a player to teleport to",
            items,
        }
    }

    /// `TeleportToTeamCategory`; `seed` picks each team's face.
    fn teams(tab: &TabList, sb: &Scoreboard, seed: usize) -> Category {
        let mut teams: Vec<_> = sb.teams().collect();
        teams.sort_by_key(|(name, _)| *name);
        let items = teams
            .into_iter()
            .map(|(_, t)| {
                let members: Vec<TabEntry> = t
                    .members
                    .iter()
                    .filter_map(|m| tab.entries.iter().find(|e| e.name == *m))
                    .cloned()
                    .collect();
                let head = (!members.is_empty()).then(|| members[seed % members.len()].clone());
                Item::Team(TeamItem {
                    name: t.display_name.clone(),
                    color: prefix_color(&t.prefix),
                    members,
                    head,
                })
            })
            .collect();
        Category {
            prompt: "Select a team to teleport to",
            items,
        }
    }
}

/// The state of `SpectatorMenu`: the open category, the page and the selected slot.
#[derive(Clone, Debug, PartialEq)]
struct Menu {
    category: Category,
    selected: i32,
    page: i32,
}

impl Menu {
    /// `new SpectatorMenu`: the root category lists the players and the teams as they are now.
    fn new(tab: &TabList, sb: &Scoreboard, seed: usize) -> Menu {
        let players = Category::players(tab.entries.clone());
        let teams = Category::teams(tab, sb, seed);
        Menu {
            category: Category {
                prompt: "Press a key to select a command, and again to use it.",
                items: vec![Item::Players(players), Item::Teams(teams)],
            },
            selected: -1,
            page: 0,
        }
    }

    /// `getItem`: slot 0 is the back arrow past page 0, 7 the forward arrow and 8 the close button.
    fn item(&self, slot: i32) -> &Item {
        let i = slot + self.page * 6;
        let len = self.category.items.len() as i32;
        if self.page > 0 && slot == 0 {
            &SCROLL_LEFT
        } else if slot == 7 {
            if i < len {
                &SCROLL_RIGHT_ENABLED
            } else {
                &SCROLL_RIGHT_DISABLED
            }
        } else if slot == 8 {
            &CLOSE
        } else if (0..len).contains(&i) {
            &self.category.items[i as usize]
        } else {
            &EMPTY
        }
    }

    /// `SpectatorMenu.selectSlot`: the first press marks the slot, the second one uses it.
    /// `None` in the first field keeps the menu open; `Some(close)` is the item's result.
    fn select_slot(&mut self, slot: i32) -> Select {
        let item = self.item(slot);
        if *item == Item::Empty {
            return Select::Stay(Pick::None);
        }
        if self.selected != slot || !item.enabled() {
            self.selected = slot;
            return Select::Stay(Pick::None);
        }
        match item.clone() {
            Item::Close => Select::Close,
            Item::Page { back, .. } => {
                self.page += if back { -1 } else { 1 };
                Select::Stay(Pick::None)
            }
            Item::Players(c) | Item::Teams(c) => self.open(c),
            Item::Team(t) => self.open(Category::players(t.members)),
            Item::Player(e) => Select::Stay(Pick::Spectate(e.uuid)),
            Item::Empty => Select::Stay(Pick::None),
        }
    }

    /// `setCategory`.
    fn open(&mut self, category: Category) -> Select {
        self.category = category;
        self.selected = -1;
        self.page = 0;
        Select::Stay(Pick::None)
    }
}

enum Select {
    Stay(Pick),
    /// `SpectatorMenu.close`.
    Close,
}

/// `SpectatorGui`: the menu while it is open and the time of its last use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpectatorGui {
    menu: Option<Menu>,
    last_select_ms: i64,
    /// Key names of the nine hotbar keys, drawn on the slots.
    keys: [String; 9],
}

impl SpectatorGui {
    pub fn is_active(&self) -> bool {
        self.menu.is_some()
    }

    /// The names the nine hotbar keys show.
    pub fn set_keys(&mut self, keys: [String; 9]) {
        self.keys = keys;
    }

    /// `selectSlot`: a hotbar key opens the menu, or marks and uses the slot in an open one.
    pub fn select_slot(&mut self, slot: i32, now_ms: i64, tab: &TabList, sb: &Scoreboard) -> Pick {
        self.last_select_ms = now_ms;
        let Some(menu) = &mut self.menu else {
            self.menu = Some(Menu::new(tab, sb, now_ms as usize));
            return Pick::None;
        };
        match menu.select_slot(slot) {
            Select::Stay(o) => o,
            Select::Close => {
                self.close();
                Pick::None
            }
        }
    }

    /// `mouseMiddleClicked`: opens the menu, or uses the marked slot.
    pub fn middle_click(&mut self, now_ms: i64, tab: &TabList, sb: &Scoreboard) -> Pick {
        match self.menu.as_ref().map(|m| m.selected) {
            Some(slot) if slot != -1 => self.select_slot(slot, now_ms, tab, sb),
            Some(_) => {
                self.last_select_ms = now_ms;
                Pick::None
            }
            None => self.select_slot_open(now_ms, tab, sb),
        }
    }

    fn select_slot_open(&mut self, now_ms: i64, tab: &TabList, sb: &Scoreboard) -> Pick {
        self.last_select_ms = now_ms;
        self.menu = Some(Menu::new(tab, sb, now_ms as usize));
        Pick::None
    }

    /// `mouseScrolled`: one step along the enabled slots; `step` is -1 or +1.
    pub fn scroll(&mut self, step: i32, now_ms: i64) {
        let Some(menu) = &mut self.menu else { return };
        let mut slot = menu.selected + step;
        while (0..=8).contains(&slot) && !menu.item(slot).enabled() {
            slot += step;
        }
        if (0..=8).contains(&slot) {
            // The target differs from the marked slot, so this only marks it.
            menu.select_slot(slot);
            self.last_select_ms = now_ms;
        }
    }

    fn close(&mut self) {
        self.menu = None;
        self.last_select_ms = 0;
    }

    /// `getHotbarAlpha`.
    fn alpha(&self, now_ms: i64) -> f32 {
        ((self.last_select_ms - now_ms + SHOW_MS) as f32 / FADE_MS).clamp(0.0, 1.0)
    }

    /// `renderHotbar`: closes the menu once it has faded out.
    pub fn draw_hotbar(&mut self, gui: &mut Gui, now_ms: i64) {
        let Some(menu) = &self.menu else { return };
        let alpha = self.alpha(now_ms);
        if alpha <= 0.0 {
            self.close();
            return;
        }
        let (cx, y) = (gui.scale.width / 2, gui.scale.height as f32 - 22.0 * alpha);
        gui.set_blend(Blend::Alpha);
        let tint = tint(1.0, alpha);
        sprite(gui, WIDGETS, (cx - 91) as f32, y, (0, 0, 182, 22), tint);
        if menu.selected >= 0 {
            let x = (cx - 91 - 1 + menu.selected * 20) as f32;
            sprite(gui, WIDGETS, x, y - 1.0, (0, 22, 24, 22), tint);
        }
        for slot in 0..9 {
            let x = (cx - 90 + slot * 20 + 2) as f32;
            let key = &self.keys[slot as usize];
            draw_slot(gui, (x, y + 3.0), alpha, menu.item(slot), key);
        }
    }

    /// `renderTooltip`: the selected item's name, or the category's prompt.
    pub fn draw_tooltip(&self, gui: &mut Gui, now_ms: i64) {
        let alpha = (self.alpha(now_ms) * 255.0) as u32;
        let Some(menu) = self.menu.as_ref().filter(|_| alpha > 3) else {
            return;
        };
        let item = menu.item(menu.selected);
        let text = match item {
            Item::Empty => menu.category.prompt,
            item => item.name(),
        };
        let x = (gui.scale.width - gui.width(text)) / 2;
        let y = gui.scale.height - 35;
        gui.set_blend(Blend::Alpha);
        gui.text(text, x as f32, y as f32, 0xFF_FFFF | alpha << 24);
    }
}

/// `GlStateManager.color4f(b, b, b, alpha)` as an ARGB tint.
fn tint(b: f32, alpha: f32) -> u32 {
    let c = (b * 255.0) as u32;
    ((alpha * 255.0) as u32) << 24 | c << 16 | c << 8 | c
}

/// `drawTexturedModalRect` at a fractional position.
fn sprite(gui: &mut Gui, tex: &'static str, x: f32, y: f32, r: (i32, i32, i32, i32), argb: u32) {
    let (u, v, w, h) = (r.0 as f32, r.1 as f32, r.2 as f32, r.3 as f32);
    let uv = [u / 256.0, v / 256.0, (u + w) / 256.0, (v + h) / 256.0];
    gui.blit_tinted(tex, x, y, w, h, uv, argb);
}

/// `renderSlot`: the item dimmed to a quarter when it is disabled, and its hotbar key.
fn draw_slot(gui: &mut Gui, (x, y): (f32, f32), alpha: f32, item: &Item, key: &str) {
    if *item == Item::Empty {
        return;
    }
    let a = (alpha * 255.0) as u32;
    let b = if item.enabled() { 1.0 } else { 0.25 };
    draw_item(gui, (x, y), item, b, a);
    if a > 3 && item.enabled() {
        let kx = x + 19.0 - 2.0 - gui.width(key) as f32;
        gui.text(key, kx, y + 6.0 + 3.0, 0xFF_FFFF | a << 24);
    }
}

/// `SpectatorMenuItem.render(brightness, alpha)` at the slot corner.
fn draw_item(gui: &mut Gui, (x, y): (f32, f32), item: &Item, b: f32, a: u32) {
    if let Some(u) = item.icon_u() {
        let tint = tint(b, a as f32 / 255.0);
        sprite(gui, SPECTATOR_WIDGETS, x, y, (u, 0, 16, 16), tint);
    }
    match item {
        Item::Player(e) => draw_face(gui, (x, y), e, 1.0, a),
        Item::Team(t) => draw_team(gui, (x, y), t, b, a),
        _ => {}
    }
}

/// `TeamMenuItem.render`: the prefix color as a square, and the face of one member on it.
fn draw_team(gui: &mut Gui, (x, y): (f32, f32), t: &TeamItem, b: f32, a: u32) {
    if let Some(rgb) = t.color {
        let [r, g, bl] = [16, 8, 0].map(|s| ((rgb >> s & 0xFF) as f32 * b) as u32);
        let (x0, y0) = (x as i32 + 1, y as i32 + 1);
        gui.fill(x0, y0, x0 + 14, y0 + 14, a << 24 | r << 16 | g << 8 | bl);
    }
    if let Some(e) = &t.head {
        draw_face(gui, (x, y), e, b, a);
    }
}

/// The face and the hat layer at 12 x 12, 2 pixels into the slot.
fn draw_face(gui: &mut Gui, (x, y): (f32, f32), e: &TabEntry, b: f32, a: u32) {
    let skin = e
        .head
        .skin
        .clone()
        .unwrap_or_else(|| default_skin(e.uuid).to_owned());
    let argb = tint(b, a as f32 / 255.0);
    for uv in head_uvs(false) {
        gui.blit_tinted(skin.clone(), x + 2.0, y + 2.0, 12.0, 12.0, uv, argb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolkit::GuiScale;

    fn tab(names: &[(&str, u128, u8)]) -> TabList {
        let mut t = TabList::default();
        for (n, u, g) in names {
            t.add(TabEntry {
                uuid: *u,
                name: (*n).into(),
                gamemode: *g,
                ..TabEntry::default()
            });
        }
        t
    }

    fn open(tab: &TabList) -> SpectatorGui {
        let mut g = SpectatorGui::default();
        g.select_slot(0, 1000, tab, &Scoreboard::default());
        g
    }

    fn selected(g: &SpectatorGui) -> i32 {
        g.menu.as_ref().unwrap().selected
    }

    #[test]
    fn a_hotbar_key_opens_the_menu_with_no_slot_marked() {
        let g = open(&tab(&[("a", 1, 0)]));
        assert!(g.is_active());
        assert_eq!(selected(&g), -1);
    }

    #[test]
    fn scroll_skips_disabled_slots_and_stops_at_the_close_button() {
        let mut g = open(&tab(&[("a", 1, 0)]));
        g.scroll(1, 1100);
        assert_eq!(selected(&g), 0);
        // Slot 1 (teams, none), 2..6 (empty) and 7 (arrow, disabled) are skipped.
        g.scroll(1, 1200);
        assert_eq!(selected(&g), 8);
        // Past the last slot nothing changes; back goes to the players.
        g.scroll(1, 1300);
        assert_eq!(selected(&g), 8);
        g.scroll(-1, 1400);
        assert_eq!(selected(&g), 0);
    }

    #[test]
    fn using_a_marked_player_sends_its_uuid_and_spectators_are_not_listed() {
        let t = tab(&[("far", 1 << 100, 0), ("dead", 5, 3), ("near", 2, 0)]);
        let sb = Scoreboard::default();
        let mut g = open(&t);
        assert_eq!(g.select_slot(0, 1, &t, &sb), Pick::None);
        assert_eq!(g.select_slot(0, 2, &t, &sb), Pick::None);
        assert_eq!(selected(&g), -1);
        // UUID order puts 2 before 2^100; the spectator in between is absent.
        assert_eq!(g.select_slot(1, 3, &t, &sb), Pick::None);
        assert_eq!(g.select_slot(1, 4, &t, &sb), Pick::Spectate(1 << 100));
        assert_eq!(g.select_slot(0, 5, &t, &sb), Pick::None);
        assert_eq!(g.select_slot(0, 6, &t, &sb), Pick::Spectate(2));
    }

    #[test]
    fn page_two_starts_after_six_items_and_shows_the_back_arrow() {
        let names: Vec<_> = (0..14).map(|i| (format!("p{i}"), i as u128, 0)).collect();
        let list: Vec<_> = names.iter().map(|(n, u, g)| (n.as_str(), *u, *g)).collect();
        let t = tab(&list);
        let sb = Scoreboard::default();
        let mut g = open(&t);
        g.select_slot(0, 1, &t, &sb);
        g.select_slot(0, 2, &t, &sb);
        let m = g.menu.as_ref().unwrap();
        assert_eq!(m.item(7), &SCROLL_RIGHT_ENABLED);
        g.select_slot(7, 3, &t, &sb);
        g.select_slot(7, 4, &t, &sb);
        let m = g.menu.as_ref().unwrap();
        assert_eq!(m.item(0), &SCROLL_LEFT);
        // `getItem` reads `slot + page * 6`, so item 6 is never shown on page 1.
        assert!(matches!(m.item(1), Item::Player(e) if e.uuid == 7));
    }

    #[test]
    fn the_bar_fades_after_five_seconds_and_the_close_button_ends_it() {
        let t = tab(&[]);
        let mut g = open(&t);
        assert_eq!(g.alpha(1000), 1.0);
        assert_eq!(g.alpha(1000 + 4000), 0.5);
        let mut gui = Gui::new(GuiScale::new(854, 480, 2, false), None, 1);
        g.draw_hotbar(&mut gui, 1000 + 5000);
        assert!(!g.is_active());
        let sb = Scoreboard::default();
        let mut g = open(&t);
        g.select_slot(8, 1, &t, &sb);
        g.select_slot(8, 2, &t, &sb);
        assert!(!g.is_active());
    }

    #[test]
    fn middle_click_opens_then_uses_the_marked_slot() {
        let t = tab(&[("a", 9, 0)]);
        let sb = Scoreboard::default();
        let mut g = SpectatorGui::default();
        assert_eq!(g.middle_click(1, &t, &sb), Pick::None);
        assert!(g.is_active());
        g.scroll(1, 2);
        g.middle_click(3, &t, &sb);
        assert_eq!(selected(&g), -1);
        g.scroll(1, 4);
        assert_eq!(g.middle_click(5, &t, &sb), Pick::Spectate(9));
    }

    #[test]
    fn the_open_bar_draws_the_bar_the_cursor_and_four_icons() {
        let mut g = open(&tab(&[("a", 1, 0)]));
        g.scroll(1, 1000);
        let mut gui = Gui::new(GuiScale::new(854, 480, 2, false), None, 1);
        g.draw_hotbar(&mut gui, 1000);
        let quads = gui.into_quads();
        let from = |tex: &str| quads.iter().filter(|q| &*q.tex == tex).count();
        assert_eq!(from(WIDGETS), 2);
        // Players, teams, the disabled next page arrow and close.
        assert_eq!(from(SPECTATOR_WIDGETS), 4);
    }

    #[test]
    fn a_team_shows_the_color_of_its_prefix() {
        assert_eq!(prefix_color("\u{a7}cRed "), Some(0xFF5555));
        assert_eq!(prefix_color("Plain"), None);
    }

    #[test]
    fn a_team_item_shows_the_display_name_not_the_key() {
        let mut sb = Scoreboard::default();
        let info = crate::scoreboard::TeamInfo {
            display_name: "Red Team".into(),
            ..Default::default()
        };
        sb.set_team("red", 0, &info, &[]);
        let Item::Team(t) = &Category::teams(&TabList::default(), &sb, 0).items[0] else {
            panic!("no team item");
        };
        assert_eq!(t.name, "Red Team");
    }
}
