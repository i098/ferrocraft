//! `GuiConnecting` and `GuiDownloadingTerrain`.

use super::{
    centered, default_background, dirt, draw_buttons, press, release_all, Button, Canvas, Ctx,
    Effect, Mods, Nav, Screen, WHITE,
};

pub struct Connecting {
    pub parent: Option<Box<dyn Screen>>,
    host: String,
    port: u16,
    started: bool,
    buttons: Vec<Button>,
}

impl Connecting {
    pub fn new(parent: Box<dyn Screen>, host: String, port: u16) -> Connecting {
        Connecting {
            parent: Some(parent),
            host,
            port,
            started: false,
            buttons: Vec::new(),
        }
    }
}

impl Screen for Connecting {
    fn init(&mut self, ctx: &mut Ctx) {
        if !self.started {
            self.started = true;
            ctx.effects.push(Effect::Connect {
                host: self.host.clone(),
                port: self.port,
            });
        }
        self.buttons = vec![Button::wide(
            0,
            ctx.width / 2 - 100,
            ctx.height / 4 + 120 + 12,
            ctx.tr("gui.cancel"),
        )];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        // The login runs on the engine side; until `JoinGame` the vanilla text is "Connecting to the server...".
        centered(
            c,
            &ctx.tr("connect.connecting"),
            ctx.width / 2,
            ctx.height / 2 - 50,
            WHITE,
        );
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    /// `GuiConnecting.keyTyped` ignores every key.
    fn key(&mut self, _code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        press(&mut self.buttons, x, y, mb, ctx)?;
        ctx.effects.push(Effect::CancelConnect);
        self.parent.take().map(Nav::Open)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }

    fn as_connecting(&mut self) -> Option<&mut Connecting> {
        Some(self)
    }
}

/// Shown from `JoinGame` until the first position packet; the engine then closes it.
#[derive(Default)]
pub struct DownloadingTerrain;

impl Screen for DownloadingTerrain {
    fn init(&mut self, _ctx: &mut Ctx) {}

    fn draw(&mut self, c: &mut dyn Canvas, _mx: i32, _my: i32, ctx: &mut Ctx) {
        dirt(c, 0, 0, ctx.width, ctx.height, 0, 0xff404040);
        centered(
            c,
            &ctx.tr("multiplayer.downloadingTerrain"),
            ctx.width / 2,
            ctx.height / 2 - 50,
            WHITE,
        );
    }

    fn key(&mut self, _code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn click(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn pauses(&self) -> bool {
        false
    }
}
