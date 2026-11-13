//! Vanilla 1.8.9 menus (`GuiScreen` and its subclasses), ported from the decompiled client.
//!
//! Screens work in GUI units (ScaledResolution space) and draw through [`Canvas`]. The engine owns
//! one [`Screens`], feeds it input as [`Event`]s with LWJGL key codes, and acts on the
//! [`Effect`]s the screens push into [`Ctx::effects`].

mod book;
mod connecting;
mod death;
mod disconnect;
mod language;
mod link;
mod multiplayer;
mod options;
mod pack_prompt;
mod packs;
mod pause;
mod sign;
mod snooper;
mod title;

pub use book::Book;
pub use connecting::{Connecting, DownloadingTerrain};
pub use death::Death;
pub use disconnect::Disconnected;
pub use language::{languages, Language};
pub use link::ConfirmLink;
pub use multiplayer::Multiplayer;
pub use options::{key_name, KeyBind, Opt, Options, Settings, SoundCategory, MODEL_PARTS};
pub use pack_prompt::PackPrompt;
pub use packs::{default_description, selected_paths};
pub use pause::Pause;
pub use sign::EditSign;
pub use title::Title;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::toolkit::GuiScale;

/// Font measurement, `FontRenderer.getStringWidth`: `§` codes have no width.
pub trait Metrics {
    fn width(&self, s: &str) -> i32;
}

/// Draw calls in GUI units; `tex` is a path under `assets/minecraft/`, such as `textures/gui/widgets.png`.
/// UVs outside `0..1` repeat the texture (the tiled `options_background.png` needs it).
pub trait Canvas: Metrics {
    /// `Gui.drawRect` with corner order as given.
    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32);
    /// `Gui.drawGradientRect`: `top` color at `y0`, `bottom` at `y1`.
    fn gradient(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, top: u32, bottom: u32);
    /// Textured rectangle; `uv` is `[u0, v0, u1, v1]`, `argb` tints and fades it.
    #[allow(clippy::too_many_arguments)] // mirrors `Gui.drawModalRectWithCustomSizedTexture`
    fn blit(&mut self, tex: &str, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], argb: u32);
    /// Any textured quad (isometric block icons); corners in draw order.
    fn quad(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: [u32; 4]);
    /// Additive-blend quad (`GL_SRC_COLOR, GL_ONE`) for the enchant glint; UVs past `0..1` repeat.
    fn quad_add(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: u32);
    /// `FontRenderer.drawString`; `argb` alpha 0 means opaque, like vanilla.
    fn text(&mut self, s: &str, x: f32, y: f32, argb: u32, shadow: bool);
    /// `FontRenderer.setBidiFlag`.
    fn set_bidi(&mut self, on: bool);
    /// Register a texture from PNG bytes under `name` (server icons: `servers/<ip>/icon`).
    fn texture(&mut self, name: &str, png: &[u8]);
    /// `GlStateManager.pushMatrix` + translate, uniform scale, then rotate about Z (degrees).
    fn push(&mut self, translate: [f32; 2], scale: f32, rotate_deg: f32);
    fn pop(&mut self);
}

/// LWJGL `Keyboard` codes, the values `options.txt` stores.
pub mod key {
    pub const ESCAPE: i32 = 1;
    pub const BACK: i32 = 14;
    pub const TAB: i32 = 15;
    pub const RETURN: i32 = 28;
    pub const A: i32 = 30;
    pub const C: i32 = 46;
    pub const V: i32 = 47;
    pub const X: i32 = 45;
    pub const F5: i32 = 63;
    pub const NUMPAD_ENTER: i32 = 156;
    pub const HOME: i32 = 199;
    pub const UP: i32 = 200;
    pub const LEFT: i32 = 203;
    pub const RIGHT: i32 = 205;
    pub const END: i32 = 207;
    pub const DOWN: i32 = 208;
    pub const DELETE: i32 = 211;
}

/// Input in GUI units. `ctrl` is Control, or Command on macOS (`GuiScreen.isCtrlKeyDown`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Click {
        x: i32,
        y: i32,
        button: i32,
    },
    Release {
        x: i32,
        y: i32,
        button: i32,
    },
    /// `ch` is `'\0'` when the key types nothing.
    Key {
        code: i32,
        ch: char,
        ctrl: bool,
        shift: bool,
    },
    /// Wheel direction: positive is up, like `Mouse.getEventDWheel`.
    Scroll(i32),
}

/// What the engine must do for the screens.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A sound event name, such as `gui.button.press`.
    Sound(&'static str),
    /// Start a login to `host:port` (`GuiConnecting`); report back with [`Screens::connect_failed`] or [`Screens::joined`].
    Connect { host: String, port: u16 },
    /// The Cancel button on the connecting screen.
    CancelConnect,
    /// Leave the server (`theWorld.sendQuittingDisconnectingPacket`).
    Disconnect,
    /// `C16PacketClientStatus(PERFORM_RESPAWN)`.
    Respawn,
    /// `Minecraft.shutdown`.
    Quit,
    /// `options.txt` was written: apply changed settings and send `C15PacketClientSettings` when in a world.
    SettingsSaved,
    /// `Settings::lang` changed: reload the language files (`Minecraft.refreshResources`), then save.
    LanguageChanged,
    /// "Super Secret Settings...": a random animal, block, mob, player or weather sound instead of
    /// the click, then `EntityRenderer.activateNextShader`.
    SuperSecret,
    /// The selected resource packs changed: reload the resources (`Minecraft.refreshResources`).
    ResourcesChanged,
    /// Open a web page or a folder in the system browser.
    OpenUrl(String),
    /// `GuiEditSign.onGuiClosed`: send `C12UpdateSign` for the block at `pos`.
    SignText { pos: [i32; 3], lines: [String; 4] },
    /// The answer of the server resource pack prompt: remember it for the server, then accept or decline.
    ServerPackAnswer {
        url: String,
        hash: String,
        accepted: bool,
    },
}

/// Localized text, `I18n`/`LanguageMap`/`LanguageManager`, and the title splashes.
#[derive(Default)]
pub struct Texts {
    lang: HashMap<String, String>,
    pub splashes: Vec<String>,
    /// `Locale.isUnicode`: over 10% of the translated text is outside Latin-1.
    pub unicode: bool,
    /// The languages of `pack.mcmeta`, sorted by code.
    pub languages: Vec<Language>,
    /// The built-in pack's `pack.png` and `pack.mcmeta` description, for the resource pack list.
    pub default_pack: (Option<Vec<u8>>, String),
}

