//! `GuiStats`, the Statistics screen: four lists (general, blocks, items, mobs) over the counters
//! the server sent with `S37PacketStatistics`.

use super::achievements::draw_downloading;
use super::stats_list::{Table, BLOCKS, ITEMS};
use super::stats_table::{self as table, MOBS};
use super::{
    button, centered, default_background, draw_buttons, press, release_all, text, Button, Canvas,
    Ctx, Effect, Nav, PlayerStats, Screen, ScrollList, WHITE,
};

const DONE: i32 = 0;
const GENERAL: i32 = 1;
const BLOCKS_TAB: i32 = 2;
const ITEMS_TAB: i32 = 3;
const MOBS_TAB: i32 = 4;
const GRAY: u32 = 0x90_90_90;
const DARK_GRAY: u32 = 0x60_60_60;
/// `FontRenderer.FONT_HEIGHT`.
const FONT_H: i32 = 9;

/// What one of the four lists shows in its rows.
enum Body {
    General,
    Table(Table),
    /// Entity keys with a kill or killed-by count.
    Mobs(Vec<&'static str>),
}

/// One list of the screen with its scrolling.
struct Pane {
    list: ScrollList,
    body: Body,
}

impl Pane {
    /// The lists span the screen between the title and the buttons (`32` to `height - 64`).
    fn new(ctx: &Ctx, slot_h: i32, body: Body) -> Pane {
        let mut list = ScrollList::new(ctx.width, ctx.height, 32, ctx.height - 64, slot_h);
        if matches!(body, Body::Table(_)) {
            list.header = 20;
        }
        Pane { list, body }
    }

    fn rows(&self) -> usize {
        match &self.body {
            Body::General => table::GENERAL.len(),
            Body::Table(t) => t.len(),
            Body::Mobs(m) => m.len(),
        }
    }

    /// `GuiSlot.handleMouse` for a left click: start a drag, or press a header button.
    fn click(&mut self, x: i32, y: i32, ctx: &mut Ctx) {
        let rows = self.rows();
        self.list.press(x, y, rows);
        if let Body::Table(t) = &mut self.body {
            let l = &self.list;
            let left = ctx.width / 2 - l.row_w / 2;
            let above_rows = y - l.top - l.header + l.scroll as i32 - 4 < 0;
            if (l.top..=l.bottom).contains(&y) && (left..=left + l.row_w).contains(&x) && above_rows
            {
                t.header_clicked(x - left, ctx);
            }
        }
    }

    /// `GuiSlot.drawScreen`.
    fn draw(&mut self, c: &mut dyn Canvas, (mx, my): (i32, i32), left_down: bool, ctx: &Ctx) {
        let rows = self.rows();
        if left_down {
            self.list.drag(my, rows);
        }
        self.list.draw_back(c, rows);
        let (x, top) = (
            self.list.row_x(),
            self.list.top + 4 - self.list.scroll as i32,
        );
        if let Body::Table(t) = &mut self.body {
            t.draw_header(c, x, top, left_down);
        }
        for i in (0..rows).filter(|&i| self.list.row_visible(i)) {
            self.draw_row(c, i, x, self.list.row_y(i), ctx);
        }
        self.list.draw_front(c, rows);
        if let Body::Table(t) = &self.body {
            t.draw_hover(c, &self.list, mx, my, ctx);
        }
    }

    fn draw_row(&self, c: &mut dyn Canvas, i: usize, x: i32, y: i32, ctx: &Ctx) {
        match &self.body {
            Body::General => draw_general(c, i, x, y, ctx),
            Body::Table(t) => t.draw_row(c, i, x, y, ctx),
            Body::Mobs(m) => draw_mob(c, m[i], x, y, ctx),
        }
    }
}

/// `StatsGeneral.drawSlot`: the stat name and its value, right aligned.
fn draw_general(c: &mut dyn Canvas, i: usize, x: i32, y: i32, ctx: &Ctx) {
    let (key, format) = table::GENERAL[i];
    let color = if i.is_multiple_of(2) { WHITE } else { GRAY };
    text(c, &ctx.tr(key), x + 2, y + 1, color);
    let value = format.format(ctx.stats.read(key));
    let w = c.width(&value);
    text(c, &value, x + 2 + 213 - w, y + 1, color);
}

/// `StatsMobsList.drawSlot`: the mob name and its two sentences.
fn draw_mob(c: &mut dyn Canvas, mob: &str, x: i32, y: i32, ctx: &Ctx) {
    let name = ctx.tr(&format!("entity.{mob}.name"));
    let (kills, killed_by) = mob_counts(ctx.stats, mob);
    let kills_text = if kills == 0 {
        ctx.texts.fmt("stat.entityKills.none", &[&name])
    } else {
        ctx.texts
            .fmt("stat.entityKills", &[&kills.to_string(), &name])
    };
    let killed_by_text = if killed_by == 0 {
        ctx.texts.fmt("stat.entityKilledBy.none", &[&name])
    } else {
        ctx.texts
            .fmt("stat.entityKilledBy", &[&name, &killed_by.to_string()])
    };
    text(c, &name, x + 2 - 10, y + 1, WHITE);
    for (line, s, count) in [(1, kills_text, kills), (2, killed_by_text, killed_by)] {
        let color = if count == 0 { DARK_GRAY } else { GRAY };
        text(c, &s, x + 2, y + 1 + FONT_H * line, color);
    }
}

/// The kill and killed-by counters of one entity.
fn mob_counts(stats: &PlayerStats, mob: &str) -> (i32, i32) {
    (
        stats.read(&format!("stat.killEntity.{mob}")),
        stats.read(&format!("stat.entityKilledBy.{mob}")),
    )
}

/// The lists, built once the counters arrived (`createLists`).
struct Panes {
    general: Pane,
    blocks: Pane,
    items: Pane,
    mobs: Pane,
}

impl Panes {
    fn new(ctx: &Ctx) -> Panes {
        let mobs = MOBS
            .iter()
            .copied()
            .filter(|m| mob_counts(ctx.stats, m) != (0, 0))
            .collect();
        Panes {
            general: Pane::new(ctx, 10, Body::General),
            blocks: Pane::new(
                ctx,
                20,
                Body::Table(Table::new(&BLOCKS, table::MINED, ctx.stats)),
            ),
            items: Pane::new(
                ctx,
                20,
                Body::Table(Table::new(&ITEMS, table::USED, ctx.stats)),
            ),
            mobs: Pane::new(ctx, FONT_H * 4, Body::Mobs(mobs)),
        }
    }

