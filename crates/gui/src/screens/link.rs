//! `GuiConfirmOpenLink`: the prompt before a chat `open_url` click opens the system browser.

use super::{
    centered, default_background, draw_buttons, press, release_all, Button, Canvas, Ctx, Effect,
    Nav, Screen, WHITE,
};

pub struct ConfirmLink {
    url: String,
    buttons: Vec<Button>,
}

impl ConfirmLink {
    pub fn new(url: String) -> ConfirmLink {
        ConfirmLink {
            url,
            buttons: Vec::new(),
        }
    }
}

impl Screen for ConfirmLink {
    fn init(&mut self, ctx: &mut Ctx) {
        let (x, y) = (ctx.width / 2 - 50, ctx.height / 6 + 96);
        self.buttons = vec![
            Button::new(0, x - 105, y, 100, 20, ctx.tr("gui.yes")),
            Button::new(2, x, y, 100, 20, ctx.tr("chat.copy")),
            Button::new(1, x + 105, y, 100, 20, ctx.tr("gui.no")),
        ];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        centered(c, &ctx.tr("chat.link.confirm"), ctx.width / 2, 70, WHITE);
        centered(c, &self.url, ctx.width / 2, 90, WHITE);
        centered(
            c,
            &ctx.tr("chat.link.warning"),
            ctx.width / 2,
            110,
            0xFFCCCC,
        );
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    /// Yes opens the link; Copy puts it on the clipboard and then answers no, as vanilla does.
    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, button, ctx)? {
            0 => ctx.effects.push(Effect::OpenUrl(self.url.clone())),
            2 => *ctx.clipboard = self.url.clone(),
            _ => {}
        }
        Some(Nav::Game)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }
}
