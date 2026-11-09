//! `GuiPlayerTabOverlay`: the player list shown while Tab is held.

use crate::scoreboard::{Scoreboard, SLOT_LIST};
use crate::toolkit::{Gui, FONT_HEIGHT, ICONS};

const SPECTATOR: u8 = 3;
// shortcut: every head is Steve's; draw the player's skin when the renderer loads skins.
const STEVE: &str = "textures/entity/steve.png";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabEntry {
    pub uuid: u128,
    pub name: String,
    pub ping: i32,
    pub gamemode: u8,
    /// Display name with `§` codes; replaces the team-formatted name.
    pub display: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabList {
    pub entries: Vec<TabEntry>,
    /// Empty means none (vanilla sets `null` for empty components).
    pub header: String,
    pub footer: String,
    /// Online-mode connection: heads are drawn.
    pub encrypted: bool,
}

impl TabList {
    /// S38 add player: replaces an entry with the same uuid.
    pub fn add(&mut self, e: TabEntry) {
        self.remove(e.uuid);
        self.entries.push(e);
    }

    pub fn remove(&mut self, uuid: u128) {
        self.entries.retain(|e| e.uuid != uuid);
    }

    pub fn entry_mut(&mut self, uuid: u128) -> Option<&mut TabEntry> {
        self.entries.iter_mut().find(|e| e.uuid == uuid)
    }

    /// `getPlayerName`.
    fn name(e: &TabEntry, sb: &Scoreboard) -> String {
        e.display.clone().unwrap_or_else(|| sb.format_name(&e.name))
    }

    /// `renderPlayerlist`.
    pub fn draw(&self, gui: &mut Gui, sb: &Scoreboard) {
        let width = gui.scale.width;
        let mut list: Vec<&TabEntry> = self.entries.iter().collect();
        // `PlayerComparator`: spectators last, then team name, then player name.
        list.sort_by(|a, b| {
            (a.gamemode == SPECTATOR)
                .cmp(&(b.gamemode == SPECTATOR))
                .then_with(|| team(sb, a).cmp(team(sb, b)))
                .then_with(|| a.name.cmp(&b.name))
        });
        let objective = sb.objective_in(SLOT_LIST);
        let (mut i, mut j) = (0, 0);
        for e in &list {
            i = i.max(gui.width(&Self::name(e, sb)));
            if let Some(o) = objective {
                let s = sb.score(o, &e.name).unwrap_or(0);
                j = j.max(gui.width(&format!(" {s}")));
            }
        }
        list.truncate(80);
        let l3 = list.len() as i32;
        let (mut i4, mut j4) = (l3, 1);
        while i4 > 20 {
            j4 += 1;
            i4 = (l3 + j4 - 1) / j4;
        }
        let flag = self.encrypted;
        let head = if flag { 9 } else { 0 };
        // shortcut: integer objectives only; the hearts render type needs the S3B type field.
        let l = if objective.is_some() { j } else { 0 };
        let i1 = (j4 * (head + i + l + 13)).min(width - 50) / j4;
        let j1 = width / 2 - (i1 * j4 + (j4 - 1) * 5) / 2;
        let mut k1 = 10;
        let mut l1 = i1 * j4 + (j4 - 1) * 5;
        let wrap = |s: &str| -> Vec<String> {
            if s.is_empty() {
                Vec::new()
            } else {
                gui.metrics().wrap(s, width - 50)
            }
        };
        let (list1, list2) = (wrap(&self.header), wrap(&self.footer));
        for s in list1.iter().chain(&list2) {
            l1 = l1.max(gui.width(s));
        }
        let (bx0, bx1) = (width / 2 - l1 / 2 - 1, width / 2 + l1 / 2 + 1);
        if !list1.is_empty() {
            gui.fill(
                bx0,
                k1 - 1,
                bx1,
                k1 + list1.len() as i32 * FONT_HEIGHT,
                i32::MIN as u32,
            );
            for s in &list1 {
                gui.text(
                    s,
                    (width / 2 - gui.width(s) / 2) as f32,
                    k1 as f32,
                    0xFFFF_FFFF,
                );
                k1 += FONT_HEIGHT;
            }
            k1 += 1;
        }
        gui.fill(bx0, k1 - 1, bx1, k1 + i4 * 9, i32::MIN as u32);
        for (k4, e) in list.iter().enumerate() {
            let k4 = k4 as i32;
            let (l4, i5) = (k4 / i4, k4 % i4);
            let mut j2 = j1 + l4 * i1 + l4 * 5;
            let k2 = k1 + i5 * 9;
            gui.fill(j2, k2, j2 + i1, k2 + 8, 553_648_127);
            gui.set_color(0xFFFF_FFFF);
            if flag {
                let (x, y) = (j2 as f32, k2 as f32);
                gui.blit_tinted(
                    STEVE,
                    x,
                    y,
                    8.0,
                    8.0,
                    [0.125, 0.125, 0.25, 0.25],
                    0xFFFF_FFFF,
                );
                gui.blit_tinted(
                    STEVE,
                    x,
                    y,
                    8.0,
                    8.0,
                    [0.625, 0.125, 0.75, 0.25],
                    0xFFFF_FFFF,
                );
                j2 += 9;
            }
            let name = Self::name(e, sb);
            if e.gamemode == SPECTATOR {
                gui.text(&format!("\u{a7}o{name}"), j2 as f32, k2 as f32, 0x90FF_FFFF);
            } else {
                gui.text(&name, j2 as f32, k2 as f32, 0xFFFF_FFFF);
                if let Some(o) = objective {
                    let k5 = j2 + i + 1;
                    let l5 = k5 + l;
                    if l5 - k5 > 5 {
                        let s = format!("\u{a7}e{}", sb.score(o, &e.name).unwrap_or(0));
                        gui.text(&s, (l5 - gui.width(&s)) as f32, k2 as f32, 0xFFFF_FFFF);
                    }
                }
            }
            draw_ping(gui, i1, j2 - head, k2, e.ping);
        }
        if !list2.is_empty() {
            k1 += i4 * 9 + 1;
            gui.fill(
                bx0,
                k1 - 1,
                bx1,
                k1 + list2.len() as i32 * FONT_HEIGHT,
                i32::MIN as u32,
            );
            for s in &list2 {
                gui.text(
                    s,
                    (width / 2 - gui.width(s) / 2) as f32,
                    k1 as f32,
                    0xFFFF_FFFF,
                );
                k1 += FONT_HEIGHT;
            }
        }
    }
}

fn team<'a>(sb: &'a Scoreboard, e: &TabEntry) -> &'a str {
    sb.team_of(&e.name).map_or("", |(n, _)| n)
}

/// `drawPing`: the signal bars of `icons.png`.
fn draw_ping(gui: &mut Gui, w: i32, x: i32, y: i32, ping: i32) {
    let j = match ping {
        p if p < 0 => 5,
        p if p < 150 => 0,
        p if p < 300 => 1,
        p if p < 600 => 2,
        p if p < 1000 => 3,
        _ => 4,
    };
    gui.set_color(0xFFFF_FFFF);
    gui.blit(ICONS, x + w - 11, y, 0, 176 + j * 8, 10, 8);
}
