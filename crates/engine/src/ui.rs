//! Menu screens: owns the `gui` screen stack, its settings and texts, and runs each call with a `Ctx`.

use gui::screens::{Canvas, Metrics};
use gui::screens::{Ctx, Effect, Event, Screen, Screens, Settings, Texts};
use std::path::PathBuf;
use std::time::Instant;

pub struct Ui {
    pub screens: Screens,
    pub settings: Settings,
    pub texts: Texts,
    /// Process-local copy; the system clipboard is mirrored when one is available.
    clipboard: String,
    system: Option<arboard::Clipboard>,
    /// Scaled screen size in GUI units, from the last [`Ui::resize`].
    pub width: i32,
    pub height: i32,
    pub in_world: bool,
    pub difficulty: Option<u8>,
    pub hardcore: bool,
    game_dir: PathBuf,
    started: Instant,
}

impl Ui {
    pub fn new(game_dir: PathBuf, texts: Texts) -> Ui {
        Ui {
            screens: Screens::default(),
            settings: Settings::load(game_dir.clone()),
            texts,
            clipboard: String::new(),
            system: arboard::Clipboard::new().ok(),
            width: 0,
            height: 0,
            in_world: false,
            difficulty: None,
            hardcore: false,
            game_dir,
            started: Instant::now(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.screens.is_open()
    }

    /// `Minecraft.isUnicode`: the language or `forceUnicodeFont` asks for the unicode glyphs.
    pub fn unicode(&self) -> bool {
        self.settings.force_unicode || self.texts.unicode
    }

    /// `LanguageManager.isCurrentLanguageBidirectional`: `FontRenderer.bidiFlag`.
    pub fn bidirectional(&self) -> bool {
        self.texts.bidirectional(&self.settings.lang)
    }

    /// Runs `f` with a `Ctx`; returns the effects the screens pushed.
    fn run<R>(
        &mut self,
        m: &dyn Metrics,
        shift: bool,
        f: impl FnOnce(&mut Screens, &mut Ctx) -> R,
    ) -> (R, Vec<Effect>) {
        let mut ctx = Ctx {
            width: self.width,
            height: self.height,
            texts: &self.texts,
            settings: &mut self.settings,
            in_world: self.in_world,
            difficulty: self.difficulty,
            hardcore: self.hardcore,
            now_ms: self.started.elapsed().as_millis() as u64,
            game_dir: self.game_dir.clone(),
            effects: Vec::new(),
            clipboard: &mut self.clipboard,
            metrics: m,
            shift,
        };
        let before = ctx.clipboard.clone();
        let r = f(&mut self.screens, &mut ctx);
        let changed = *ctx.clipboard != before;
        let (w, h, effects) = (ctx.width, ctx.height, ctx.effects);
        (self.width, self.height) = (w, h);
        if changed {
            if let Some(sys) = &mut self.system {
                // A failed write keeps the process-local copy, so paste still works in-game.
                let _ = sys.set_text(self.clipboard.clone());
            }
        }
        (r, effects)
    }

    pub fn open(&mut self, m: &dyn Metrics, screen: Box<dyn Screen>) -> Vec<Effect> {
        self.run(m, false, |s, c| s.open(screen, c)).1
    }

    /// `displayGuiScreen(null)`: the game, or the title screen without a world.
    pub fn close(&mut self, m: &dyn Metrics) -> Vec<Effect> {
        self.run(m, false, |s, c| s.close(c)).1
    }

    pub fn resize(&mut self, m: &dyn Metrics, px: (u32, u32)) -> Vec<Effect> {
        self.run(m, false, |s, c| {
            s.resize(px.0, px.1, c);
        })
        .1
    }

    pub fn event(&mut self, m: &dyn Metrics, ev: Event, shift: bool) -> Vec<Effect> {
        if matches!(ev, Event::Key { ctrl: true, .. } | Event::Click { .. }) {
            if let Some(text) = self.system.as_mut().and_then(|c| c.get_text().ok()) {
                self.clipboard = text;
            }
        }
        self.run(m, shift, |s, c| s.event(ev, c)).1
    }

    pub fn tick(&mut self, m: &dyn Metrics) -> Vec<Effect> {
        self.run(m, false, |s, c| s.tick(c)).1
    }

    pub fn draw(&mut self, m: &dyn Metrics, c: &mut dyn Canvas, mx: i32, my: i32) -> Vec<Effect> {
        self.run(m, false, |s, ctx| s.draw(c, mx, my, ctx)).1
    }

    /// A fresh title screen, as the parent of the login screen.
    pub fn title(&mut self, m: &dyn Metrics) -> Box<dyn Screen> {
        self.run(m, false, |_, c| {
            Box::new(gui::screens::Title::new(c)) as Box<dyn Screen>
        })
        .0
    }

    pub fn connect_failed(&mut self, m: &dyn Metrics, reason: &str) -> Vec<Effect> {
        self.run(m, false, |s, c| s.connect_failed(reason, c)).1
    }

    pub fn joined(&mut self, m: &dyn Metrics) -> Vec<Effect> {
        self.run(m, false, |s, c| s.joined(c)).1
    }

    pub fn disconnected(&mut self, m: &dyn Metrics, reason: &str) -> Vec<Effect> {
        self.run(m, false, |s, c| s.disconnected(reason, c)).1
    }
}

/// Font measurement for screens, from the renderer's font.
pub struct Fm<'a>(pub gui::Metrics<'a>);

impl Metrics for Fm<'_> {
    fn width(&self, s: &str) -> i32 {
        self.0.width(s)
    }
}
