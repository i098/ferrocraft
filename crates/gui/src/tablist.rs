//! `GuiPlayerTabOverlay`: the player list shown while Tab is held.

use crate::scoreboard::{Scoreboard, SLOT_LIST};
use crate::toolkit::{Gui, FONT_HEIGHT, ICONS};

const SPECTATOR: u8 = 3;
/// `l` of `renderPlayerlist` for the `hearts` type.
const HEARTS_WIDTH: i32 = 90;
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
    pub hearts: HeartState,
}

/// The health fields of `PlayerInfo` that animate the heart row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeartState {
    health: i32,
    last_health: i32,
    /// `Minecraft.getTime` of the last change.
    last_update: i64,
    /// `GameGui.getTicks` until which the row flashes.
    flash_until: i64,
    /// The list's `visibility` stamp at the last draw; 0 before the first.
    render_visibility: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabList {
    pub entries: Vec<TabEntry>,
    /// Empty means none (vanilla sets `null` for empty components).
    pub header: String,
    pub footer: String,
    /// Online-mode connection: heads are drawn.
    pub encrypted: bool,
    /// `visibility`: the time the list last opened.
    opened: i64,
    visible: bool,
}

impl TabList {
    /// S38 add player: replaces an entry with the same uuid.
    pub fn add(&mut self, e: TabEntry) {
        self.remove(e.uuid);
        self.entries.push(e);
    }

    /// `setVisible`, called each frame with the key state.
    pub fn set_visible(&mut self, visible: bool, now_ms: i64) {
        if visible && !self.visible {
            self.opened = now_ms;
        }
        self.visible = visible;
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
    pub fn draw(&mut self, gui: &mut Gui, sb: &Scoreboard, clock: Clock) {
        let width = gui.scale.width;
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        // `PlayerComparator`: spectators last, then team name, then player name.
        order.sort_by(|&a, &b| {
            let (a, b) = (&self.entries[a], &self.entries[b]);
            (a.gamemode == SPECTATOR)
                .cmp(&(b.gamemode == SPECTATOR))
                .then_with(|| team(sb, a).cmp(team(sb, b)))
                .then_with(|| a.name.cmp(&b.name))
        });
        let objective = sb.objective_in(SLOT_LIST);
        let hearts = objective.is_some_and(|o| sb.is_hearts(o));
        let (i, j) = Self::widths(gui, order.iter().map(|&k| &self.entries[k]), sb, objective);
        order.truncate(80);
        let l3 = order.len() as i32;
        let (mut i4, mut j4) = (l3, 1);
        while i4 > 20 {
            j4 += 1;
            i4 = (l3 + j4 - 1) / j4;
        }
        let flag = self.encrypted;
        let head = if flag { 9 } else { 0 };
        let l = match objective {
            Some(_) if hearts => HEARTS_WIDTH,
            Some(_) => j,
            None => 0,
        };
        let i1 = (j4 * (head + i + l + 13)).min(width - 50) / j4;
        let j1 = width / 2 - (i1 * j4 + (j4 - 1) * 5) / 2;
        let mut k1 = 10;
        let mut l1 = i1 * j4 + (j4 - 1) * 5;
        let (list1, list2) = (
            wrap(gui, &self.header, width),
            wrap(gui, &self.footer, width),
        );
        for s in list1.iter().chain(&list2) {
            l1 = l1.max(gui.width(s));
        }
        let (bx0, bx1) = (width / 2 - l1 / 2 - 1, width / 2 + l1 / 2 + 1);
        if !list1.is_empty() {
            k1 = draw_lines(gui, &list1, (bx0, bx1), k1, width) + 1;
        }
        gui.fill(bx0, k1 - 1, bx1, k1 + i4 * 9, i32::MIN as u32);
        for (k4, &entry) in order.iter().enumerate() {
            let e = &mut self.entries[entry];
            let k4 = k4 as i32;
            let (l4, i5) = (k4 / i4, k4 % i4);
            let mut j2 = j1 + l4 * i1 + l4 * 5;
            let k2 = k1 + i5 * 9;
            gui.fill(j2, k2, j2 + i1, k2 + 8, 553_648_127);
            gui.set_color(0xFFFF_FFFF);
            if flag {
                draw_head(gui, j2, k2);
                j2 += 9;
            }
            let opened = self.opened;
            Self::draw_name(
                gui,
                e,
                sb,
                objective,
                (j2, k2),
                (j2 + i + 1, l),
                (opened, clock),
            );
            draw_ping(gui, i1, j2 - head, k2, e.ping);
        }
        if !list2.is_empty() {
            draw_lines(gui, &list2, (bx0, bx1), k1 + i4 * 9 + 1, width);
        }
    }

    /// Widest name and widest ` <score>` of `list`.
    fn widths<'a>(
        gui: &Gui,
        list: impl Iterator<Item = &'a TabEntry>,
        sb: &Scoreboard,
        objective: Option<&str>,
    ) -> (i32, i32) {
        let (mut i, mut j) = (0, 0);
        for e in list {
            i = i.max(gui.width(&Self::name(e, sb)));
            if let Some(o) = objective.filter(|o| !sb.is_hearts(o)) {
                let s = sb.score(o, &e.name).unwrap_or(0);
                j = j.max(gui.width(&format!(" {s}")));
            }
        }
        (i, j)
    }