impl Texts {
    /// `LanguageManager.isCurrentLanguageBidirectional` for the language `code`.
    pub fn bidirectional(&self, code: &str) -> bool {
        self.languages
            .iter()
            .any(|l| l.code == code && l.bidirectional)
    }

    /// `lang` is `.lang` text (`key=value` lines; a later line wins), `splashes` is `texts/splashes.txt`.
    pub fn parse(lang: &str, splashes: &str) -> Texts {
        let lang: HashMap<String, String> = lang
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let splashes = splashes
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        // `Locale.checkUnicodeUsage` counts UTF-16 units, as Java strings hold them.
        let (wide, all) = lang
            .values()
            .flat_map(|v| v.encode_utf16())
            .fold((0, 0), |(w, a), u| (w + usize::from(u >= 0x100), a + 1));
        Texts {
            lang,
            splashes,
            unicode: wide as f32 / all as f32 > 0.1,
            ..Texts::default()
        }
    }

    /// `I18n.format(key)`; an unknown key is returned as is.
    pub fn tr(&self, key: &str) -> String {
        self.fmt(key, &[])
    }

    /// `I18n.format(key, args)` with the `%s`, `%d`, `%n$s` and `%%` forms the lang files use.
    pub fn fmt(&self, key: &str, args: &[&str]) -> String {
        let pattern = self.lang.get(key).map_or(key, String::as_str);
        let mut out = String::new();
        let mut next = 0;
        let mut rest = pattern;
        while let Some(i) = rest.find('%') {
            out.push_str(&rest[..i]);
            let spec = &rest[i + 1..];
            let digits = spec.bytes().take_while(u8::is_ascii_digit).count();
            let (index, tail) = match spec[digits..].strip_prefix('$') {
                Some(t) if digits > 0 => (spec[..digits].parse::<usize>().ok().map(|n| n - 1), t),
                _ => (None, spec),
            };
            match tail.chars().next() {
                Some('%') => out.push('%'),
                Some('s' | 'd') => {
                    let i = index.unwrap_or_else(|| {
                        next += 1;
                        next - 1
                    });
                    out.push_str(args.get(i).copied().unwrap_or(""));
                }
                _ => {
                    out.push('%');
                    rest = spec;
                    continue;
                }
            }
            rest = &tail[1..];
        }
        out.push_str(rest);
        out
    }
}

/// Everything a screen may read or change while it handles one call.
pub struct Ctx<'a> {
    /// Scaled screen size in GUI units.
    pub width: i32,
    pub height: i32,
    pub texts: &'a Texts,
    pub settings: &'a mut Settings,
    /// `mc.theWorld != null`: in-game screens get the translucent background.
    pub in_world: bool,
    /// The world's difficulty (0 peaceful .. 3 hard) and hardcore flag, when in a world.
    pub difficulty: Option<u8>,
    pub hardcore: bool,
    /// `Minecraft.getSystemTime()` in milliseconds.
    pub now_ms: u64,
    /// Folder that holds `servers.dat`.
    pub game_dir: PathBuf,
    pub effects: Vec<Effect>,
    /// The clipboard text for Ctrl+C and Ctrl+V in text fields and Copy link; the engine mirrors it to the system clipboard.
    pub clipboard: &'a mut String,
    /// Font widths, for text fields handling keys outside a draw.
    pub metrics: &'a dyn Metrics,
    /// `GuiScreen.isShiftKeyDown`, for clicks.
    pub shift: bool,
}

impl Ctx<'_> {
    pub fn tr(&self, key: &str) -> String {
        self.texts.tr(key)
    }

    /// `Minecraft.isUnicode`: the font draws `unicode_page_*.png` glyphs for this language or setting.
    pub fn unicode(&self) -> bool {
        self.settings.force_unicode || self.texts.unicode
    }

    /// `GameSettings.saveOptions`.
    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            eprintln!("Failed to save options: {e}");
        }
        self.effects.push(Effect::SettingsSaved);
    }
}

/// The next screen after an input, `Minecraft.displayGuiScreen`.
pub enum Nav {
    /// `displayGuiScreen(null)`: back to the game, or the title screen when there is no world.
    Game,
    Open(Box<dyn Screen>),
    /// Open a screen built from the current one, which becomes its parent (`new GuiX(this)`).
    Child(ChildFn),
}

/// Builds a child screen from its parent, for [`Nav::Child`].
pub type ChildFn = Box<dyn FnOnce(Box<dyn Screen>) -> Box<dyn Screen>>;

