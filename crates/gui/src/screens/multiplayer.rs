//! `GuiMultiplayer` with its server list and status pings, `GuiScreenAddServer` and `GuiScreenServerList`
//! (Direct Connect).

use super::{
    answer, blit256, centered, default_background, draw_buttons, key, press, release_all, text,
    tooltip, wrap, Button, Canvas, Connecting, Ctx, Mods, Nav, Screen, ScrollList, TextField,
    YesNo, GRAY_LABEL, ICONS, WHITE,
};
use ferrocraft_net_client::{
    base64, load_servers, ping, save_servers, split_address, PingError, ServerEntry, ServerStatus,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};

const UNKNOWN_SERVER: &str = "textures/misc/unknown_server.png";
const SELECTION: &str = "textures/gui/server_selection.png";
const PROTOCOL: i32 = 47;
const ROW_W: i32 = 305;

/// `ServerData` status fields while and after the ping.
#[derive(Debug, Clone, PartialEq)]
enum Ping {
    Pending,
    Failed(&'static str),
    Done(ServerStatus),
}

impl Ping {
    fn status(&self) -> Option<&ServerStatus> {
        match self {
            Ping::Done(s) => Some(s),
            _ => None,
        }
    }

    fn motd(&self) -> String {
        match self {
            Ping::Pending => String::new(),
            Ping::Failed(m) => format!("§4{m}"),
            Ping::Done(s) => s.motd.clone(),
        }
    }
}

struct Row {
    entry: ServerEntry,
    ping: Ping,
    /// Ties ping answers to the row after moves and deletes.
    key: u64,
    /// The icon string last uploaded to the canvas (`ServerListEntryNormal.field_148299_g`).
    icon_shown: Option<String>,
    /// `field_148298_f`: time of the last click, for double clicks.
    last_click: u64,
}

/// What the open child screen answers for (`GuiMultiplayer` flags `u`, `v`, `w`, `x`).
#[derive(Clone, Copy, PartialEq)]
enum Pending {
    Delete,
    Add,
    Edit,
    Direct,
}

const CANCEL: i32 = 0;
const SELECT: i32 = 1;
const DELETE: i32 = 2;
const ADD: i32 = 3;
const DIRECT: i32 = 4;
const EDIT: i32 = 7;
const REFRESH: i32 = 8;

/// A finished status ping for the row with this key.
type PingDone = (u64, Result<ServerStatus, PingError>);

pub struct Multiplayer {
    parent: Option<Box<dyn Screen>>,
    rows: Vec<Row>,
    selected: Option<usize>,
    list: ScrollList,
    buttons: Vec<Button>,
    started: bool,
    next_key: u64,
    pings: (Sender<PingDone>, Receiver<PingDone>),
    pending: Option<Pending>,
    /// The `ServerData` a child screen edits in place, as vanilla shares it.
    edit: Rc<RefCell<ServerEntry>>,
    held: bool,
    servers_dat: PathBuf,
}

impl Multiplayer {
    pub fn new(parent: Box<dyn Screen>) -> Multiplayer {
        Multiplayer {
            parent: Some(parent),
            rows: Vec::new(),
            selected: None,
            list: ScrollList::new(0, 0, 0, 0, 36),
            buttons: Vec::new(),
            started: false,
            next_key: 0,
            pings: channel(),
            pending: None,
            edit: Rc::new(RefCell::new(blank(""))),
            held: false,
            servers_dat: PathBuf::new(),
        }
    }

    fn push_row(&mut self, entry: ServerEntry) {
        self.rows.push(Row {
            entry,
            ping: Ping::Pending,
            key: self.next_key,
            icon_shown: None,
            last_click: 0,
        });
        self.start_ping(self.rows.len() - 1);
        self.next_key += 1;
    }

    /// `ServerListEntryNormal.drawEntry` submits the ping on first draw; here it starts with the row.
    fn start_ping(&mut self, i: usize) {
        let (key, ip, tx) = (
            self.rows[i].key,
            self.rows[i].entry.ip.clone(),
            self.pings.0.clone(),
        );
        self.rows[i].ping = Ping::Pending;
        std::thread::Builder::new()
            .name("Server Pinger".into())
            .spawn(move || {
                let _ = tx.send((key, ping(&ip)));
            })
            .expect("spawn server pinger");
    }

