//! `GuiDisconnected`.

use super::{
    centered, default_background, draw_buttons, press, release_all, wrap, Button, Canvas, Ctx,
    Mods, Nav, Screen, WHITE,
};

pub struct Disconnected {
    parent: Option<Box<dyn Screen>>,
    /// Translation key of the title line, such as `connect.failed`.
    reason: &'static str,
    message: String,
    lines: Option<Vec<String>>,
    buttons: Vec<Button>,
}

impl Disconnected {
    pub fn new(parent: Box<dyn Screen>, reason: &'static str, message: String) -> Disconnected {
        Disconnected {
            parent: Some(parent),
            reason,
            message,
            lines: None,
            buttons: Vec::new(),
        }
    }
}

impl Screen for Disconnected {
    fn init(&mut self, ctx: &mut Ctx) {
        self.lines = None;
        self.buttons = vec![Button::wide(
            0,
            ctx.width / 2 - 100,
            0,
            ctx.tr("gui.toMenu"),
        )];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        // The wrap needs the font, so it waits for the first frame; `initGui` does it in vanilla.
        let lines = self
            .lines
            .get_or_insert_with(|| wrap(c, &self.message, w - 50));
        let text_h = lines.len() as i32 * 9;
        self.buttons[0].y = h / 2 + text_h / 2 + 9;
        default_background(c, ctx);
        centered(
            c,
            &ctx.tr(self.reason),
            w / 2,
            h / 2 - text_h / 2 - 9 * 2,
            0xaaaaaa,
        );
        for (i, l) in lines.iter().enumerate() {
            centered(c, l, w / 2, h / 2 - text_h / 2 + i as i32 * 9, WHITE);
        }
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    /// `GuiDisconnected.keyTyped` ignores every key.
    fn key(&mut self, _code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        press(&mut self.buttons, x, y, mb, ctx)?;
        self.parent.take().map(Nav::Open)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }
}
