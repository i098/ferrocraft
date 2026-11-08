//! HUD layout: turns a [`Hud`](crate::Hud) into screen-space triangles (pixels, origin top-left).

use crate::assets::Font;
use crate::Hud;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct HudVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

const CHAT_WIDTH: f32 = 320.0;
const CHAT_LINES: usize = 10;
const LINE_HEIGHT: f32 = 9.0;

/// Vanilla `Minecraft.func_71...` auto GUI scale: the largest factor up to 4 that keeps 320x240 visible.
pub fn gui_scale(width: u32, height: u32) -> f32 {
    let mut scale = 1;
    while scale < 4 && width / (scale + 1) >= 320 && height / (scale + 1) >= 240 {
        scale += 1;
    }
    scale as f32
}

const COLORS: [u32; 16] = [
    0x000000, 0x0000aa, 0x00aa00, 0x00aaaa, 0xaa0000, 0xaa00aa, 0xffaa00, 0xaaaaaa, 0x555555,
    0x5555ff, 0x55ff55, 0x55ffff, 0xff5555, 0xff55ff, 0xffff55, 0xffffff,
];

pub struct Painter<'a> {
    pub out: Vec<HudVertex>,
    font: Option<&'a Font>,
    /// Texture size in texels: the font image plus one white row below it.
    tex: (f32, f32),
    scale: f32,
}

impl<'a> Painter<'a> {
    pub fn new(font: Option<&'a Font>, scale: f32) -> Self {
        let side = font.map_or(128.0, |f| f.image.width as f32);
        Painter {
            out: Vec::new(),
            font,
            tex: (side, side + 1.0),
            scale,
        }
    }

    fn quad(&mut self, p: [f32; 4], uv: [f32; 4], color: [f32; 4]) {
        let v = |x, y, u, vv| HudVertex {
            pos: [x, y],
            uv: [u, vv],
            color,
        };
        let (a, b, c, d) = (
            v(p[0], p[1], uv[0], uv[1]),
            v(p[2], p[1], uv[2], uv[1]),
            v(p[2], p[3], uv[2], uv[3]),
            v(p[0], p[3], uv[0], uv[3]),
        );
        self.out.extend_from_slice(&[a, b, c, a, c, d]);
    }

    /// Filled rectangle in GUI units.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let s = self.scale;
        let (u, v) = (0.5 / self.tex.0, (self.tex.1 - 0.5) / self.tex.1);
        self.quad(
            [x * s, y * s, (x + w) * s, (y + h) * s],
            [u, v, u, v],
            color,
        );
    }

    fn char_code(c: char) -> u8 {
        if c.is_ascii() && !c.is_ascii_control() {
            c as u8
        } else {
            b'?'
        }
    }

    /// Pixel advance of `text` in GUI units, skipping `§` formatting codes.
    pub fn text_width(&self, text: &str) -> f32 {
        let Some(font) = self.font else { return 0.0 };
        let mut chars = text.chars();
        let mut w = 0;
        while let Some(c) = chars.next() {
            if c == '§' {
                chars.next();
            } else {
                w += font.advance(Self::char_code(c));
            }
        }
        w as f32
    }

    /// Draw `text` at GUI position `(x, y)` with the vanilla drop shadow. Without a font nothing is drawn.
    pub fn text(&mut self, text: &str, x: f32, y: f32, color: u32) {
        if self.font.is_none() {
            return;
        }
        self.run(text, x + 1.0, y + 1.0, color, true);
        self.run(text, x, y, color, false);
    }

    fn run(&mut self, text: &str, x: f32, y: f32, base: u32, shadow: bool) {
        let Some(font) = self.font else { return };
        let mut color = base;
        let mut x = x;
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '§' {
                let code = chars.next().and_then(|d| d.to_digit(16));
                color = code.map_or(base, |i| COLORS[i as usize]);
                continue;
            }
            let code = Self::char_code(c);
            let gw = font.glyph_width(code) as f32;
            let (col, row) = ((code % 16) as f32, (code / 16) as f32);
            let rgb = if shadow {
                (color & 0xfcfcfc) >> 2
            } else {
                color
            };
            let tint = [
                (rgb >> 16 & 255) as f32 / 255.0,
                (rgb >> 8 & 255) as f32 / 255.0,
                (rgb & 255) as f32 / 255.0,
                1.0,
            ];
            let side = self.tex.0;
            let cell = side / 16.0;
            let (u0, v0) = (col * cell / side, row * cell / self.tex.1);
            let (u1, v1) = (u0 + gw / 8.0 * cell / side, v0 + cell / self.tex.1);
            let s = self.scale;
            self.quad(
                [x * s, y * s, (x + gw) * s, (y + 8.0) * s],
                [u0, v0, u1, v1],
                tint,
            );
            x += font.advance(code) as f32;
        }
    }

    /// Split `text` into lines of at most `max` GUI units, breaking inside words only when one is too long.
    fn wrap(&self, text: &str, max: f32) -> Vec<String> {
        let mut lines = vec![String::new()];
        for word in text.split_inclusive(' ') {
            let cur = lines.last().expect("lines starts non-empty");
            if self.text_width(&format!("{cur}{word}")) <= max
                || cur.is_empty() && self.text_width(word) <= max
            {
                lines.last_mut().expect("non-empty").push_str(word);
                continue;
            }
            if !cur.is_empty() {
                lines.push(String::new());
            }
            for c in word.chars() {
                let cur = lines.last().expect("non-empty");
                if self.text_width(&format!("{cur}{c}")) > max && !cur.is_empty() {
                    lines.push(String::new());
                }
                lines.last_mut().expect("non-empty").push(c);
            }
        }
        lines
    }
}