    fn save(&self) {
        let list: Vec<ServerEntry> = self.rows.iter().map(|r| r.entry.clone()).collect();
        if let Err(e) = save_servers(&self.servers_dat, &list) {
            eprintln!("Couldn't save server list: {e}");
        }
    }

    /// `selectServer`: also sets which buttons are usable.
    fn select(&mut self, i: Option<usize>) {
        self.selected = i.filter(|&i| i < self.rows.len());
        let on = self.selected.is_some();
        for b in self
            .buttons
            .iter_mut()
            .filter(|b| matches!(b.id, SELECT | EDIT | DELETE))
        {
            b.enabled = on;
        }
    }

    /// `connectToSelected`.
    fn join(&mut self, entry: &ServerEntry) -> Nav {
        let (host, port) = split_address(&entry.ip);
        Nav::Child(Box::new(move |me| {
            Box::new(Connecting::new(me, host, port))
        }))
    }

    fn row_count(&self) -> usize {
        // The LAN "Scanning for games" row follows the saved servers.
        self.rows.len() + 1
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.rows.swap(a, b);
        self.save();
        if self.selected == Some(a) {
            self.select(Some(b));
        }
    }

    /// The child screen for `pending`, editing `self.edit`.
    fn child(&mut self, p: Pending, ctx: &Ctx) -> Nav {
        self.pending = Some(p);
        let edit = Rc::clone(&self.edit);
        match p {
            Pending::Direct => {
                Nav::Child(Box::new(move |me| Box::new(DirectConnect::new(me, edit))))
            }
            _ => {
                let title = ctx.tr("addServer.title");
                Nav::Child(Box::new(move |me| {
                    Box::new(AddServer::new(me, edit, title))
                }))
            }
        }
    }

    fn draw_row(
        &mut self,
        c: &mut dyn Canvas,
        i: usize,
        mx: i32,
        my: i32,
        ctx: &Ctx,
    ) -> Option<String> {
        let (x, y) = (self.list.row_x(), self.list.row_y(i));
        let hovered = self.list.row_at(mx, my, self.row_count()) == Some(i);
        let Some(row) = self.rows.get(i) else {
            draw_scanning(c, y, ctx);
            return None;
        };
        let status = row.ping.status();
        let protocol = status.map_or(PROTOCOL, |s| s.protocol);
        let incompatible = protocol != PROTOCOL;
        text(c, &row.entry.name, x + 32 + 3, y + 1, WHITE);
        let motd = row.ping.motd();
        for (j, l) in wrap(c, &motd, ROW_W - 32 - 2).iter().take(2).enumerate() {
            text(c, l, x + 32 + 3, y + 12 + 9 * j as i32, 0x808080);
        }
        let population = population(status, incompatible);
        let pw = c.width(&population);
        text(c, &population, x + ROW_W - pw - 15 - 2, y + 1, 0x808080);
        let (u, v, ping_tip, players) = ping_icon(&row.ping, status, protocol, ctx.now_ms, i);
        blit256(c, ICONS, x + ROW_W - 15, y, u * 10, 176 + v * 8, 10, 8);
        let tex = format!("servers/{}/icon", row.entry.ip);
        self.sync_icon(c, i, &tex);
        let row = &self.rows[i];
        let icon = if row.entry.icon.is_some() {
            tex.as_str()
        } else {
            UNKNOWN_SERVER
        };
        c.blit(
            icon,
            x as f32,
            y as f32,
            32.0,
            32.0,
            [0.0, 0.0, 1.0, 1.0],
            0xffffffff,
        );
        let (rx, ry) = (mx - x, my - y);
        let mut tip = None;
        if (ROW_W - 15..=ROW_W - 5).contains(&rx) && (0..=8).contains(&ry) {
            tip = Some(ping_tip);
        } else if (ROW_W - pw - 15 - 2..=ROW_W - 15 - 2).contains(&rx) && (0..=8).contains(&ry) {
            tip = players;
        }
        if ctx.settings.touchscreen || hovered {
            self.draw_row_buttons(c, x, y, i, rx, ry);
        }
        tip
    }