/// One `GuiScreen`. Default methods are the `GuiScreen` base behavior.
pub trait Screen {
    /// `initGui`; called again after every resize.
    fn init(&mut self, ctx: &mut Ctx);
    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx);
    /// `updateScreen`, 20 times a second.
    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }
    /// `keyTyped`: Escape closes the screen.
    fn key(&mut self, code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        (code == key::ESCAPE).then_some(Nav::Game)
    }
    /// `mouseClicked`.
    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav>;
    /// `mouseReleased`.
    fn release(&mut self, _x: i32, _y: i32, _button: i32, _ctx: &mut Ctx) {}
    fn scroll(&mut self, _dir: i32, _ctx: &mut Ctx) {}
    /// `onGuiClosed`.
    fn closed(&mut self, _ctx: &mut Ctx) {}
    /// `doesGuiPauseGame`.
    fn pauses(&self) -> bool {
        true
    }
    /// `GuiYesNoCallback.confirmClicked`: the answer of a [`YesNo`] this screen opened. `None` reopens this screen.
    fn confirm(&mut self, _yes: bool, _id: i32, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }
    /// Downcast hook for [`Screens`] events that concern one screen type.
    fn as_connecting(&mut self) -> Option<&mut Connecting> {
        None
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

/// The open screen, as `Minecraft.currentScreen`.
#[derive(Default)]
pub struct Screens {
    current: Option<Box<dyn Screen>>,
    px: (u32, u32),
    /// `(guiScale, unicode)` of the last resize; a change rescales the screen.
    scale: (u8, bool),
}

impl Screens {
    pub fn is_open(&self) -> bool {
        self.current.is_some()
    }

    /// The game pauses only in singleplayer; the engine uses this for cursor release and tick logic.
    pub fn pauses(&self) -> bool {
        self.current.as_ref().is_some_and(|s| s.pauses())
    }

    /// `displayGuiScreen(screen)`.
    pub fn open(&mut self, screen: Box<dyn Screen>, ctx: &mut Ctx) {
        self.nav(Nav::Open(screen), ctx);
    }

    /// `displayGuiScreen(null)`.
    pub fn close(&mut self, ctx: &mut Ctx) {
        self.nav(Nav::Game, ctx);
    }

    fn nav(&mut self, nav: Nav, ctx: &mut Ctx) {
        if let Some(old) = self.current.as_mut() {
            old.closed(ctx);
        }
        self.current = match nav {
            Nav::Open(s) => Some(s),
            Nav::Child(f) => self.current.take().map(f),
            Nav::Game if ctx.in_world => None,
            Nav::Game => Some(Box::new(Title::new(ctx))),
        };
        if let Some(s) = self.current.as_mut() {
            s.init(ctx);
        }
    }

    /// Framebuffer size changed (or call once at start). Returns the GUI scale factor.
    pub fn resize(&mut self, px_w: u32, px_h: u32, ctx: &mut Ctx) -> i32 {
        self.px = (px_w, px_h);
        self.scale = (ctx.settings.gui_scale, ctx.unicode());
        let s = GuiScale::new(px_w, px_h, u32::from(ctx.settings.gui_scale), ctx.unicode());
        (ctx.width, ctx.height) = (s.width, s.height);
        if let Some(s) = self.current.as_mut() {
            s.init(ctx);
        }
        s.factor
    }

    pub fn event(&mut self, ev: Event, ctx: &mut Ctx) {
        let Some(s) = self.current.as_mut() else {
            return;
        };
        let nav = match ev {
            Event::Click { x, y, button } => s.click(x, y, button, ctx),
            Event::Release { x, y, button } => {
                s.release(x, y, button, ctx);
                None
            }
            Event::Key {
                code,
                ch,
                ctrl,
                shift,
            } => s.key(code, ch, Mods { ctrl, shift }, ctx),
            Event::Scroll(d) => {
                s.scroll(d, ctx);
                None
            }
        };
        if let Some(nav) = nav {
            self.nav(nav, ctx);
        }
        if (ctx.settings.gui_scale, ctx.unicode()) != self.scale {
            self.resize(self.px.0, self.px.1, ctx);
        }
    }

    pub fn tick(&mut self, ctx: &mut Ctx) {
        if let Some(nav) = self.current.as_mut().and_then(|s| s.tick(ctx)) {
            self.nav(nav, ctx);
        }
    }

    pub fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if let Some(s) = self.current.as_mut() {
            s.draw(c, mx, my, ctx);
        }
    }

    /// The login failed: `GuiDisconnected(parent, "connect.failed", reason)`.
    pub fn connect_failed(&mut self, reason: &str, ctx: &mut Ctx) {
        let parent = self
            .current
            .as_mut()
            .and_then(|s| s.as_connecting())
            .and_then(|c| c.parent.take());
        let parent =
            parent.unwrap_or_else(|| Box::new(Multiplayer::new(Box::new(Title::new(ctx)))));
        let msg = ctx.texts.fmt("disconnect.genericReason", &[reason]);
        self.open(
            Box::new(Disconnected::new(parent, "connect.failed", msg)),
            ctx,
        );
    }

    /// `S01PacketJoinGame` arrived: `GuiDownloadingTerrain` until the first position packet.
    pub fn joined(&mut self, ctx: &mut Ctx) {
        self.open(Box::new(DownloadingTerrain), ctx);
    }

    /// The connection closed in game: `GuiDisconnected(GuiMultiplayer(GuiMainMenu), "disconnect.lost", reason)`.
    pub fn disconnected(&mut self, reason: &str, ctx: &mut Ctx) {
        let parent = Box::new(Multiplayer::new(Box::new(Title::new(ctx))));
        self.open(
            Box::new(Disconnected::new(
                parent,
                "disconnect.lost",
                reason.to_owned(),
            )),
            ctx,
        );
    }
}

pub const WIDGETS: &str = "textures/gui/widgets.png";
pub const OPTIONS_BACKGROUND: &str = "textures/gui/options_background.png";
pub const ICONS: &str = "textures/gui/icons.png";
pub const WHITE: u32 = 0xffffff;
pub const GRAY_LABEL: u32 = 0xa0a0a0;

/// `Gui.drawTexturedModalRect`: a `w`x`h` region at `(u, v)` of a 256x256 texture.
#[allow(clippy::too_many_arguments)] // mirrors the vanilla signature
pub fn blit256(c: &mut dyn Canvas, tex: &str, x: i32, y: i32, u: i32, v: i32, w: i32, h: i32) {
    let s = 1.0 / 256.0;
    let uv = [
        u as f32 * s,
        v as f32 * s,
        (u + w) as f32 * s,
        (v + h) as f32 * s,
    ];
    c.blit(tex, x as f32, y as f32, w as f32, h as f32, uv, 0xffffffff);
}

/// `drawString` with shadow.
pub fn text(c: &mut dyn Canvas, s: &str, x: i32, y: i32, rgb: u32) {
    c.text(s, x as f32, y as f32, rgb, true);
}

/// `drawCenteredString`.
pub fn centered(c: &mut dyn Canvas, s: &str, cx: i32, y: i32, rgb: u32) {
    let w = c.width(s);
    c.text(s, (cx - w / 2) as f32, y as f32, rgb, true);
}

/// `GuiScreen.drawDefaultBackground`: dark gradient in game, dirt tiles otherwise.
pub fn default_background(c: &mut dyn Canvas, ctx: &Ctx) {
    if ctx.in_world {
        c.gradient(0, 0, ctx.width, ctx.height, 0xc0101010, 0xd0101010);
    } else {
        dirt(c, 0, 0, ctx.width, ctx.height, 0, 0xff404040);
    }
}

/// `options_background.png` tiled at 32 GUI units, scrolled by `scroll` (GuiSlot.overlayBackground and drawBackground).
pub fn dirt(c: &mut dyn Canvas, x0: i32, y0: i32, x1: i32, y1: i32, scroll: i32, argb: u32) {
    let uv = [
        x0 as f32 / 32.0,
        (y0 + scroll) as f32 / 32.0,
        x1 as f32 / 32.0,
        (y1 + scroll) as f32 / 32.0,
    ];
    let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
    c.blit(OPTIONS_BACKGROUND, x0 as f32, y0 as f32, w, h, uv, argb);
}

