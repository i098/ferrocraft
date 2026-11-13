//! The `GuiYesNo` `NetHandlerPlayClient.handleResourcePack` opens before it accepts a server resource pack.

use super::{
    centered, default_background, draw_buttons, press, release_all, wrap, Button, Canvas, Ctx,
    Effect, Nav, Screen, WHITE,
};

pub struct PackPrompt {
    url: String,
    hash: String,
    lines: Vec<String>,
    buttons: Vec<Button>,
}

impl PackPrompt {
    pub fn new(url: String, hash: String) -> PackPrompt {
        PackPrompt {
            url,
            hash,
            lines: Vec::new(),
            buttons: Vec::new(),
        }
    }
}

impl Screen for PackPrompt {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![
            Button::new(0, w / 2 - 155, h / 6 + 96, 150, 20, ctx.tr("gui.yes")),
            Button::new(1, w / 2 - 155 + 160, h / 6 + 96, 150, 20, ctx.tr("gui.no")),
        ];
        self.lines.clear();
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if self.lines.is_empty() {
            let text = ctx.tr("multiplayer.texturePrompt.line2");
            self.lines = wrap(c, &text, ctx.width - 50);
        }
        default_background(c, ctx);
        let title = ctx.tr("multiplayer.texturePrompt.line1");
        centered(c, &title, ctx.width / 2, 70, WHITE);
        for (i, l) in self.lines.iter().enumerate() {
            centered(c, l, ctx.width / 2, 90 + i as i32 * 9, WHITE);
        }
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    /// Escape closes the screen without an answer, as `GuiYesNo` does.
    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav> {
        let id = press(&mut self.buttons, x, y, button, ctx)?;
        ctx.effects.push(Effect::ServerPackAnswer {
            url: self.url.clone(),
            hash: self.hash.clone(),
            accepted: id == 0,
        });
        Some(Nav::Game)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }
}