    fn sync_icon(&mut self, c: &mut dyn Canvas, i: usize, tex: &str) {
        let row = &mut self.rows[i];
        if row.icon_shown == row.entry.icon {
            return;
        }
        row.icon_shown = row.entry.icon.clone();
        let png = row
            .entry
            .icon
            .as_deref()
            .and_then(base64)
            .filter(|p| is_64px_png(p));
        match png {
            Some(p) => c.texture(tex, &p),
            None => row.entry.icon = None,
        }
        self.save();
    }

    fn draw_row_buttons(&self, c: &mut dyn Canvas, x: i32, y: i32, i: usize, rx: i32, ry: i32) {
        c.fill(x, y, x + 32, y + 32, 0xa0909090);
        blit_sel(c, x, y, 0, if rx < 32 && rx > 16 { 32 } else { 0 });
        if i > 0 {
            blit_sel(c, x, y, 96, if rx < 16 && ry < 16 { 32 } else { 0 });
        }
        if i + 1 < self.rows.len() {
            blit_sel(c, x, y, 64, if rx < 16 && ry > 16 { 32 } else { 0 });
        }
    }

    /// `ServerListEntryNormal.mousePressed`.
    fn row_pressed(
        &mut self,
        i: usize,
        rx: i32,
        ry: i32,
        shift: bool,
        ctx: &mut Ctx,
    ) -> Option<Nav> {
        if i >= self.rows.len() {
            return None;
        }
        if rx < 32 && rx > 16 {
            self.select(Some(i));
            let e = self.rows[i].entry.clone();
            return Some(self.join(&e));
        }
        if let Some(j) = self.move_target(i, rx, ry, shift) {
            self.swap(i, j);
            return None;
        }
        self.select(Some(i));
        let double = ctx.now_ms - self.rows[i].last_click < 250;
        self.rows[i].last_click = ctx.now_ms;
        if double {
            let e = self.rows[i].entry.clone();
            return Some(self.join(&e));
        }
        None
    }

