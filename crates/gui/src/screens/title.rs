//! `GuiMainMenu`.

use super::{
    blit256, centered, draw_buttons, press, release_all, text, Button, Canvas, Ctx, Effect, Mods,
    Multiplayer, Nav, Options, Screen,
};

const LOGO: &str = "textures/gui/title/minecraft.png";
/// shortcut: the first panorama face drawn flat, not the spinning blurred cube; port `renderSkybox` for full parity.
const PANORAMA: &str = "textures/gui/title/background/panorama_0.png";

const OPTIONS: i32 = 0;
const SINGLEPLAYER: i32 = 1;
const MULTIPLAYER: i32 = 2;
const QUIT: i32 = 4;
const LANGUAGE: i32 = 5;
const REALMS: i32 = 14;

pub struct Title {
    splash: String,
    buttons: Vec<Button>,
}

impl Title {
    /// shortcut: no date splashes (X-mas, new year, Halloween); add them with a local-date source.
    pub fn new(ctx: &Ctx) -> Title {
        let s = &ctx.texts.splashes;
        let splash = if s.is_empty() {
            "missingno".to_owned()
        } else {
            s[(ctx.now_ms as usize) % s.len()].clone()
        };
        Title {
            splash,
            buttons: Vec::new(),
        }
    }
}

impl Screen for Title {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let j = h / 4 + 48;
        let mut b = vec![
            Button::wide(SINGLEPLAYER, w / 2 - 100, j, ctx.tr("menu.singleplayer")),
            Button::wide(MULTIPLAYER, w / 2 - 100, j + 24, ctx.tr("menu.multiplayer")),
            Button::wide(REALMS, w / 2 - 100, j + 48, ctx.tr("menu.online")),
            Button::new(
                OPTIONS,
                w / 2 - 100,
                j + 72 + 12,
                98,
                20,
                ctx.tr("menu.options"),
            ),
            Button::new(QUIT, w / 2 + 2, j + 72 + 12, 98, 20, ctx.tr("menu.quit")),
            Button {
                globe: true,
                ..Button::new(LANGUAGE, w / 2 - 124, j + 72 + 12, 20, 20, "")
            },
        ];
        // The client has no integrated server, Realms or language screen.
        for x in b
            .iter_mut()
            .filter(|x| matches!(x.id, SINGLEPLAYER | REALMS | LANGUAGE))
        {
            x.enabled = false;
        }
        self.buttons = std::mem::take(&mut b);
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        c.blit(
            PANORAMA,
            0.0,
            0.0,
            w as f32,
            h as f32,
            [0.0, 0.0, 1.0, 1.0],
            0xffffffff,
        );
        c.gradient(0, 0, w, h, 0x80ffffff, 0x00ffffff);
        c.gradient(0, 0, w, h, 0x00000000, 0x80000000);
        let l = w / 2 - 274 / 2;
        blit256(c, LOGO, l, 30, 0, 0, 155, 44);
        blit256(c, LOGO, l + 155, 30, 0, 45, 155, 44);
        let t = (ctx.now_ms % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
        let f = (1.8 - (t.sin() * 0.1).abs()) * 100.0 / (c.width(&self.splash) + 32) as f32;
        c.push([(w / 2 + 90) as f32, 70.0], f, -20.0);
        centered(c, &self.splash, 0, -8, 0xffffff00);
        c.pop();
        text(c, "Minecraft 1.8.9", 2, h - 10, 0xffffffff);
        let copy = "Copyright Mojang AB. Do not distribute!";
        let cw = c.width(copy);
        text(c, copy, w - cw - 2, h - 10, 0xffffffff);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    /// `GuiMainMenu.keyTyped` ignores every key, Escape too.
    fn key(&mut self, _code: i32, _ch: char, _mods: Mods, _ctx: &mut Ctx) -> Option<Nav> {
        None
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx)? {
            OPTIONS => Some(Nav::Child(Box::new(|me| Box::new(Options::new(me))))),
            MULTIPLAYER => Some(Nav::Child(Box::new(|me| Box::new(Multiplayer::new(me))))),
            QUIT => {
                ctx.effects.push(Effect::Quit);
                None
            }
            _ => None,
        }
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.buttons);
    }

    fn pauses(&self) -> bool {
        false
    }
}
