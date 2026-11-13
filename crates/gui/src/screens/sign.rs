//! `GuiEditSign`: the four sign lines over a front view of the sign being edited.

use super::{
    centered, default_background, draw_buttons, key, press, release_all, Button, Canvas, Ctx,
    Effect, Metrics, Mods, Nav, Screen, WHITE,
};

const DONE: i32 = 0;
const TEXTURE: &str = "textures/entity/sign.png";
/// `wall_sign`; any other block is drawn as the `standing_sign`, with its post.
const WALL_SIGN: u16 = 68;
/// Widest line `keyTyped` accepts, in font units.
const MAX_WIDTH: i32 = 90;
/// GUI units per model pixel: the GUI scale 93.75 times `ModelSign`'s 0.6666667 and 1/16.
const UNIT: f32 = 93.75 * 0.666_666_7 / 16.0;
/// GUI units per font unit of a sign line: 93.75 times `0.015625 * 0.6666667`.
const TEXT_SCALE: f32 = 93.75 * 0.015_625 * 0.666_666_7;

pub struct EditSign {
    pos: [i32; 3],
    wall: bool,
    lines: [String; 4],
    /// `editLine`.
    line: usize,
    /// `updateCounter`.
    counter: i32,
    done: Option<Button>,
}

impl EditSign {
    /// `state` is the block state at `pos`; the sign text starts empty (the client keeps no sign text).
    pub fn new(pos: [i32; 3], state: u16) -> EditSign {
        EditSign {
            pos,
            wall: state >> 4 == WALL_SIGN,
            lines: Default::default(),
            line: 0,
            counter: 0,
            done: None,
        }
    }

    /// `GuiEditSign.keyTyped` without the Escape key: up, down and Enter pick the line,
    /// Backspace and typed characters edit it.
    fn edit(&mut self, code: i32, ch: char, m: &dyn Metrics) {
        if code == key::UP {
            self.line = (self.line + 3) & 3;
        }
        if matches!(code, key::DOWN | key::RETURN | key::NUMPAD_ENTER) {
            self.line = (self.line + 1) & 3;
        }
        let text = &mut self.lines[self.line];
        if code == key::BACK {
            text.pop();
        }
        // `ChatAllowedCharacters.isAllowedCharacter`
        if ch >= ' ' && ch != '\u{a7}' && ch != '\u{7f}' {
            let mut longer = text.clone();
            longer.push(ch);
            if m.width(&longer) <= MAX_WIDTH {
                *text = longer;
            }
        }
    }

    /// The sign as `TileEntitySignRenderer` draws it in the GUI: a front view, no depth.
    fn draw_sign(&self, c: &mut dyn Canvas, cx: f32) {
        // Origin of the model: the GUI's translate plus the renderer's, in GUI units.
        let origin = if self.wall {
            (0.7625 + 0.25 + 0.3125) * 93.75
        } else {
            (1.0625 + 0.25) * 93.75
        };
        let blit = |c: &mut dyn Canvas, x: i32, y: i32, w: i32, h: i32, u: i32, v: i32| {
            let uv = [
                u as f32 / 64.0,
                v as f32 / 32.0,
                (u + w) as f32 / 64.0,
                (v + h) as f32 / 32.0,
            ];
            let rect = [
                cx + x as f32 * UNIT,
                origin + y as f32 * UNIT,
                w as f32 * UNIT,
                h as f32 * UNIT,
            ];
            c.blit(TEXTURE, rect[0], rect[1], rect[2], rect[3], uv, 0xffff_ffff);
        };
        // Front faces of the board (`-12,-14,-1` 24x12x2 at 0,0) and the post (`-1,-2,-1` 2x14x2 at 0,14).
        blit(c, -12, -14, 24, 12, 2, 2);
        if !self.wall {
            blit(c, -1, -2, 2, 14, 2, 16);
        }
        let blink = self.counter / 6 % 2 == 0;
        c.push([cx, origin - 8.0 * UNIT], TEXT_SCALE, 0.0);
        for (i, line) in self.lines.iter().enumerate() {
            let s = if blink && i == self.line {
                format!("> {line} <")
            } else {
                line.clone()
            };
            let x = -(c.width(&s) / 2) as f32;
            c.text(&s, x, (i as i32 * 10 - 20) as f32, 0, false);
        }
        c.pop();
    }
}

impl Screen for EditSign {
    fn init(&mut self, ctx: &mut Ctx) {
        self.done = Some(Button::wide(
            DONE,
            ctx.width / 2 - 100,
            ctx.height / 4 + 120,
            ctx.tr("gui.done"),
        ));
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        centered(c, &ctx.tr("sign.edit"), ctx.width / 2, 40, WHITE);
        self.draw_sign(c, ctx.width as f32 / 2.0);
        if let Some(done) = &mut self.done {
            draw_buttons(std::slice::from_mut(done), c, mx, my);
        }
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        self.counter += 1;
        None
    }

    fn key(&mut self, code: i32, ch: char, _mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        self.edit(code, ch, ctx.metrics);
        (code == key::ESCAPE).then_some(Nav::Game)
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        let done = self.done.as_mut()?;
        press(std::slice::from_mut(done), x, y, mb, ctx).map(|_| Nav::Game)
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        if let Some(done) = &mut self.done {
            release_all(std::slice::from_mut(done));
        }
    }

    /// `onGuiClosed`: `C12UpdateSign` with the four lines, on Done, Escape or any other close.
    fn closed(&mut self, ctx: &mut Ctx) {
        ctx.effects.push(Effect::SignText {
            pos: self.pos,
            lines: self.lines.clone(),
        });
    }

    fn pauses(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every char is 6 units wide.
    struct Mono;
    impl Metrics for Mono {
        fn width(&self, s: &str) -> i32 {
            s.chars().count() as i32 * 6
        }
    }

    fn typed(s: &mut EditSign, text: &str) {
        for ch in text.chars() {
            s.edit(0, ch, &Mono);
        }
    }

    #[test]
    fn lines_edit_like_vanilla() {
        let mut s = EditSign::new([1, 64, 2], 63 << 4);
        typed(&mut s, "0123456789abcdefg");
        assert_eq!(s.lines[0], "0123456789abcde", "90 units fit 15 chars");
        s.edit(key::BACK, '\u{8}', &Mono);
        assert_eq!(s.lines[0], "0123456789abcd");
        s.edit(key::RETURN, '\r', &Mono);
        typed(&mut s, "\u{a7}x");
        assert_eq!((s.line, s.lines[1].as_str()), (1, "x"));
        s.edit(key::UP, '\0', &Mono);
        s.edit(key::UP, '\0', &Mono);
        assert_eq!(s.line, 3, "up wraps from the first line");
        s.edit(key::DOWN, '\0', &Mono);
        assert_eq!(s.line, 0);
    }
}
