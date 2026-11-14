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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::{Metrics, Settings, Texts};
    use std::path::PathBuf;

    struct M;
    impl Metrics for M {
        fn width(&self, s: &str) -> i32 {
            s.len() as i32 * 6
        }
    }

    /// Vanilla `GuiYesNo` layout: Yes at the left, No at the right; a click answers once.
    fn click_at(x: i32) -> (Option<Nav>, Vec<Effect>) {
        let texts = Texts::parse("gui.yes=Yes\ngui.no=No\n", "");
        let mut settings = Settings::new(PathBuf::from("/tmp"));
        let mut clip = String::new();
        let stats = crate::screens::PlayerStats::default();
        let mut ctx = Ctx {
            width: 320,
            height: 240,
            texts: &texts,
            settings: &mut settings,
            in_world: true,
            difficulty: None,
            hardcore: false,
            now_ms: 0,
            game_dir: PathBuf::from("/tmp"),
            effects: Vec::new(),
            clipboard: &mut clip,
            metrics: &M,
            shift: false,
            stats: &stats,
            icons: None,
            player_id: "",
        };
        let mut s = PackPrompt::new("http://h/p.zip".into(), "abc".into());
        s.init(&mut ctx);
        let nav = s.click(x, 240 / 6 + 96 + 5, 0, &mut ctx);
        (nav, ctx.effects)
    }

    #[test]
    fn yes_accepts_and_no_declines() {
        for (x, want) in [(320 / 2 - 100, true), (320 / 2 + 60, false)] {
            let (nav, fx) = click_at(x);
            assert!(nav.is_some());
            let answers: Vec<bool> = fx
                .iter()
                .filter_map(|e| match e {
                    Effect::ServerPackAnswer {
                        url,
                        hash,
                        accepted,
                    } => {
                        assert_eq!((url.as_str(), hash.as_str()), ("http://h/p.zip", "abc"));
                        Some(*accepted)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(answers, vec![want]);
        }
    }

    #[test]
    fn click_outside_buttons_gives_no_answer() {
        let (nav, fx) = click_at(1);
        assert!(nav.is_none());
        assert!(!fx
            .iter()
            .any(|e| matches!(e, Effect::ServerPackAnswer { .. })));
    }
}