    fn get(&mut self, tab: i32) -> &mut Pane {
        match tab {
            BLOCKS_TAB => &mut self.blocks,
            ITEMS_TAB => &mut self.items,
            MOBS_TAB => &mut self.mobs,
            _ => &mut self.general,
        }
    }
}

pub struct Stats {
    parent: Option<Box<dyn Screen>>,
    /// `downloadingStats`: the wait screen shows until the counters arrive.
    downloading: bool,
    buttons: Vec<Button>,
    panes: Option<Panes>,
    /// The tab button that chose the list on show.
    tab: i32,
    left_down: bool,
}

impl Stats {
    pub fn new(parent: Box<dyn Screen>) -> Stats {
        Stats {
            parent: Some(parent),
            downloading: true,
            buttons: Vec::new(),
            panes: None,
            tab: GENERAL,
            left_down: false,
        }
    }

    /// `onStatsReady` has no `Ctx`; build the lists and buttons at the next call that has one.
    fn sync(&mut self, ctx: &Ctx) {
        if self.downloading || self.panes.is_some() {
            return;
        }
        let panes = Panes::new(ctx);
        let (w, h) = (ctx.width, ctx.height);
        let mut buttons = vec![
            Button::new(DONE, w / 2 + 4, h - 28, 150, 20, ctx.tr("gui.done")),
            Button::new(
                GENERAL,
                w / 2 - 160,
                h - 52,
                80,
                20,
                ctx.tr("stat.generalButton"),
            ),
            Button::new(
                BLOCKS_TAB,
                w / 2 - 80,
                h - 52,
                80,
                20,
                ctx.tr("stat.blocksButton"),
            ),
            Button::new(ITEMS_TAB, w / 2, h - 52, 80, 20, ctx.tr("stat.itemsButton")),
            Button::new(
                MOBS_TAB,
                w / 2 + 80,
                h - 52,
                80,
                20,
                ctx.tr("stat.mobsButton"),
            ),
        ];
        for (id, pane) in [
            (BLOCKS_TAB, &panes.blocks),
            (ITEMS_TAB, &panes.items),
            (MOBS_TAB, &panes.mobs),
        ] {
            button(&mut buttons, id).enabled = pane.rows() > 0;
        }
        self.buttons = buttons;
        self.panes = Some(panes);
        self.tab = GENERAL;
    }
}

impl Screen for Stats {
    fn init(&mut self, ctx: &mut Ctx) {
        // `getNetHandler().addToSendQueue(new C16PacketClientStatus(REQUEST_STATS))`.
        ctx.effects.push(Effect::RequestStats);
        self.downloading = true;
        self.panes = None;
        self.buttons.clear();
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        self.sync(ctx);
        let Some(panes) = &mut self.panes else {
            draw_downloading(c, ctx);
            return;
        };
        default_background(c, ctx);
        panes.get(self.tab).draw(c, (mx, my), self.left_down, ctx);
        centered(c, &ctx.tr("gui.stats"), ctx.width / 2, 20, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        self.sync(ctx);
        if let Some(id) = press(&mut self.buttons, x, y, mb, ctx) {
            if id == DONE {
                return self.parent.take().map(Nav::Open);
            }
            self.tab = id;
            return None;
        }
        if mb == 0 {
            self.left_down = true;
            if let Some(panes) = &mut self.panes {
                panes.get(self.tab).click(x, y, ctx);
            }
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, b: i32, _ctx: &mut Ctx) {
        if b == 0 {
            self.left_down = false;
            if let Some(panes) = &mut self.panes {
                panes.get(self.tab).list.release();
            }
        }
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        if let Some(panes) = &mut self.panes {
            let pane = panes.get(self.tab);
            let rows = pane.rows();
            pane.list.wheel(dir, rows);
        }
    }

    fn pauses(&self) -> bool {
        !self.downloading
    }

    fn stats_ready(&mut self) {
        self.downloading = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::icons::Icons;
    use crate::screens::{Settings, Texts};
    use crate::toolkit::{Gui, GuiScale};
    use std::io::Read;

    struct Stub;
    impl Screen for Stub {
        fn init(&mut self, _: &mut Ctx) {}
        fn draw(&mut self, _: &mut dyn Canvas, _: i32, _: i32, _: &mut Ctx) {}
        fn click(&mut self, _: i32, _: i32, _: i32, _: &mut Ctx) -> Option<Nav> {
            None
        }
    }

    struct Font<'a>(crate::toolkit::Metrics<'a>);
    impl crate::screens::Metrics for Font<'_> {
        fn width(&self, s: &str) -> i32 {
            self.0.width(s)
        }
    }

    /// Opens the screen, answers with counters, and draws each of the four lists. `FERROCRAFT_CLIENT_JAR`
    /// gives the 1.8.9 client jar for textures and text; `STATS_DIR` gets one PNG per list.
    #[test]
    fn four_lists_draw_from_the_counters() {
        let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
        let read = |name: &str| -> Option<Vec<u8>> {
            let mut z = zip::ZipArchive::new(std::fs::File::open(jar.as_ref()?).ok()?).ok()?;
            let mut out = Vec::new();
            z.by_name(name).ok()?.read_to_end(&mut out).ok()?;
            Some(out)
        };
        let lang = read("assets/minecraft/lang/en_US.lang").unwrap_or_default();
        let texts = Texts::parse(&String::from_utf8_lossy(&lang), "");
        let icons = jar
            .is_some()
            .then(|| Icons::new(|n| read(&format!("assets/minecraft/{n}"))));
        let mut stats = PlayerStats::default();
        for (k, v) in [
            ("stat.walkOneCm", 123_456),
            ("stat.playOneMinute", 72_000),
            ("stat.jump", 1_234),
            ("stat.damageDealt", 456),
            ("stat.deaths", 3),
            ("stat.mineBlock.minecraft.stone", 300),
            ("stat.mineBlock.minecraft.dirt", 25),
            ("stat.craftItem.minecraft.planks", 40),
            ("stat.useItem.minecraft.stone", 12),
            ("stat.useItem.minecraft.iron_pickaxe", 80),
            ("stat.craftItem.minecraft.iron_pickaxe", 2),
            ("stat.breakItem.minecraft.iron_pickaxe", 1),
            ("stat.killEntity.Zombie", 7),
            ("stat.entityKilledBy.Creeper", 2),
        ] {
            stats.set(k, v);
        }
        let mut r = render::Renderer::new_offscreen(854, 480, jar.as_deref());
        let font = Font(crate::toolkit::Metrics(r.font()));
        let mut settings = Settings::new(std::path::PathBuf::from("/tmp"));
        let mut clip = String::new();
        let mut ctx = Ctx {
            width: 427,
            height: 240,
            texts: &texts,
            settings: &mut settings,
            in_world: true,
            difficulty: None,
            hardcore: false,
            now_ms: 0,
            game_dir: std::path::PathBuf::from("/tmp"),
            effects: Vec::new(),
            clipboard: &mut clip,
            metrics: &font,
            shift: false,
            stats: &stats,
            icons: icons.as_ref(),
            player_id: "Player",
        };
        let mut s = Stats::new(Box::new(Stub));
        s.init(&mut ctx);
        assert_eq!(ctx.effects, [Effect::RequestStats]);
        assert!(
            !s.pauses(),
            "the game keeps running while the counters load"
        );
        s.stats_ready();
        assert!(s.pauses());
        // Tab buttons sit at `height - 52`, 80 wide, from `width / 2 - 160`.
        let mut shots = Vec::new();
        for (name, tab_x) in [("general", 0), ("blocks", 1), ("items", 2), ("mobs", 3)] {
            let x = 427 / 2 - 160 + tab_x * 80 + 40;
            assert!(s.click(x, 240 - 52 + 10, 0, &mut ctx).is_none());
            s.release(x, 240 - 52 + 10, 0, &mut ctx);
            let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
            let mut c = crate::GuiCanvas::new(&mut g);
            s.draw(&mut c, 0, 0, &mut ctx);
            let quads = g.into_quads();
            let sprites = quads
                .iter()
                .filter(|q| q.tex == super::super::stats_list::STATS_ICONS)
                .count();
            // The tables draw a slot frame per column button and per row; the others none.
            assert_eq!(sprites > 0, matches!(name, "blocks" | "items"), "{name}");
            shots.push((name, quads));
        }
        if let (Some(_), Some(dir)) = (&jar, std::env::var_os("STATS_DIR")) {
            for (name, quads) in shots {
                let (w, h, rgba) = r.render_to_rgba(&render::Camera::default(), &quads);
                let path = std::path::Path::new(&dir).join(format!("stats_{name}.png"));
                render::encode_png(&path, w, h, &rgba).expect("write png");
            }
        }
    }
}