    /// The row the up or down arrow at (`rx`, `ry`) moves row `i` to.
    fn move_target(&self, i: usize, rx: i32, ry: i32, shift: bool) -> Option<usize> {
        let last = self.rows.len() - 1;
        if rx >= 16 {
            None
        } else if ry < 16 && i > 0 {
            Some(if shift { 0 } else { i - 1 })
        } else if ry > 16 && i < last {
            Some(if shift { last } else { i + 1 })
        } else {
            None
        }
    }
}

fn blank(ip: &str) -> ServerEntry {
    ServerEntry {
        name: String::new(),
        ip: ip.to_owned(),
        icon: None,
        accept_textures: None,
    }
}

fn blit_sel(c: &mut dyn Canvas, x: i32, y: i32, u: i32, v: i32) {
    blit256(c, SELECTION, x, y, u, v, 32, 32);
}

/// `ServerData.playerList`: the sample names, then "... and N more ...".
fn player_list(s: &ServerStatus) -> String {
    let mut out = s.sample.join("\n");
    let shown = s.sample.len() as i32;
    if shown < s.online && shown > 0 {
        out += &format!("\n... and {} more ...", s.online - shown);
    }
    out
}

fn draw_scanning(c: &mut dyn Canvas, y: i32, ctx: &Ctx) {
    let s = ctx.tr("lanServer.scanning");
    let ty = y + 32 / 2 - 9 / 2;
    let sw = c.width(&s);
    text(c, &s, ctx.width / 2 - sw / 2, ty, WHITE);
    let dots = ["O o o", "o O o", "o o O", "o O o"][((ctx.now_ms / 300) % 4) as usize];
    let dw = c.width(dots);
    text(c, dots, ctx.width / 2 - dw / 2, ty + 9, 0x808080);
}

fn population(status: Option<&ServerStatus>, incompatible: bool) -> String {
    match status {
        Some(s) if incompatible => format!("§c{}", s.version_name),
        Some(s) if s.max >= 0 => format!("§7{}§8/§7{}", s.online, s.max),
        Some(_) => "§8???".to_owned(),
        None => String::new(),
    }
}

/// Ping icon column, row, tooltip and player list tooltip.
fn ping_icon(
    ping: &Ping,
    status: Option<&ServerStatus>,
    protocol: i32,
    now_ms: u64,
    i: usize,
) -> (i32, i32, String, Option<String>) {
    match (ping, status) {
        (_, Some(s)) if protocol != PROTOCOL => {
            let tip = if protocol > PROTOCOL {
                "Client out of date!"
            } else {
                "Server out of date!"
            };
            (0, 5, tip.to_owned(), Some(player_list(s)))
        }
        (Ping::Failed(_), _) => (0, 5, "(no connection)".to_owned(), None),
        (_, Some(s)) => {
            let bars = [150, 300, 600, 1000]
                .iter()
                .filter(|&&t| s.ping_ms >= t)
                .count() as i32;
            (0, bars, format!("{}ms", s.ping_ms), Some(player_list(s)))
        }
        _ => {
            let l = ((now_ms / 100 + i as u64 * 2) & 7) as i32;
            (
                1,
                if l > 4 { 8 - l } else { l },
                "Pinging...".to_owned(),
                None,
            )
        }
    }
}

/// The icon must be a 64x64 PNG; the IHDR width and height sit at bytes 16..24.
fn is_64px_png(p: &[u8]) -> bool {
    p.len() >= 24 && p[..8] == *b"\x89PNG\r\n\x1a\n" && p[16..24] == [0, 0, 0, 64, 0, 0, 0, 64]
}

impl Screen for Multiplayer {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        if !self.started {
            self.started = true;
            self.servers_dat = ctx.game_dir.join("servers.dat");
            for e in load_servers(&self.servers_dat) {
                self.push_row(e);
            }
        }
        let scroll = self.list.scroll;
        self.list = ScrollList::new(w, h, 32, h - 64, 36);
        self.list.row_w = ROW_W;
        self.list.bar_x = w / 2 + 124 + 30;
        self.list.scroll = scroll;
        let t = |k: &str| ctx.tr(k);
        self.buttons = vec![
            Button::new(EDIT, w / 2 - 154, h - 28, 70, 20, t("selectServer.edit")),
            Button::new(DELETE, w / 2 - 74, h - 28, 70, 20, t("selectServer.delete")),
            Button::new(
                SELECT,
                w / 2 - 154,
                h - 52,
                100,
                20,
                t("selectServer.select"),
            ),
            Button::new(
                DIRECT,
                w / 2 - 50,
                h - 52,
                100,
                20,
                t("selectServer.direct"),
            ),
            Button::new(ADD, w / 2 + 4 + 50, h - 52, 100, 20, t("selectServer.add")),
            Button::new(
                REFRESH,
                w / 2 + 4,
                h - 28,
                70,
                20,
                t("selectServer.refresh"),
            ),
            Button::new(CANCEL, w / 2 + 4 + 76, h - 28, 75, 20, t("gui.cancel")),
        ];
        self.select(self.selected);
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        while let Ok((key, res)) = self.pings.1.try_recv() {
            if let Some(r) = self.rows.iter_mut().find(|r| r.key == key) {
                r.ping = match res {
                    Ok(s) => {
                        // `ServerPinger`: the ping's icon replaces the stored one, or clears it.
                        r.entry.icon = s.favicon.clone();
                        Ping::Done(s)
                    }
                    Err(PingError::UnknownHost) => Ping::Failed("Can't resolve hostname"),
                    Err(PingError::Failed(_)) => Ping::Failed("Can't connect to server."),
                };
            }
        }
        None
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let n = self.row_count();
        if self.held {
            self.list.drag(my, n);
        }
        default_background(c, ctx);
        self.list.draw_back(c, n);
        let mut tip = None;
        for i in 0..n {
            if !self.list.row_visible(i) {
                continue;
            }
            if self.selected == Some(i) {
                self.list.draw_selection(c, i);
            }
            tip = self.draw_row(c, i, mx, my, ctx).or(tip);
        }
        self.list.draw_front(c, n);
        centered(c, &ctx.tr("multiplayer.title"), ctx.width / 2, 20, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
        if let Some(t) = tip.filter(|t| !t.is_empty()) {
            let lines: Vec<String> = t.split('\n').map(str::to_owned).collect();
            tooltip(c, &lines, mx, my, ctx.width, ctx.height);
        }
    }