/// Lay out the whole HUD for a `width` x `height` pixel target.
pub fn layout(hud: &Hud, font: Option<&Font>, width: u32, height: u32) -> Vec<HudVertex> {
    let scale = gui_scale(width, height);
    let (gw, gh) = (width as f32 / scale, height as f32 / scale);
    let mut p = Painter::new(font, scale);

    if hud.crosshair {
        let (cx, cy) = ((gw / 2.0).floor(), (gh / 2.0).floor());
        let white = [1.0, 1.0, 1.0, 0.85];
        p.rect(cx - 5.0, cy, 11.0, 1.0, white);
        p.rect(cx, cy - 5.0, 1.0, 5.0, white);
        p.rect(cx, cy + 1.0, 1.0, 5.0, white);
    }

    for (i, line) in hud.debug.iter().enumerate() {
        let y = 2.0 + i as f32 * 10.0;
        p.rect(
            1.0,
            y - 1.0,
            p.text_width(line) + 2.0,
            9.0,
            [0.0, 0.0, 0.0, 0.31],
        );
        p.text(line, 2.0, y, 0xe0e0e0);
    }

    let mut rows: Vec<String> = hud
        .chat
        .iter()
        .flat_map(|l| p.wrap(l, CHAT_WIDTH))
        .collect();
    let skip = rows.len().saturating_sub(CHAT_LINES);
    rows.drain(..skip);
    let bottom = gh - 8.0;
    for (i, row) in rows.iter().rev().enumerate() {
        let y = bottom - (i as f32 + 1.0) * LINE_HEIGHT - 31.0 + 8.0;
        p.rect(
            2.0,
            y - 1.0,
            CHAT_WIDTH + 4.0,
            LINE_HEIGHT,
            [0.0, 0.0, 0.0, 0.5],
        );
        p.text(row, 2.0, y, 0xffffff);
    }

    if let Some(input) = &hud.chat_input {
        p.rect(2.0, gh - 14.0, gw - 4.0, 12.0, [0.0, 0.0, 0.0, 0.5]);
        p.text(&format!("{input}_"), 4.0, gh - 12.0, 0xe0e0e0);
    }
    p.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_scale_follows_vanilla_auto() {
        assert_eq!(gui_scale(500, 300), 1.0);
        assert_eq!(gui_scale(854, 480), 2.0);
        assert_eq!(gui_scale(1280, 720), 3.0);
        assert_eq!(gui_scale(1920, 1080), 4.0);
    }

    #[test]
    fn crosshair_without_font_still_draws_rects() {
        let hud = Hud {
            crosshair: true,
            ..Hud::default()
        };
        assert_eq!(layout(&hud, None, 854, 480).len(), 3 * 6);
    }
}
