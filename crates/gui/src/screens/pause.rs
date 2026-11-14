//! `GuiIngameMenu`, the Escape menu.

use super::{
    centered, default_background, draw_buttons, press, release_all, Achievements, Button, Canvas,
    Ctx, Effect, Multiplayer, Nav, Options, Screen, Title, WHITE,
};

const OPTIONS: i32 = 0;
const DISCONNECT: i32 = 1;
const BACK: i32 = 4;
const ACHIEVEMENTS: i32 = 5;
const STATS: i32 = 6;
const LAN: i32 = 7;

#[derive(Default)]
pub struct Pause {
    buttons: Vec<Button>,
}

impl Screen for Pause {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let i = -16;
        // Always a remote server here, so the button reads "Disconnect".
        self.buttons = vec![
            Button::wide(
                DISCONNECT,
                w / 2 - 100,
                h / 4 + 120 + i,
                ctx.tr("menu.disconnect"),
            ),
            Button::wide(
                BACK,
                w / 2 - 100,
                h / 4 + 24 + i,
                ctx.tr("menu.returnToGame"),
            ),
            Button::new(
                OPTIONS,
                w / 2 - 100,
                h / 4 + 96 + i,
                98,
                20,
                ctx.tr("menu.options"),
            ),
            Button::new(
                LAN,
                w / 2 + 2,
                h / 4 + 96 + i,
                98,
                20,
                ctx.tr("menu.shareToLan"),
            ),
            Button::new(
                ACHIEVEMENTS,
                w / 2 - 100,
                h / 4 + 48 + i,
                98,
                20,
                ctx.tr("gui.achievements"),
            ),
            Button::new(
                STATS,
                w / 2 + 2,
                h / 4 + 48 + i,
                98,
                20,
                ctx.tr("gui.stats"),
            ),
        ];
        // shortcut: no statistics screen yet, and LAN needs singleplayer; their buttons stay disabled.
        for b in self
            .buttons
            .iter_mut()
            .filter(|b| matches!(b.id, LAN | STATS))
        {
            b.enabled = false;
        }
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        centered(c, &ctx.tr("menu.game"), ctx.width / 2, 40, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx)? {
            OPTIONS => Some(Nav::Child(Box::new(|me| Box::new(Options::new(me))))),
            DISCONNECT => {
                ctx.effects.push(Effect::Disconnect);
                ctx.in_world = false;
                Some(Nav::Open(Box::new(Multiplayer::new(Box::new(Title::new(
                    ctx,
                ))))))
            }
            ACHIEVEMENTS => Some(Nav::Child(Box::new(|me| Box::new(Achievements::new(me))))),
            BACK => Some(Nav::Game),
            _ => None,
        }
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }
}