    /// The player name at `(x, y)` and its score right aligned in the `l` wide column at `score_x`.
    fn draw_name(
        gui: &mut Gui,
        e: &mut TabEntry,
        sb: &Scoreboard,
        objective: Option<&str>,
        (x, y): (i32, i32),
        (score_x, l): (i32, i32),
        (opened, clock): (i64, Clock),
    ) {
        let name = Self::name(e, sb);
        if e.gamemode == SPECTATOR {
            gui.text(&format!("\u{a7}o{name}"), x as f32, y as f32, 0x90FF_FFFF);
            return;
        }
        gui.text(&name, x as f32, y as f32, 0xFFFF_FFFF);
        if let Some(o) = objective.filter(|_| l > 5) {
            let score = sb.score(o, &e.name).unwrap_or(0);
            if sb.is_hearts(o) {
                draw_hearts(
                    gui,
                    &mut e.hearts,
                    score,
                    (opened, clock),
                    (score_x, score_x + l, y),
                );
                return;
            }
            let s = format!("\u{a7}e{score}");
            gui.text(
                &s,
                (score_x + l - gui.width(&s)) as f32,
                y as f32,
                0xFFFF_FFFF,
            );
        }
    }
}

/// The time sources of the heart animation: `Minecraft.getTime` in ms and `GameGui.getTicks`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Clock {
    pub now_ms: i64,
    pub ticks: i32,
}

/// `renderDisplayScore` for the `hearts` type: the heart row, or `<n>hp` when each heart is under 3 px wide.
fn draw_hearts(
    gui: &mut Gui,
    h: &mut HeartState,
    score: i32,
    (opened, clock): (i64, Clock),
    (x1, x2, y): (i32, i32, i32),
) {
    h.track(score, opened, clock);
    let shown = (score.max(h.health) as f32 / 2.0).ceil() as i32;
    let total = (score / 2).max((h.health / 2).max(10));
    if shown <= 0 {
        return;
    }
    let step = (((x2 - x1 - 4) as f32) / total as f32).min(9.0);
    if step > 3.0 {
        let flash = h.flash_until > i64::from(clock.ticks)
            && (h.flash_until - i64::from(clock.ticks)) / 3 % 2 == 1;
        draw_heart_icons(gui, h, score, flash, (shown, total, step), (x1, y));
    } else {
        draw_hearts_text(gui, score, (x1, x2, y));
    }
}

impl HeartState {
    /// The `PlayerInfo` bookkeeping at the top of `renderDisplayScore`.
    fn track(&mut self, score: i32, opened: i64, clock: Clock) {
        let (now, ticks) = (clock.now_ms, i64::from(clock.ticks));
        if opened == self.render_visibility {
            if score < self.last_health {
                self.last_update = now;
                self.flash_until = ticks + 20;
            } else if score > self.last_health {
                self.last_update = now;
                self.flash_until = ticks + 10;
            }
        }
        if now - self.last_update > 1000 || opened != self.render_visibility {
            self.last_health = score;
            self.health = score;
            self.last_update = now;
        }
        self.render_visibility = opened;
        self.last_health = score;
    }
}