/// `GuiScreen.drawHoveringText`.
pub fn tooltip(c: &mut dyn Canvas, lines: &[String], x: i32, y: i32, sw: i32, sh: i32) {
    if lines.is_empty() {
        return;
    }
    let w = lines.iter().map(|l| c.width(l)).max().unwrap_or(0);
    let (mut x, mut y) = (x + 12, y - 12);
    let mut h = 8;
    if lines.len() > 1 {
        h += 2 + (lines.len() as i32 - 1) * 10;
    }
    if x + w > sw {
        x -= 28 + w;
    }
    if y + h + 6 > sh {
        y = sh - h - 6;
    }
    let bg = 0xf0100010;
    c.gradient(x - 3, y - 4, x + w + 3, y - 3, bg, bg);
    c.gradient(x - 3, y + h + 3, x + w + 3, y + h + 4, bg, bg);
    c.gradient(x - 3, y - 3, x + w + 3, y + h + 3, bg, bg);
    c.gradient(x - 4, y - 3, x - 3, y + h + 3, bg, bg);
    c.gradient(x + w + 3, y - 3, x + w + 4, y + h + 3, bg, bg);
    let top = 0x505000ffu32;
    let bottom = (top & 0xfefefe) >> 1 | top & 0xff000000;
    c.gradient(x - 3, y - 2, x - 2, y + h + 2, top, bottom);
    c.gradient(x + w + 2, y - 2, x + w + 3, y + h + 2, top, bottom);
    c.gradient(x - 3, y - 3, x + w + 3, y - 2, top, top);
    c.gradient(x - 3, y + h + 2, x + w + 3, y + h + 3, bottom, bottom);
    for (i, l) in lines.iter().enumerate() {
        c.text(l, x as f32, y as f32, 0xffffffff, true);
        y += if i == 0 { 12 } else { 10 };
    }
}

/// The last `§` color/format codes in `s`, `FontRenderer.getFormatFromString`.
fn format_of(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '§' {
            continue;
        }
        match chars.next().map(|c| c.to_ascii_lowercase()) {
            Some(f @ ('0'..='9' | 'a'..='f')) => out = format!("§{f}"),
            Some(f @ ('k'..='o' | 'r')) => out.push_str(&format!("§{f}")),
            _ => {}
        }
    }
    out
}

/// `FontRenderer.listFormattedStringToWidth`: word wrap that carries `§` formats to the next line.
pub fn wrap(m: &dyn Metrics, s: &str, max: i32) -> Vec<String> {
    let mut out = Vec::new();
    // A real newline is a cut too, so the formats carry across it.
    let mut carry = String::new();
    for line in s.split('\n') {
        let mut rest = format!("{carry}{line}");
        loop {
            let cut = wrap_cut(m, &rest, max);
            if cut >= rest.len() {
                out.push(rest);
                break;
            }
            let head = rest[..cut].to_owned();
            let skip = rest[cut..].starts_with([' ', '\n']) as usize;
            let tail = format!("{}{}", format_of(&head), &rest[cut + skip..]);
            out.push(head);
            rest = tail;
        }
        carry = out
            .last()
            .map_or_else(String::new, |l: &String| format_of(l));
    }
    out
}

/// `FontRenderer.sizeStringToWidth`: byte length that fits, preferring the last space.
fn wrap_cut(m: &dyn Metrics, s: &str, max: i32) -> usize {
    let mut space = None;
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c == ' ' {
            space = Some(i);
        }
        if m.width(&s[..i + c.len_utf8()]) > max {
            end = i;
            break;
        }
    }
    match space {
        Some(sp) if end != s.len() && sp > 0 => sp,
        _ if end == 0 && !s.is_empty() => s.chars().next().map_or(0, char::len_utf8),
        _ => end,
    }
}

/// `FontRenderer.trimStringToWidth`; `reverse` keeps the end of `s`.
pub fn trim(m: &dyn Metrics, s: &str, max: i32, reverse: bool) -> String {
    if reverse {
        let mut start = s.len();
        for (i, _) in s.char_indices().rev() {
            if m.width(&s[i..]) > max {
                break;
            }
            start = i;
        }
        s[start..].to_owned()
    } else {
        let mut end = 0;
        for (i, c) in s.char_indices() {
            if m.width(&s[..i + c.len_utf8()]) > max {
                break;
            }
            end = i + c.len_utf8();
        }
        s[..end].to_owned()
    }
}

/// Slider state of a [`Button`] (`GuiOptionSlider`, `GuiScreenOptionsSounds.Button`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slider {
    /// Knob position, `0..=1`.
    pub pos: f32,
    pub dragging: bool,
}

/// `GuiButton`, and with `slider` set a slider.
#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub hovered: bool,
    pub slider: Option<Slider>,
    /// `GuiButtonLanguage`: the globe icon button.
    pub globe: bool,
}

impl Button {
    pub fn new(id: i32, x: i32, y: i32, w: i32, h: i32, label: impl Into<String>) -> Button {
        Button {
            id,
            x,
            y,
            w,
            h,
            label: label.into(),
            enabled: true,
            visible: true,
            hovered: false,
            slider: None,
            globe: false,
        }
    }

    /// The 200x20 default size.
    pub fn wide(id: i32, x: i32, y: i32, label: impl Into<String>) -> Button {
        Button::new(id, x, y, 200, 20, label)
    }

    pub fn slider(id: i32, x: i32, y: i32, w: i32, pos: f32, label: impl Into<String>) -> Button {
        Button {
            slider: Some(Slider {
                pos,
                dragging: false,
            }),
            ..Button::new(id, x, y, w, 20, label)
        }
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    /// `GuiButton.mousePressed`; a slider also takes the knob.
    pub fn pressed(&mut self, x: i32, y: i32) -> bool {
        let hit = self.enabled && self.visible && self.inside(x, y);
        if let (true, Some(s)) = (hit, self.slider.as_mut()) {
            s.pos = ((x - (self.x + 4)) as f32 / (self.w - 8) as f32).clamp(0.0, 1.0);
            s.dragging = true;
        }
        hit
    }

    /// While dragging, the knob position under the mouse (`GuiOptionSlider.mouseDragged`).
    pub fn drag(&mut self, mx: i32) -> Option<f32> {
        let s = self
            .slider
            .as_mut()
            .filter(|s| s.dragging && self.visible)?;
        s.pos = ((mx - (self.x + 4)) as f32 / (self.w - 8) as f32).clamp(0.0, 1.0);
        Some(s.pos)
    }

    /// `GuiButton.mouseReleased`.
    pub fn released(&mut self) {
        if let Some(s) = self.slider.as_mut() {
            s.dragging = false;
        }
    }

    /// `GuiButton.drawButton`.
    pub fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32) {
        if !self.visible {
            return;
        }
        self.hovered = self.inside(mx, my);
        if self.globe {
            let v = if self.hovered { 106 + self.h } else { 106 };
            blit256(c, WIDGETS, self.x, self.y, 0, v, self.w, self.h);
            return;
        }
        let state = match (self.slider.is_some(), self.enabled, self.hovered) {
            (true, ..) | (_, false, _) => 0,
            (_, true, false) => 1,
            (_, true, true) => 2,
        };
        let half = self.w / 2;
        blit256(c, WIDGETS, self.x, self.y, 0, 46 + state * 20, half, self.h);
        blit256(
            c,
            WIDGETS,
            self.x + half,
            self.y,
            200 - half,
            46 + state * 20,
            half,
            self.h,
        );
        if let Some(s) = self.slider {
            let kx = self.x + (s.pos * (self.w - 8) as f32) as i32;
            blit256(c, WIDGETS, kx, self.y, 0, 66, 4, 20);
            blit256(c, WIDGETS, kx + 4, self.y, 196, 66, 4, 20);
        }
        let color = if !self.enabled {
            0xa0a0a0
        } else if self.hovered {
            0xffffa0
        } else {
            0xe0e0e0
        };
        centered(
            c,
            &self.label,
            self.x + self.w / 2,
            self.y + (self.h - 8) / 2,
            color,
        );
    }
}