    fn key(&mut self, code: i32, _ch: char, mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        if code == key::F5 {
            return self.click_button(REFRESH, ctx);
        }
        let Some(i) = self.selected else {
            return (code == key::ESCAPE).then_some(Nav::Game);
        };
        match code {
            key::UP if mods.shift => {
                if i > 0 {
                    self.swap(i, i - 1);
                    self.list.scroll_by(-self.list.slot_h, self.row_count());
                }
            }
            key::UP => {
                self.select(i.checked_sub(1));
                self.list.scroll_by(-self.list.slot_h, self.row_count());
            }
            key::DOWN if mods.shift => {
                if i + 1 < self.rows.len() {
                    self.swap(i, i + 1);
                    self.list.scroll_by(self.list.slot_h, self.row_count());
                }
            }
            key::DOWN => {
                // Stepping onto the LAN scan row clears the selection, as vanilla does.
                self.select(Some(i + 1));
                self.list.scroll_by(self.list.slot_h, self.row_count());
            }
            key::RETURN | key::NUMPAD_ENTER => return self.click_button(SELECT, ctx),
            key::ESCAPE => return Some(Nav::Game),
            _ => {}
        }
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        if let Some(id) = press(&mut self.buttons, x, y, mb, ctx) {
            return self.click_button(id, ctx);
        }
        let n = self.row_count();
        if mb == 0 {
            self.held = true;
            self.list.press(x, y, n);
        }
        if y < self.list.top || y > self.list.bottom {
            return None;
        }
        let i = self.list.row_at(x, y, n)?;
        let (rx, ry) = (x - self.list.row_x(), y - self.list.row_y(i));
        let shift = ctx.shift;
        self.row_pressed(i, rx, ry, shift, ctx)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        self.held = false;
        self.list.release();
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        let n = self.row_count();
        self.list.wheel(dir, n);
    }

    fn pauses(&self) -> bool {
        false
    }