/// `shown` hearts of `total` containers, `step` px apart, over the empty containers.
fn draw_heart_icons(
    gui: &mut Gui,
    h: &HeartState,
    score: i32,
    flash: bool,
    (shown, total, step): (i32, i32, f32),
    (x1, y): (i32, i32),
) {
    let heart = |gui: &mut Gui, m: i32, u: i32| {
        let x = x1 as f32 + m as f32 * step;
        let uv = [u as f32 / 256.0, 0.0, (u + 9) as f32 / 256.0, 9.0 / 256.0];
        gui.blit_tinted(ICONS, x, y as f32, 9.0, 9.0, uv, 0xFFFF_FFFF);
    };
    let container = if flash { 25 } else { 16 };
    gui.set_color(0xFFFF_FFFF);
    for m in shown..total {
        heart(gui, m, container);
    }
    for m in 0..shown {
        heart(gui, m, container);
        let half = m * 2 + 1;
        if flash {
            if half < h.health {
                heart(gui, m, 70);
            }
            if half == h.health {
                heart(gui, m, 79);
            }
        }
        let u = if m >= 10 { [160, 169] } else { [52, 61] };
        if half < score {
            heart(gui, m, u[0]);
        }
        if half == score {
            heart(gui, m, u[1]);
        }
    }
}

/// The `<n>hp` text, green for full health and red for none.
fn draw_hearts_text(gui: &mut Gui, score: i32, (x1, x2, y): (i32, i32, i32)) {
    let g = (score as f32 / 20.0).clamp(0.0, 1.0);
    let color = ((((1.0 - g) * 255.0) as u32) << 16) | (((g * 255.0) as u32) << 8);
    let mut s = format!("{:?}", score as f32 / 2.0);
    if x2 - gui.width(&format!("{s}hp")) >= x1 {
        s.push_str("hp");
    }
    let x = (x2 + x1) / 2 - gui.width(&s) / 2;
    gui.text(&s, x as f32, y as f32, 0xFF00_0000 | color);
}

/// The head and its hat layer; every head is Steve's.
fn draw_head(gui: &mut Gui, x: i32, y: i32) {
    let (x, y) = (x as f32, y as f32);
    for uv in [[0.125, 0.125, 0.25, 0.25], [0.625, 0.125, 0.75, 0.25]] {
        gui.blit_tinted(STEVE, x, y, 8.0, 8.0, uv, 0xFFFF_FFFF);
    }
}

fn wrap(gui: &Gui, s: &str, width: i32) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        gui.metrics().wrap(s, width - 50)
    }
}

/// Dark box behind `lines`, then the lines centered from `y`; returns the y below them.
fn draw_lines(
    gui: &mut Gui,
    lines: &[String],
    (bx0, bx1): (i32, i32),
    mut y: i32,
    width: i32,
) -> i32 {
    gui.fill(
        bx0,
        y - 1,
        bx1,
        y + lines.len() as i32 * FONT_HEIGHT,
        i32::MIN as u32,
    );
    for s in lines {
        gui.text(
            s,
            (width / 2 - gui.width(s) / 2) as f32,
            y as f32,
            0xFFFF_FFFF,
        );
        y += FONT_HEIGHT;
    }
    y
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolkit::GuiScale;

    /// One quad per drawn 9 px icon: 10 containers, 7 full hearts and a half heart for a score of 15.
    #[test]
    fn fifteen_points_draw_eight_hearts_of_ten() {
        let mut gui = Gui::new(GuiScale::new(854, 480, 0, false), None, 0);
        let mut h = HeartState::default();
        let clock = Clock {
            now_ms: 5000,
            ticks: 100,
        };
        draw_hearts(&mut gui, &mut h, 15, (1, clock), (0, 100, 0));
        assert_eq!(gui.into_quads().len(), 10 + 7 + 1);
        assert_eq!((h.health, h.last_health), (15, 15));
    }

    /// A drop of health keeps the old container count and flashes for 20 ticks.
    #[test]
    fn damage_flashes_the_row() {
        let mut gui = Gui::new(GuiScale::new(854, 480, 0, false), None, 0);
        let mut h = HeartState::default();
        let clock = |now_ms, ticks| Clock { now_ms, ticks };
        draw_hearts(&mut gui, &mut h, 20, (1, clock(5000, 100)), (0, 100, 0));
        draw_hearts(&mut gui, &mut h, 20, (1, clock(5050, 101)), (0, 100, 0));
        draw_hearts(&mut gui, &mut h, 14, (1, clock(5100, 102)), (0, 100, 0));
        assert_eq!((h.health, h.last_health, h.flash_until), (20, 14, 122));
    }
}