/// `GuiScreen.mouseClicked` for left clicks: the id of the pressed button, with the click sound.
pub fn press(buttons: &mut [Button], x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<i32> {
    if button != 0 {
        return None;
    }
    let id = buttons
        .iter_mut()
        .find_map(|b| b.pressed(x, y).then_some(b.id))?;
    ctx.effects.push(Effect::Sound("gui.button.press"));
    Some(id)
}

/// `GuiScreen.mouseReleased`.
pub fn release_all(buttons: &mut [Button]) {
    buttons.iter_mut().for_each(Button::released);
}

pub fn draw_buttons(buttons: &mut [Button], c: &mut dyn Canvas, mx: i32, my: i32) {
    buttons.iter_mut().for_each(|b| b.draw(c, mx, my));
}

pub fn button(buttons: &mut [Button], id: i32) -> &mut Button {
    buttons
        .iter_mut()
        .find(|b| b.id == id)
        .expect("button id exists")
}

/// `GuiTextField`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextField {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub text: String,
    pub max_len: usize,
    /// `enableBackgroundDrawing`: the grey frame and black box, and 4 units of padding.
    pub background: bool,
    /// `setTextColor`, as RGB.
    pub color: u32,
    pub focused: bool,
    ticks: u32,
    /// Char indices: cursor, selection end, first visible char.
    cursor: usize,
    sel: usize,
    scroll: usize,
}

impl TextField {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> TextField {
        TextField {
            x,
            y,
            w,
            h,
            text: String::new(),
            max_len: 32,
            background: true,
            color: 0xe0e0e0,
            focused: false,
            ticks: 0,
            cursor: 0,
            sel: 0,
            scroll: 0,
        }
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, i: usize) -> usize {
        self.text
            .char_indices()
            .nth(i)
            .map_or(self.text.len(), |(b, _)| b)
    }

    fn slice(&self, a: usize, b: usize) -> &str {
        &self.text[self.byte(a)..self.byte(b)]
    }

    /// `setText`.
    pub fn set(&mut self, s: &str, m: &dyn Metrics) {
        self.text = s.chars().take(self.max_len).collect();
        self.move_end(m);
    }

    /// `updateCursorCounter`.
    pub fn tick(&mut self) {
        self.ticks += 1;
    }

    pub fn set_focused(&mut self, f: bool) {
        if f && !self.focused {
            self.ticks = 0;
        }
        self.focused = f;
    }

    fn pad(&self) -> i32 {
        if self.background {
            4
        } else {
            0
        }
    }

    fn inner_width(&self) -> i32 {
        self.w - 2 * self.pad()
    }

    fn selected(&self) -> &str {
        let (a, b) = (self.cursor.min(self.sel), self.cursor.max(self.sel));
        self.slice(a, b)
    }

    /// `writeText`.
    fn write(&mut self, s: &str, m: &dyn Metrics) {
        let allowed: String = s.chars().filter(|&c| allowed_char(c)).collect();
        let (a, b) = (self.cursor.min(self.sel), self.cursor.max(self.sel));
        let room = self.max_len.saturating_sub(self.len() - (b - a));
        let ins: String = allowed.chars().take(room).collect();
        let n = ins.chars().count();
        self.text = format!("{}{}{}", self.slice(0, a), ins, self.slice(b, self.len()));
        self.set_cursor(a + n, m);
    }

    /// `deleteFromCursor`: `n` chars (negative = before the cursor), or the selection.
    fn delete(&mut self, n: isize, m: &dyn Metrics) {
        if self.text.is_empty() {
            return;
        }
        if self.sel != self.cursor {
            self.write("", m);
            return;
        }
        let c = self.cursor as isize;
        let (a, b) = if n < 0 { (c + n, c) } else { (c, c + n) };
        let (a, b) = (a.max(0) as usize, (b as usize).min(self.len()));
        self.text = format!("{}{}", self.slice(0, a), self.slice(b, self.len()));
        if n < 0 {
            self.set_cursor(a, m);
        } else {
            self.set_cursor(self.cursor, m);
        }
    }

    /// `getNthWordFromPos`.
    fn word(&self, n: i32, from: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = from;
        for _ in 0..n.abs() {
            if n > 0 {
                i = chars[i.min(chars.len())..]
                    .iter()
                    .position(|&c| c == ' ')
                    .map_or(chars.len(), |p| i + p);
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
            } else {
                while i > 0 && chars[i - 1] == ' ' {
                    i -= 1;
                }
                while i > 0 && chars[i - 1] != ' ' {
                    i -= 1;
                }
            }
        }
        i
    }

    /// `setCursorPosition`: moves both ends.
    fn set_cursor(&mut self, i: usize, m: &dyn Metrics) {
        self.cursor = i.min(self.len());
        self.set_sel(self.cursor, m);
    }

    fn move_end(&mut self, m: &dyn Metrics) {
        self.set_cursor(self.len(), m);
    }

    /// `setSelectionPos`: also scrolls so the selection end stays visible.
    fn set_sel(&mut self, i: usize, m: &dyn Metrics) {
        let len = self.len();
        let i = i.min(len);
        self.sel = i;
        self.scroll = self.scroll.min(len);
        let w = self.inner_width();
        let visible = trim(m, self.slice(self.scroll, len), w, false)
            .chars()
            .count();
        let end = visible + self.scroll;
        if i == self.scroll {
            self.scroll -= trim(m, &self.text, w, true)
                .chars()
                .count()
                .min(self.scroll);
        }
        if i > end {
            self.scroll += i - end;
        } else if i <= self.scroll {
            self.scroll = i;
        }
        self.scroll = self.scroll.min(len);
    }