    fn confirm(&mut self, yes: bool, id: i32, _ctx: &mut Ctx) -> Option<Nav> {
        let edited = self.edit.borrow().clone();
        match self.pending.take() {
            Some(Pending::Delete) if yes => {
                if let Ok(i) = usize::try_from(id) {
                    if i < self.rows.len() {
                        self.rows.remove(i);
                        self.save();
                    }
                }
                self.select(None);
            }
            Some(Pending::Direct) if yes => {
                return Some(self.join(&edited));
            }
            Some(Pending::Add) if yes => {
                self.push_row(edited);
                self.save();
                self.select(None);
            }
            Some(Pending::Edit) if yes => {
                if let Some(r) = self.selected.and_then(|i| self.rows.get_mut(i)) {
                    r.entry = edited;
                    self.save();
                }
            }
            _ => {}
        }
        None
    }
}

impl Multiplayer {
    /// `GuiMultiplayer.actionPerformed`.
    fn click_button(&mut self, id: i32, ctx: &mut Ctx) -> Option<Nav> {
        if !self.buttons.iter().any(|b| b.id == id && b.enabled) {
            return None;
        }
        let sel = self.selected.map(|i| self.rows[i].entry.clone());
        match id {
            DELETE => {
                let e = sel?;
                self.pending = Some(Pending::Delete);
                let q = ctx.tr("selectServer.deleteQuestion");
                let m = format!("'{}' {}", e.name, ctx.tr("selectServer.deleteWarning"));
                let (y, n) = (ctx.tr("selectServer.deleteButton"), ctx.tr("gui.cancel"));
                let i = self.selected? as i32;
                Some(Nav::Child(Box::new(move |me| {
                    Box::new(YesNo::new(me, q, m, y, n, i))
                })))
            }
            SELECT => sel.map(|e| self.join(&e)),
            DIRECT => {
                *self.edit.borrow_mut() = ServerEntry {
                    name: ctx.tr("selectServer.defaultName"),
                    ..blank("")
                };
                Some(self.child(Pending::Direct, ctx))
            }
            ADD => {
                *self.edit.borrow_mut() = ServerEntry {
                    name: ctx.tr("selectServer.defaultName"),
                    ..blank("")
                };
                Some(self.child(Pending::Add, ctx))
            }
            EDIT => {
                *self.edit.borrow_mut() = sel?;
                Some(self.child(Pending::Edit, ctx))
            }
            CANCEL => self.parent.take().map(Nav::Open),
            REFRESH => self
                .parent
                .take()
                .map(|p| Nav::Open(Box::new(Multiplayer::new(p)))),
            _ => None,
        }
    }
}

/// `GuiScreenAddServer`.
struct AddServer {
    parent: Option<Box<dyn Screen>>,
    edit: Rc<RefCell<ServerEntry>>,
    title: String,
    name: TextField,
    ip: TextField,
    buttons: Vec<Button>,
}

impl AddServer {
    fn new(parent: Box<dyn Screen>, edit: Rc<RefCell<ServerEntry>>, title: String) -> AddServer {
        AddServer {
            parent: Some(parent),
            edit,
            title,
            name: TextField::new(0, 0, 200, 20),
            ip: TextField::new(0, 0, 200, 20),
            buttons: Vec::new(),
        }
    }

    fn pack_label(&self, ctx: &Ctx) -> String {
        let mode = match self.edit.borrow().accept_textures {
            Some(true) => "enabled",
            Some(false) => "disabled",
            None => "prompt",
        };
        format!(
            "{}: {}",
            ctx.tr("addServer.resourcePack"),
            ctx.tr(&format!("addServer.resourcePack.{mode}"))
        )
    }

    fn update_add(&mut self) {
        self.buttons[0].enabled = !self.ip.text.is_empty() && !self.name.text.is_empty();
    }

