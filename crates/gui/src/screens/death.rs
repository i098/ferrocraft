//! `GuiGameOver`.

use super::{
    centered, draw_buttons, press, release_all, Button, Canvas, Ctx, Effect, Mods, Nav, Screen,
    Title, YesNo, WHITE,
};

const RESPAWN: i32 = 0;
const TITLE: i32 = 1;

pub struct Death {
    /// The death message from the combat packet, already formatted.
    cause: Option<String>,
    score: i32,
    buttons: Vec<Button>,
    /// `enableButtonsTimer`.
    timer: i32,
}

impl Death {
    pub fn new(cause: Option<String>, score: i32) -> Death {
        Death {
            cause,
            score,
            buttons: Vec::new(),
            timer: 0,
        }
    }

    fn to_title(ctx: &mut Ctx) -> Nav {
        ctx.effects.push(Effect::Disconnect);
        ctx.in_world = false;
        Nav::Open(Box::new(Title::new(ctx)))
    }
}

impl Screen for Death {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = if ctx.hardcore {
            vec![Button::wide(
                TITLE,
                w / 2 - 100,
                h / 4 + 96,
                ctx.tr("deathScreen.leaveServer"),
            )]
        } else {
            vec![
                Button::wide(
                    RESPAWN,
                    w / 2 - 100,
                    h / 4 + 72,
                    ctx.tr("deathScreen.respawn"),
                ),
                Button::wide(
                    TITLE,
                    w / 2 - 100,
                    h / 4 + 96,
                    ctx.tr("deathScreen.titleScreen"),
                ),
            ]
        };
        self.buttons.iter_mut().for_each(|b| b.enabled = false);
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        c.gradient(0, 0, w, h, 0x60500000, 0xa0803030);
        c.push([0.0, 0.0], 2.0, 0.0);
        let title = if ctx.hardcore {
            "deathScreen.title.hardcore"
        } else {
            "deathScreen.title"
        };
        centered(c, &ctx.tr(title), w / 2 / 2, 30, WHITE);
        c.pop();
        if let Some(cause) = &self.cause {
            centered(c, cause, w / 2, 85, WHITE);
        }
        let score = format!("{}: §e{}", ctx.tr("deathScreen.score"), self.score);
        centered(c, &score, w / 2, 100, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        self.timer += 1;
        if self.timer == 20 {
            self.buttons.iter_mut().for_each(|b| b.enabled = true);
        }
        None
    }

    /// `GuiGameOver.keyTyped` ignores every key.
    fn key(&mut self, _code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx)? {
            RESPAWN => {
                ctx.effects.push(Effect::Respawn);
                Some(Nav::Game)
            }
            _ if ctx.hardcore => Some(Self::to_title(ctx)),
            _ => {
                let (q, t, r) = (
                    ctx.tr("deathScreen.quit.confirm"),
                    ctx.tr("deathScreen.titleScreen"),
                    ctx.tr("deathScreen.respawn"),
                );
                Some(Nav::Child(Box::new(move |me| {
                    Box::new(YesNo::new(me, q, String::new(), t, r, 0).with_delay(20))
                })))
            }
        }
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }

    fn confirm(&mut self, yes: bool, _id: i32, ctx: &mut Ctx) -> Option<Nav> {
        Some(if yes {
            Self::to_title(ctx)
        } else {
            ctx.effects.push(Effect::Respawn);
            Nav::Game
        })
    }

    fn pauses(&self) -> bool {
        false
    }
}