    /// `textboxKeyTyped`; true when the field used the key.
    pub fn key(
        &mut self,
        code: i32,
        ch: char,
        mods: Mods,
        m: &dyn Metrics,
        clip: &mut String,
    ) -> bool {
        if !self.focused {
            return false;
        }
        if mods.ctrl && !mods.shift && self.clip_key(code, m, clip) {
            return true;
        }
        match code {
            key::BACK | key::DELETE => self.delete_key(code == key::BACK, mods.ctrl, m),
            key::HOME | key::END => self.home_end(code == key::END, mods.shift, m),
            key::LEFT | key::RIGHT => self.arrow(code == key::LEFT, mods, m),
            _ if allowed_char(ch) => self.write(&ch.to_string(), m),
            _ => return false,
        }
        true
    }

    /// Ctrl+A, C, V and X; true when `code` is one of them.
    fn clip_key(&mut self, code: i32, m: &dyn Metrics, clip: &mut String) -> bool {
        match code {
            key::A => {
                self.move_end(m);
                self.set_sel(0, m);
            }
            key::C => *clip = self.selected().to_owned(),
            key::V => {
                let s = clip.clone();
                self.write(&s, m);
            }
            key::X => {
                *clip = self.selected().to_owned();
                self.write("", m);
            }
            _ => return false,
        }
        true
    }

    fn arrow(&mut self, left: bool, mods: Mods, m: &dyn Metrics) {
        let dir = if left { -1 } else { 1 };
        let from = if mods.shift || !mods.ctrl {
            self.sel
        } else {
            self.cursor
        };
        let to = if mods.ctrl {
            self.word(dir, from)
        } else {
            (from as isize + dir as isize).max(0) as usize
        };
        if mods.shift {
            self.set_sel(to, m);
        } else {
            self.set_cursor(to, m);
        }
    }

    /// Backspace or Delete; with Ctrl a whole word.
    fn delete_key(&mut self, back: bool, ctrl: bool, m: &dyn Metrics) {
        let dir = if back { -1 } else { 1 };
        let n = if ctrl {
            self.word(dir, self.cursor) as isize - self.cursor as isize
        } else {
            dir as isize
        };
        self.delete(n, m);
    }

    fn home_end(&mut self, end: bool, shift: bool, m: &dyn Metrics) {
        match (end, shift) {
            (false, true) => self.set_sel(0, m),
            (false, false) => self.set_cursor(0, m),
            (true, true) => self.set_sel(self.len(), m),
            (true, false) => self.move_end(m),
        }
    }

    /// `mouseClicked`: focus follows the click; a left click inside places the cursor.
    pub fn click(&mut self, x: i32, y: i32, button: i32, m: &dyn Metrics) {
        let inside = x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h;
        self.set_focused(inside);
        if inside && button == 0 {
            let rel = x - self.x - self.pad();
            let shown = trim(
                m,
                self.slice(self.scroll, self.len()),
                self.inner_width(),
                false,
            );
            let n = trim(m, &shown, rel, false).chars().count();
            self.set_cursor(n + self.scroll, m);
        }
    }

    fn draw_frame(&self, c: &mut dyn Canvas) {
        if !self.background {
            return;
        }
        c.fill(
            self.x - 1,
            self.y - 1,
            self.x + self.w + 1,
            self.y + self.h + 1,
            0xffa0a0a0,
        );
        c.fill(self.x, self.y, self.x + self.w, self.y + self.h, 0xff000000);
    }

    /// Where the text starts: padded and centred in the box, or at the corner without a background.
    fn text_origin(&self) -> (i32, i32) {
        if self.background {
            (self.x + 4, self.y + (self.h - 8) / 2)
        } else {
            (self.x, self.y)
        }
    }

    /// `drawTextBox`.
    pub fn draw(&self, c: &mut dyn Canvas) {
        self.draw_frame(c);
        let color = self.color;
        let len = self.len();
        let shown = trim(c, self.slice(self.scroll, len), self.inner_width(), false);
        let shown_len = shown.chars().count();
        let cur = self.cursor as isize - self.scroll as isize;
        let sel = (self.sel as isize - self.scroll as isize).min(shown_len as isize);
        let cursor_in = cur >= 0 && cur as usize <= shown_len;
        let blink = self.focused && (self.ticks / 6).is_multiple_of(2) && cursor_in;
        let (x0, y0) = self.text_origin();
        let byte_at = |i: usize| shown.char_indices().nth(i).map_or(shown.len(), |(b, _)| b);
        let mut x = x0;
        if !shown.is_empty() {
            let head = if cursor_in {
                &shown[..byte_at(cur as usize)]
            } else {
                &shown[..]
            };
            c.text(head, x as f32, y0 as f32, color, true);
            x += c.width(head) + 1;
        }
        let mid = self.cursor < len || len >= self.max_len;
        let (caret, x) = caret_x(cursor_in, mid, cur, x, x0, self.w);
        if !shown.is_empty() && cursor_in && (cur as usize) < shown_len {
            c.text(
                &shown[byte_at(cur as usize)..],
                x as f32,
                y0 as f32,
                color,
                true,
            );
        }
        if blink {
            draw_caret(c, mid, caret, y0, color);
        }
        if sel != cur {
            let sx = x0 + c.width(&shown[..byte_at(sel.max(0) as usize)]);
            let (a, b) = (caret.min(sx - 1), caret.max(sx - 1));
            // shortcut: vanilla inverts with GL_OR_REVERSE; a translucent blue fill is the closest Canvas call.
            c.fill(a, y0 - 1, b.min(self.x + self.w), y0 + 1 + 9, 0x800000ff);
        }
    }
}

/// Caret x and where the text after the caret starts.
fn caret_x(cursor_in: bool, mid: bool, cur: isize, x: i32, x0: i32, w: i32) -> (i32, i32) {
    if !cursor_in {
        (if cur > 0 { x0 + w } else { x0 }, x)
    } else if mid {
        (x - 1, x - 1)
    } else {
        (x, x)
    }
}

fn draw_caret(c: &mut dyn Canvas, mid: bool, caret: i32, y0: i32, color: u32) {
    if mid {
        c.fill(caret, y0 - 1, caret + 1, y0 + 1 + 9, 0xffd0d0d0);
    } else {
        c.text("_", caret as f32, y0 as f32, color, true);
    }
}

/// `ChatAllowedCharacters.isAllowedCharacter`.
pub fn allowed_char(c: char) -> bool {
    c != '§' && c >= ' ' && c != '\u{7f}'
}