    fn answer(&mut self, yes: bool, ctx: &mut Ctx) -> Option<Nav> {
        if yes {
            let mut e = self.edit.borrow_mut();
            e.name = self.name.text.clone();
            e.ip = self.ip.text.clone();
        }
        Some(answer(self.parent.take()?, yes, 0, ctx))
    }
}

impl Screen for AddServer {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![
            Button::wide(0, w / 2 - 100, h / 4 + 96 + 18, ctx.tr("addServer.add")),
            Button::wide(1, w / 2 - 100, h / 4 + 120 + 18, ctx.tr("gui.cancel")),
            Button::wide(2, w / 2 - 100, h / 4 + 72, self.pack_label(ctx)),
        ];
        let (name, ip) = {
            let e = self.edit.borrow();
            (
                if self.name.text.is_empty() {
                    e.name.clone()
                } else {
                    self.name.text.clone()
                },
                e.ip.clone(),
            )
        };
        let ip = if self.ip.text.is_empty() {
            ip
        } else {
            self.ip.text.clone()
        };
        let focus_name = !self.ip.focused;
        self.name = TextField::new(w / 2 - 100, 66, 200, 20);
        self.name.set_focused(focus_name);
        self.name.set(&name, ctx.metrics);
        self.ip = TextField::new(w / 2 - 100, 106, 200, 20);
        self.ip.max_len = 128;
        self.ip.set_focused(!focus_name);
        self.ip.set(&ip, ctx.metrics);
        self.update_add();
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        self.name.tick();
        self.ip.tick();
        None
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        centered(c, &self.title, ctx.width / 2, 17, WHITE);
        text(
            c,
            &ctx.tr("addServer.enterName"),
            ctx.width / 2 - 100,
            53,
            GRAY_LABEL,
        );
        text(
            c,
            &ctx.tr("addServer.enterIp"),
            ctx.width / 2 - 100,
            94,
            GRAY_LABEL,
        );
        self.name.draw(c);
        self.ip.draw(c);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn key(&mut self, code: i32, ch: char, mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        self.name.key(code, ch, mods, ctx.metrics, ctx.clipboard);
        self.ip.key(code, ch, mods, ctx.metrics, ctx.clipboard);
        if code == key::TAB {
            let f = self.name.focused;
            self.name.set_focused(!f);
            self.ip.set_focused(f);
        }
        self.update_add();
        if (code == key::RETURN || code == key::NUMPAD_ENTER) && self.buttons[0].enabled {
            return self.answer(true, ctx);
        }
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        let pressed = press(&mut self.buttons, x, y, mb, ctx);
        self.ip.click(x, y, mb, ctx.metrics);
        self.name.click(x, y, mb, ctx.metrics);
        match pressed? {
            2 => {
                let next = match self.edit.borrow().accept_textures {
                    Some(true) => Some(false),
                    Some(false) => None,
                    None => Some(true),
                };
                self.edit.borrow_mut().accept_textures = next;
                self.buttons[2].label = self.pack_label(ctx);
                None
            }
            1 => self.answer(false, ctx),
            _ => self.answer(true, ctx),
        }
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }
}

/// `GuiScreenServerList`, the Direct Connect screen.
struct DirectConnect {
    parent: Option<Box<dyn Screen>>,
    edit: Rc<RefCell<ServerEntry>>,
    ip: TextField,
    buttons: Vec<Button>,
}

impl DirectConnect {
    fn new(parent: Box<dyn Screen>, edit: Rc<RefCell<ServerEntry>>) -> DirectConnect {
        DirectConnect {
            parent: Some(parent),
            edit,
            ip: TextField::new(0, 0, 200, 20),
            buttons: Vec::new(),
        }
    }

    fn answer(&mut self, yes: bool, ctx: &mut Ctx) -> Option<Nav> {
        if yes {
            self.edit.borrow_mut().ip = self.ip.text.clone();
        }
        Some(answer(self.parent.take()?, yes, 0, ctx))
    }
}

impl Screen for DirectConnect {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![
            Button::wide(
                0,
                w / 2 - 100,
                h / 4 + 96 + 12,
                ctx.tr("selectServer.select"),
            ),
            Button::wide(1, w / 2 - 100, h / 4 + 120 + 12, ctx.tr("gui.cancel")),
        ];
        let text = if self.ip.text.is_empty() {
            ctx.settings.last_server.clone()
        } else {
            self.ip.text.clone()
        };
        self.ip = TextField::new(w / 2 - 100, 116, 200, 20);
        self.ip.max_len = 128;
        self.ip.set_focused(true);
        self.ip.set(&text, ctx.metrics);
        self.buttons[0].enabled = !self.ip.text.is_empty();
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        self.ip.tick();
        None
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        centered(c, &ctx.tr("selectServer.direct"), ctx.width / 2, 20, WHITE);
        text(
            c,
            &ctx.tr("addServer.enterIp"),
            ctx.width / 2 - 100,
            100,
            GRAY_LABEL,
        );
        self.ip.draw(c);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn key(&mut self, code: i32, ch: char, mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        if self.ip.key(code, ch, mods, ctx.metrics, ctx.clipboard) {
            self.buttons[0].enabled = !self.ip.text.is_empty();
        } else if (code == key::RETURN || code == key::NUMPAD_ENTER) && self.buttons[0].enabled {
            return self.answer(true, ctx);
        }
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        let pressed = press(&mut self.buttons, x, y, mb, ctx);
        self.ip.click(x, y, mb, ctx.metrics);
        self.answer(pressed? == 0, ctx)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }

    /// `onGuiClosed`: remember the address in `lastServer`.
    fn closed(&mut self, ctx: &mut Ctx) {
        ctx.settings.last_server = self.ip.text.clone();
        ctx.save_settings();
    }
}