/// `GuiSlot`/`GuiListExtended` geometry and scrolling; the screen draws the rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollList {
    /// `left`: the list spans `left..left + width`.
    pub left: i32,
    pub width: i32,
    pub height: i32,
    pub top: i32,
    pub bottom: i32,
    pub slot_h: i32,
    /// `headerPadding`: space above the first row.
    pub header: i32,
    /// `getListWidth`.
    pub row_w: i32,
    /// `getScrollBarX`.
    pub bar_x: i32,
    pub scroll: f32,
    /// `initialClickY`: -1 dragging none, -2 idle, >= 0 drag anchor.
    drag_y: i32,
    drag_factor: f32,
}

impl ScrollList {
    pub fn new(width: i32, height: i32, top: i32, bottom: i32, slot_h: i32) -> ScrollList {
        ScrollList {
            left: 0,
            width,
            height,
            top,
            bottom,
            slot_h,
            header: 0,
            row_w: 220,
            bar_x: width / 2 + 124,
            scroll: 0.0,
            drag_y: -2,
            drag_factor: 1.0,
        }
    }

    /// `setSlotXBoundsFromLeft` for a list of `width` whose rows fill it, with a header
    /// (`GuiResourcePackList`).
    pub fn at(mut self, left: i32, header: i32) -> ScrollList {
        self.left = left;
        self.header = header;
        self.row_w = self.width;
        self.bar_x = left + self.width - 6;
        self
    }

    fn content_h(&self, rows: usize) -> i32 {
        rows as i32 * self.slot_h + self.header
    }

    /// `getMaxScroll`.
    pub fn max_scroll(&self, rows: usize) -> i32 {
        (self.content_h(rows) - (self.bottom - self.top - 4)).max(0)
    }

    fn bind(&mut self, rows: usize) {
        self.scroll = self.scroll.clamp(0.0, self.max_scroll(rows) as f32);
    }

    /// `scrollBy`.
    pub fn scroll_by(&mut self, d: i32, rows: usize) {
        self.scroll += d as f32;
        self.bind(rows);
        self.drag_y = -2;
    }

    /// Left edge of the rows.
    pub fn row_x(&self) -> i32 {
        self.left + self.width / 2 - self.row_w / 2 + 2
    }

    /// Top of row `i` on screen.
    pub fn row_y(&self, i: usize) -> i32 {
        self.top + 4 - self.scroll as i32 + i as i32 * self.slot_h + self.header
    }

    /// `getSlotIndexFromScreenCoords`.
    pub fn row_at(&self, x: i32, y: i32, rows: usize) -> Option<usize> {
        let (l, r) = self.row_edges();
        let rel = y - self.top - self.header + self.scroll as i32 - 4;
        let i = rel / self.slot_h;
        (x < self.bar_x && x >= l && x <= r && i >= 0 && rel >= 0 && (i as usize) < rows)
            .then_some(i as usize)
    }

    fn row_edges(&self) -> (i32, i32) {
        let mid = self.left + self.width / 2;
        (mid - self.row_w / 2, mid + self.row_w / 2)
    }

    /// Left-button press on the list: starts a scroll-bar or content drag (`GuiSlot.handleMouseInput`).
    pub fn press(&mut self, x: i32, y: i32, rows: usize) {
        if y < self.top || y > self.bottom {
            self.drag_y = -2;
            return;
        }
        if x >= self.bar_x && x <= self.bar_x + 6 {
            let max = self.max_scroll(rows).max(1);
            let span = self.bottom - self.top;
            // `MathHelper.clamp_int` never panics, even when the list is shorter than 40 units.
            let knob = (span * span / self.content_h(rows).max(1))
                .min(span - 8)
                .max(32);
            self.drag_factor = -1.0 / ((span - knob) as f32 / max as f32);
        } else {
            self.drag_factor = 1.0;
        }
        self.drag_y = y;
    }

    /// Mouse held at `my` during a frame: drag-scrolls the list.
    pub fn drag(&mut self, my: i32, rows: usize) {
        if self.drag_y >= 0 {
            self.scroll -= (my - self.drag_y) as f32 * self.drag_factor;
            self.drag_y = my;
            self.bind(rows);
        }
    }

    pub fn release(&mut self) {
        self.drag_y = -1;
    }

    pub fn wheel(&mut self, dir: i32, rows: usize) {
        self.scroll += (-dir.signum() * self.slot_h / 2) as f32;
        self.bind(rows);
    }

    /// Dirt behind the rows (`drawScreen` before `drawSelectionBox`).
    pub fn draw_back(&mut self, c: &mut dyn Canvas, rows: usize) {
        self.bind(rows);
        dirt(
            c,
            self.left,
            self.top,
            self.left + self.width,
            self.bottom,
            self.scroll as i32,
            0xff202020,
        );
    }

    /// `drawSelectionBox` highlight for row `i`.
    pub fn draw_selection(&self, c: &mut dyn Canvas, i: usize) {
        let (l, r) = self.row_edges();
        let y = self.row_y(i);
        let h = self.slot_h - 4;
        c.fill(l, y - 2, r, y + h + 2, 0xff808080);
        c.fill(l + 1, y - 1, r - 1, y + h + 1, 0xff000000);
    }

    /// Whether row `i` is (partly) inside the list area.
    pub fn row_visible(&self, i: usize) -> bool {
        let y = self.row_y(i);
        y <= self.bottom && y + self.slot_h - 4 >= self.top
    }

    /// Dirt bands, shadows and the scroll bar over the rows.
    pub fn draw_front(&self, c: &mut dyn Canvas, rows: usize) {
        let (x0, x1, w) = (self.left, self.left + self.width, self.width as f32);
        // `overlayBackground`: the texture starts at the list's left edge.
        for (y0, y1) in [(0, self.top), (self.bottom, self.height)] {
            let uv = [0.0, y0 as f32 / 32.0, w / 32.0, y1 as f32 / 32.0];
            let h = (y1 - y0) as f32;
            c.blit(
                OPTIONS_BACKGROUND,
                x0 as f32,
                y0 as f32,
                w,
                h,
                uv,
                0xff404040,
            );
        }
        c.gradient(x0, self.top, x1, self.top + 4, 0xff000000, 0x00000000);
        c.gradient(x0, self.bottom - 4, x1, self.bottom, 0x00000000, 0xff000000);
        let max = self.max_scroll(rows);
        if max > 0 {
            let span = self.bottom - self.top;
            let knob = (span * span / self.content_h(rows)).min(span - 8).max(32);
            let y = (self.scroll as i32 * (span - knob) / max + self.top).max(self.top);
            let (x0, x1) = (self.bar_x, self.bar_x + 6);
            c.fill(x0, self.top, x1, self.bottom, 0xff000000);
            c.fill(x0, y, x1, y + knob, 0xff808080);
            c.fill(x0, y, x1 - 1, y + knob - 1, 0xffc0c0c0);
        }
    }
}

/// `GuiYesNo`: asks a question and hands the answer to the screen that opened it.
pub struct YesNo {
    parent: Option<Box<dyn Screen>>,
    title: String,
    message: String,
    yes: String,
    no: String,
    id: i32,
    lines: Vec<String>,
    buttons: Vec<Button>,
    /// `setButtonDelay`: ticks until the buttons enable.
    delay: i32,
}

impl YesNo {
    pub fn new(
        parent: Box<dyn Screen>,
        title: String,
        message: String,
        yes: String,
        no: String,
        id: i32,
    ) -> YesNo {
        YesNo {
            parent: Some(parent),
            title,
            message,
            yes,
            no,
            id,
            lines: Vec::new(),
            buttons: Vec::new(),
            delay: 0,
        }
    }

    pub fn with_delay(mut self, ticks: i32) -> YesNo {
        self.delay = ticks;
        self
    }
}

impl Screen for YesNo {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![
            Button::new(0, w / 2 - 155, h / 6 + 96, 150, 20, self.yes.clone()),
            Button::new(1, w / 2 - 155 + 160, h / 6 + 96, 150, 20, self.no.clone()),
        ];
        self.lines.clear();
        let enabled = self.delay <= 0;
        self.buttons.iter_mut().for_each(|b| b.enabled = enabled);
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if self.lines.is_empty() && !self.message.is_empty() {
            self.lines = wrap(c, &self.message, ctx.width - 50);
        }
        default_background(c, ctx);
        centered(c, &self.title, ctx.width / 2, 70, WHITE);
        for (i, l) in self.lines.iter().enumerate() {
            centered(c, l, ctx.width / 2, 90 + i as i32 * 9, WHITE);
        }
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        self.delay -= 1;
        if self.delay == 0 {
            self.buttons.iter_mut().for_each(|b| b.enabled = true);
        }
        None
    }

    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav> {
        let id = press(&mut self.buttons, x, y, button, ctx)?;
        Some(answer(self.parent.take()?, id == 0, self.id, ctx))
    }
}

/// Give a dialog's answer to the screen that opened it, and return to that screen or to its choice.
pub fn answer(mut parent: Box<dyn Screen>, yes: bool, id: i32, ctx: &mut Ctx) -> Nav {
    match parent.confirm(yes, id, ctx) {
        None => Nav::Open(parent),
        // The parent, not the dialog, owns a sub-screen it opens.
        Some(Nav::Child(f)) => Nav::Open(f(parent)),
        Some(nav) => nav,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every char is 6 units wide; `§x` codes are free.
    pub(crate) struct Mono;
    impl Metrics for Mono {
        fn width(&self, s: &str) -> i32 {
            let mut n = 0;
            let mut it = s.chars();
            while let Some(c) = it.next() {
                if c == '§' {
                    it.next();
                } else {
                    n += 6;
                }
            }
            n
        }
    }

    #[test]
    fn unicode_locale_needs_over_ten_percent_wide_chars() {
        let t = |lang: &str| Texts::parse(lang, "").unicode;
        assert!(!t("a=Hello\nb=Français"));
        assert!(t("a=Привет\nb=Hello"));
        // en_US under the chosen language: the English keys count too (6 of 66 is under 10%).
        assert!(!t(&format!("a=Привет\nb={}", "e".repeat(60))));
        assert!(!t(""));
    }

    #[test]
    fn lang_format_forms() {
        let t = Texts::parse(
            "a=%s and %s\nb=%2$s %1$s 100%%\nkey.mouseButton=Button %1$s",
            "",
        );
        assert_eq!(t.fmt("a", &["x", "y"]), "x and y");
        assert_eq!(t.fmt("b", &["x", "y"]), "y x 100%");
        assert_eq!(t.fmt("key.mouseButton", &["4"]), "Button 4");
        assert_eq!(t.tr("missing.key"), "missing.key");
    }

    #[test]
    fn scaled_resolution_matches_vanilla() {
        let at = |w, h, g, u| {
            let s = GuiScale::new(w, h, g, u);
            (s.width, s.height, s.factor)
        };
        assert_eq!(at(854, 480, 0, false), (427, 240, 2));
        assert_eq!(at(1920, 1080, 2, false), (960, 540, 2));
        assert_eq!(at(1920, 1080, 0, true), (480, 270, 4));
        assert_eq!(at(1440, 900, 0, true), (720, 450, 2));
    }

    #[test]
    fn wrap_breaks_at_spaces_and_carries_color() {
        let lines = wrap(&Mono, "§cab cd ef", 30);
        assert_eq!(lines, vec!["§cab cd", "§cef"]);
        assert_eq!(wrap(&Mono, "abcdefgh", 24), vec!["abcd", "efgh"]);
        assert_eq!(wrap(&Mono, "§6§lab\ncd", 100), vec!["§6§lab", "§6§lcd"]);
    }

    #[test]
    fn text_field_edits_like_vanilla() {
        let mut f = TextField::new(0, 0, 200, 20);
        f.focused = true;
        let mut clip = String::new();
        let none = Mods::default();
        for ch in "ab c".chars() {
            f.key(0, ch, none, &Mono, &mut clip);
        }
        assert_eq!(f.text, "ab c");
        f.key(
            key::BACK,
            '\0',
            Mods {
                ctrl: true,
                shift: false,
            },
            &Mono,
            &mut clip,
        );
        assert_eq!(f.text, "ab ");
        f.key(
            key::A,
            'a',
            Mods {
                ctrl: true,
                shift: false,
            },
            &Mono,
            &mut clip,
        );
        f.key(
            key::X,
            'x',
            Mods {
                ctrl: true,
                shift: false,
            },
            &Mono,
            &mut clip,
        );
        assert_eq!((f.text.as_str(), clip.as_str()), ("", "ab "));
        f.max_len = 2;
        f.key(
            key::V,
            'v',
            Mods {
                ctrl: true,
                shift: false,
            },
            &Mono,
            &mut clip,
        );
        assert_eq!(f.text, "ab");
        f.key(0, '§', none, &Mono, &mut clip);
        assert_eq!(f.text, "ab");
    }

    #[test]
    fn list_scroll_is_clamped_and_hits_rows() {
        let mut l = ScrollList::new(400, 300, 32, 236, 36);
        l.wheel(-1, 10);
        assert_eq!(l.scroll, 18.0);
        l.scroll_by(10_000, 10);
        assert_eq!(l.scroll as i32, l.max_scroll(10));
        l.scroll = 0.0;
        assert_eq!(l.row_at(200, 32 + 4 + 36 + 1, 10), Some(1));
        assert_eq!(l.row_at(200, 20, 10), None);
    }
}
