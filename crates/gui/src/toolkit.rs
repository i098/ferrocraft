//! Vanilla `Gui`, `FontRenderer`, `ScaledResolution` and `GuiButton`, drawing into a list of
//! [`render::Quad`]s. Coordinates are GUI units (scaled pixels, origin top-left).

use render::{Blend, Font, Quad};
use std::borrow::Cow;
use std::sync::LazyLock;

pub const ICONS: &str = "textures/gui/icons.png";
pub const WIDGETS: &str = "textures/gui/widgets.png";
const ASCII: &str = "textures/font/ascii.png";
pub const FONT_HEIGHT: i32 = 9;

/// `ScaledResolution`: the largest factor (up to `gui_scale`, 0 = auto) that keeps 320x240 GUI
/// units; an odd factor above 1 drops by one when the unicode font is forced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuiScale {
    pub factor: i32,
    pub width: i32,
    pub height: i32,
}

impl GuiScale {
    pub fn new(px_w: u32, px_h: u32, gui_scale: u32, unicode: bool) -> GuiScale {
        let (w, h) = (px_w as i32, px_h as i32);
        let max = if gui_scale == 0 {
            1000
        } else {
            gui_scale as i32
        };
        let mut factor = 1;
        while factor < max && w / (factor + 1) >= 320 && h / (factor + 1) >= 240 {
            factor += 1;
        }
        if unicode && factor % 2 != 0 && factor != 1 {
            factor -= 1;
        }
        GuiScale {
            factor,
            width: (w as f64 / factor as f64).ceil() as i32,
            height: (h as f64 / factor as f64).ceil() as i32,
        }
    }
}

/// `"\u00c0...".indexOf(c)` of `FontRenderer`: the `ascii.png` cell of each glyph.
const GLYPHS: [&str; 16] = [
    "\u{c0}\u{c1}\u{c2}\u{c8}\u{ca}\u{cb}\u{cd}\u{d3}\u{d4}\u{d5}\u{da}\u{df}\u{e3}\u{f5}\u{11f}\u{130}",
    "\u{131}\u{152}\u{153}\u{15e}\u{15f}\u{174}\u{175}\u{17e}\u{207}\0\0\0\0\0\0\0",
    " !\"#$%&'()*+,-./",
    "0123456789:;<=>?",
    "@ABCDEFGHIJKLMNO",
    "PQRSTUVWXYZ[\\]^_",
    "`abcdefghijklmno",
    "pqrstuvwxyz{|}~\0",
    "\u{c7}\u{fc}\u{e9}\u{e2}\u{e4}\u{e0}\u{e5}\u{e7}\u{ea}\u{eb}\u{e8}\u{ef}\u{ee}\u{ec}\u{c4}\u{c5}",
    "\u{c9}\u{e6}\u{c6}\u{f4}\u{f6}\u{f2}\u{fb}\u{f9}\u{ff}\u{d6}\u{dc}\u{f8}\u{a3}\u{d8}\u{d7}\u{192}",
    "\u{e1}\u{ed}\u{f3}\u{fa}\u{f1}\u{d1}\u{aa}\u{ba}\u{bf}\u{ae}\u{ac}\u{bd}\u{bc}\u{a1}\u{ab}\u{bb}",
    "\u{2591}\u{2592}\u{2593}\u{2502}\u{2524}\u{2561}\u{2562}\u{2556}\u{2555}\u{2563}\u{2551}\u{2557}\u{255d}\u{255c}\u{255b}\u{2510}",
    "\u{2514}\u{2534}\u{252c}\u{251c}\u{2500}\u{253c}\u{255e}\u{255f}\u{255a}\u{2554}\u{2569}\u{2566}\u{2560}\u{2550}\u{256c}\u{2567}",
    "\u{2568}\u{2564}\u{2565}\u{2559}\u{2558}\u{2552}\u{2553}\u{256b}\u{256a}\u{2518}\u{250c}\u{2588}\u{2584}\u{258c}\u{2590}\u{2580}",
    "\u{3b1}\u{3b2}\u{393}\u{3c0}\u{3a3}\u{3c3}\u{3bc}\u{3c4}\u{3a6}\u{398}\u{3a9}\u{3b4}\u{221e}\u{2205}\u{2208}\u{2229}",
    "\u{2261}\u{b1}\u{2265}\u{2264}\u{2320}\u{2321}\u{f7}\u{2248}\u{b0}\u{2219}\u{b7}\u{221a}\u{207f}\u{b2}\u{25a0}\0",
];

static GLYPH_CHARS: LazyLock<Vec<char>> = LazyLock::new(|| GLYPHS.concat().chars().collect());

static UNICODE_PAGES: LazyLock<Vec<String>> = LazyLock::new(|| {
    (0..256)
        .map(|p| format!("textures/font/unicode_page_{p:02x}.png"))
        .collect()
});

fn glyph_index(c: char) -> Option<usize> {
    if (' '..='~').contains(&c) {
        return Some(c as usize);
    }
    GLYPH_CHARS.iter().position(|&g| g == c)
}

/// `FontRenderer.colorCode`: 16 colors, then their shadows at a quarter brightness.
fn color_code(i: usize) -> u32 {
    let i = i as u32;
    let j = (i >> 3 & 1) * 85;
    let mut r = (i >> 2 & 1) * 170 + j;
    let mut g = (i >> 1 & 1) * 170 + j;
    let mut b = (i & 1) * 170 + j;
    if i == 6 {
        r += 85;
    }
    if i >= 16 {
        r /= 4;
        g /= 4;
        b /= 4;
    }
    (r & 255) << 16 | (g & 255) << 8 | (b & 255)
}

const FORMAT_CODES: &str = "0123456789abcdefklmnor";

fn is_format_color(c: char) -> bool {
    c.is_ascii_digit() || ('a'..='f').contains(&c) || ('A'..='F').contains(&c)
}

/// Bold state after `§<code>` in `sizeStringToWidth`: `l` sets it, `r` and colors clear it.
fn bold_after(code: char, bold: bool) -> bool {
    match code {
        'l' | 'L' => true,
        'r' | 'R' => false,
        c => bold && !is_format_color(c),
    }
}

fn is_format_special(c: char) -> bool {
    ('k'..='o').contains(&c) || ('K'..='O').contains(&c) || c == 'r' || c == 'R'
}

/// `FontRenderer` measurements; without a font every glyph is 0 wide.
#[derive(Clone, Copy)]
pub struct Metrics<'f>(pub Option<&'f Font>);

impl Metrics<'_> {
    /// `getCharWidth`: -1 for `§`.
    pub fn char_width(&self, c: char) -> i32 {
        if c == '\u{a7}' {
            return -1;
        }
        if c == ' ' {
            return 4;
        }
        let Some(font) = self.0 else { return 0 };
        if let Some(i) = glyph_index(c).filter(|_| c != '\0' && !font.force_unicode) {
            return font.ascii[i] as i32;
        }
        let g = font.unicode.get(c as usize).copied().unwrap_or(0);
        if g == 0 {
            return 0;
        }
        let (mut j, mut k) = ((g >> 4) as i32, (g & 15) as i32);
        if k > 7 {
            k = 15;
            j = 0;
        }
        k += 1;
        (k - j) / 2 + 1
    }

    /// `getStringWidth`: format codes are free, bold adds 1 per glyph.
    pub fn width(&self, text: &str) -> i32 {
        let mut total = 0;
        let mut bold = false;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            let mut k = self.char_width(c);
            if k < 0 {
                if let Some(code) = chars.next() {
                    match code {
                        'l' | 'L' => bold = true,
                        'r' | 'R' => bold = false,
                        _ => {}
                    }
                    k = 0;
                }
            }
            total += k;
            if bold && k > 0 {
                total += 1;
            }
        }
        total
    }

    /// `trimStringToWidth`: the longest prefix (or suffix when `reverse`) that fits `width`.
    pub fn trim(&self, text: &str, width: i32, reverse: bool) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut kept: Vec<char> = Vec::new();
        let (mut i, mut code, mut bold) = (0, false, false);
        let order: Box<dyn Iterator<Item = &char>> = if reverse {
            Box::new(chars.iter().rev())
        } else {
            Box::new(chars.iter())
        };
        for &c in order {
            if i >= width {
                break;
            }
            i += self.trim_step(c, &mut code, &mut bold);
            if i > width {
                break;
            }
            kept.push(c);
        }
        if reverse {
            kept.reverse();
        }
        kept.into_iter().collect()
    }

    fn trim_step(&self, c: char, code: &mut bool, bold: &mut bool) -> i32 {
        let w = self.char_width(c);
        if *code {
            *code = false;
            match c {
                'l' | 'L' => *bold = true,
                'r' | 'R' => *bold = false,
                _ => {}
            }
            0
        } else if w < 0 {
            *code = true;
            0
        } else {
            w + i32::from(*bold)
        }
    }

    /// `listFormattedStringToWidth`: word-wrapped lines that carry the active format codes over.
    pub fn wrap(&self, text: &str, width: i32) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest: Vec<char> = text.chars().collect();
        loop {
            let i = self.size_to_width(&rest, width);
            if rest.len() <= i {
                out.push(rest.iter().collect());
                break;
            }
            let head: String = rest[..i].iter().collect();
            let skip = usize::from(rest[i] == ' ' || rest[i] == '\n');
            let mut next: Vec<char> = format_from(&head).chars().collect();
            next.extend_from_slice(&rest[i + skip..]);
            out.push(head);
            rest = next;
        }
        // `String.split("\n")` drops trailing empty strings.
        while out.len() > 1 && out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        out
    }

    /// `sizeStringToWidth`: how many chars of `s` fit `width`, breaking at the last space.
    fn size_to_width(&self, s: &[char], width: i32) -> usize {
        let len = s.len();
        let (mut j, mut k, mut space, mut bold) = (0, 0usize, None, false);
        while k < len {
            let c = s[k];
            if c == '\n' {
                space = Some(k);
                break;
            }
            if c != '\u{a7}' {
                if c == ' ' {
                    space = Some(k);
                }
                j += self.char_width(c) + i32::from(bold);
            } else if k < len - 1 {
                k += 1;
                bold = bold_after(s[k], bold);
            }
            if j > width {
                break;
            }
            k += 1;
        }
        match space {
            Some(l) if k != len && l < k => l,
            _ => k,
        }
    }
}

/// `FontRenderer.getFormatFromString`: the last color code and the specials after it.
pub fn format_from(text: &str) -> String {
    let mut s = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{a7}' {
            continue;
        }
        match chars.peek().copied() {
            Some(n) if is_format_color(n) => {
                s.clear();
                s.push('\u{a7}');
                s.push(n);
            }
            Some(n) if is_format_special(n) => {
                s.push('\u{a7}');
                s.push(n);
            }
            _ => {}
        }
    }
    s
}

/// `EnumChatFormatting.getTextWithoutFormattingCodes`.
pub fn strip_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            if let Some(n) = chars.clone().next() {
                if FORMAT_CODES.contains(n.to_ascii_lowercase()) {
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Row-major 2D affine transform `[a, b, c, d, e, f]`: `x' = a x + c y + e`, `y' = b x + d y + f`.
type Affine = [f32; 6];

fn apply(m: &Affine, [x, y]: [f32; 2]) -> [f32; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

/// Corners, texture coordinates and colors of one quad.
type Clipped = ([[f32; 2]; 4], [[f32; 2]; 4], [u32; 4]);

/// The part `(s0, s1)` of the edge `p0 -> p1` (as fractions) that lies in `lo..hi`; `None` when none does.
fn span(p0: f32, p1: f32, lo: f32, hi: f32) -> Option<(f32, f32)> {
    if p0 == p1 {
        return (lo <= p0 && p0 <= hi).then_some((0.0, 1.0));
    }
    let (a, b) = ((lo - p0) / (p1 - p0), (hi - p0) / (p1 - p0));
    let (s0, s1) = (a.min(b).max(0.0), a.max(b).min(1.0));
    (s0 < s1).then_some((s0, s1))
}

#[derive(Clone, Copy, Default)]
struct TextStyle {
    random: bool,
    bold: bool,
    strike: bool,
    underline: bool,
    italic: bool,
}

impl TextStyle {
    /// One `§` code of `renderStringAtPos`: the new color when the code sets one.
    fn apply_code(&mut self, code: char, color: u32, shadow: bool) -> Option<u32> {
        let i = FORMAT_CODES.find(code.to_ascii_lowercase());
        match i {
            Some(16) => self.random = true,
            Some(17) => self.bold = true,
            Some(18) => self.strike = true,
            Some(19) => self.underline = true,
            Some(20) => self.italic = true,
            Some(21) => {
                *self = TextStyle::default();
                return Some(color);
            }
            _ => {
                *self = TextStyle::default();
                let i = i.filter(|&i| i < 16).unwrap_or(15) + if shadow { 16 } else { 0 };
                return Some(color_code(i) | color & 0xFF00_0000);
            }
        }
        None
    }
}

/// A frame's draw list with the GL state vanilla GUI code leans on: a matrix stack, the current
/// color (`GlStateManager.color`) and the blend function.
pub struct Gui<'f> {
    pub scale: GuiScale,
    font: Metrics<'f>,
    quads: Vec<Quad>,
    matrix: Affine,
    stack: Vec<Affine>,
    color: u32,
    blend: Blend,
    /// Window-pixel rectangle `[x0, y0, x1, y1]` that later quads are cut to.
    clip: Option<[f32; 4]>,
    seed: u32,
}

impl<'f> Gui<'f> {
    /// `seed` drives `§k` (obfuscated) glyphs; pass something that changes each frame.
    pub fn new(scale: GuiScale, font: Option<&'f Font>, seed: u32) -> Gui<'f> {
        let s = scale.factor as f32;
        Gui {
            scale,
            font: Metrics(font),
            quads: Vec::new(),
            matrix: [s, 0.0, 0.0, s, 0.0, 0.0],
            stack: Vec::new(),
            color: 0xFFFF_FFFF,
            blend: Blend::Alpha,
            clip: None,
            seed: seed | 1,
        }
    }

    pub fn into_quads(self) -> Vec<Quad> {
        self.quads
    }

    pub fn metrics(&self) -> Metrics<'f> {
        self.font
    }

    pub fn width(&self, text: &str) -> i32 {
        self.font.width(text)
    }

    // --- GL state ---

    pub fn push(&mut self) {
        self.stack.push(self.matrix);
    }

    pub fn pop(&mut self) {
        if let Some(m) = self.stack.pop() {
            self.matrix = m;
        }
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        let m = &mut self.matrix;
        m[4] += m[0] * x + m[2] * y;
        m[5] += m[1] * x + m[3] * y;
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        let m = &mut self.matrix;
        m[0] *= sx;
        m[1] *= sx;
        m[2] *= sy;
        m[3] *= sy;
    }

    /// `GlStateManager.rotate(deg, 0, 0, 1)`.
    pub fn rotate(&mut self, deg: f32) {
        let (s, c) = deg.to_radians().sin_cos();
        let [a, b, cc, d, e, f] = self.matrix;
        self.matrix = [
            a * c + cc * s,
            b * c + d * s,
            cc * c - a * s,
            d * c - b * s,
            e,
            f,
        ];
    }

    /// `GlStateManager.color` as ARGB; textured blits are tinted by it.
    pub fn set_color(&mut self, argb: u32) {
        self.color = argb;
    }

    pub fn set_blend(&mut self, blend: Blend) {
        self.blend = blend;
    }

    /// Cut every later quad to `rect` (`[x0, y0, x1, y1]` in scaled GUI units), as `glScissor` does; `None` lifts it.
    /// Upright quads are trimmed with their texture coordinates and colors; turned quads are only dropped when outside.
    pub fn set_clip(&mut self, rect: Option<[f32; 4]>) {
        let k = self.scale.factor as f32;
        self.clip = rect.map(|r| r.map(|v| v * k));
    }

    fn clipped(&self, pos: [[f32; 2]; 4], uv: [[f32; 2]; 4], c: [u32; 4]) -> Option<Clipped> {
        let Some([cx0, cy0, cx1, cy1]) = self.clip else {
            return Some((pos, uv, c));
        };
        if cx0 >= cx1 || cy0 >= cy1 {
            return None;
        }
        let [a, b, d, e] = pos;
        if !(a[1] == b[1] && b[0] == d[0] && d[1] == e[1] && e[0] == a[0]) {
            let (lo, hi) = pos
                .iter()
                .fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| {
                    (
                        [lo[0].min(p[0]), lo[1].min(p[1])],
                        [hi[0].max(p[0]), hi[1].max(p[1])],
                    )
                });
            let outside = hi[0] <= cx0 || lo[0] >= cx1 || hi[1] <= cy0 || lo[1] >= cy1;
            return (!outside).then_some((pos, uv, c));
        }
        let (s0, s1) = span(a[0], d[0], cx0, cx1)?;
        let (t0, t1) = span(a[1], d[1], cy0, cy1)?;
        let at = |s: f32, t: f32| {
            let w = [(1.0 - s) * (1.0 - t), s * (1.0 - t), s * t, (1.0 - s) * t];
            let mix = |v: [f32; 4]| v[0] * w[0] + v[1] * w[1] + v[2] * w[2] + v[3] * w[3];
            let p = [mix(pos.map(|p| p[0])), mix(pos.map(|p| p[1]))];
            let q = [mix(uv.map(|p| p[0])), mix(uv.map(|p| p[1]))];
            let shade = (0..4).fold(0u32, |acc, i| {
                let ch = mix(c.map(|v| (v >> (24 - 8 * i) & 0xFF) as f32));
                acc | (ch.round() as u32) << (24 - 8 * i)
            });
            (p, q, shade)
        };
        let corners = [at(s0, t0), at(s1, t0), at(s1, t1), at(s0, t1)];
        Some((
            corners.map(|v| v.0),
            corners.map(|v| v.1),
            corners.map(|v| v.2),
        ))
    }

    // --- Gui primitives ---

    /// One textured quad; `pts` and `uv` run top-left, top-right, bottom-right, bottom-left.
    pub fn quad(
        &mut self,
        tex: impl Into<Cow<'static, str>>,
        pts: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        argb: [u32; 4],
    ) {
        let m = self.matrix;
        let Some((pos, uv, color)) = self.clipped(pts.map(|p| apply(&m, p)), uv, argb) else {
            return;
        };
        self.quads.push(Quad {
            tex: tex.into(),
            blend: self.blend,
            pos,
            uv,
            color,
        });
    }

    /// `drawRect`: a solid ARGB rectangle; the corners may come in any order.
    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32) {
        self.gradient(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1), argb, argb);
    }

    /// `drawGradientRect`: `top` color along the top edge, `bottom` along the bottom edge.
    pub fn gradient(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, top: u32, bottom: u32) {
        let (x0, y0, x1, y1) = (x0 as f32, y0 as f32, x1 as f32, y1 as f32);
        self.rect_f("", [x0, y0, x1, y1], [0.0; 4], [top, top, bottom, bottom]);
    }

    fn rect_f(
        &mut self,
        tex: impl Into<Cow<'static, str>>,
        r: [f32; 4],
        uv: [f32; 4],
        c: [u32; 4],
    ) {
        let [x0, y0, x1, y1] = r;
        let [u0, v0, u1, v1] = uv;
        self.quad(
            tex,
            [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            c,
        );
    }

    /// `drawTexturedModalRect`: a `w` x `h` cut at `(u, v)` of a 256x256 texture, tinted by the current color.
    #[allow(clippy::too_many_arguments)] // vanilla `drawTexturedModalRect` signature
    pub fn blit(
        &mut self,
        tex: impl Into<Cow<'static, str>>,
        x: i32,
        y: i32,
        u: i32,
        v: i32,
        w: i32,
        h: i32,
    ) {
        let f = 1.0 / 256.0;
        let (u, v, w, h) = (u as f32, v as f32, w as f32, h as f32);
        let (x, y) = (x as f32, y as f32);
        self.rect_f(
            tex,
            [x, y, x + w, y + h],
            [u * f, v * f, (u + w) * f, (v + h) * f],
            [self.color; 4],
        );
    }

    /// A `w` x `h` rectangle with texture coordinates `uv` (u0, v0, u1, v1 in texture sizes, past 1 tiles) and an ARGB tint.
    #[allow(clippy::too_many_arguments)] // `blit` plus a tint
    pub fn blit_tinted(
        &mut self,
        tex: impl Into<Cow<'static, str>>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv: [f32; 4],
        argb: u32,
    ) {
        self.rect_f(tex, [x, y, x + w, y + h], uv, [argb; 4]);
    }

    // --- FontRenderer ---

    /// `drawStringWithShadow`.
    pub fn text(&mut self, s: &str, x: f32, y: f32, argb: u32) -> i32 {
        self.draw_string(s, x, y, argb, true)
    }

    /// `drawString` without a shadow.
    pub fn text_plain(&mut self, s: &str, x: f32, y: f32, argb: u32) -> i32 {
        self.draw_string(s, x, y, argb, false)
    }

    /// `drawCenteredString`: shadowed, centered on `cx`.
    pub fn centered(&mut self, s: &str, cx: i32, y: i32, argb: u32) {
        let w = self.width(s);
        self.text(s, (cx - w / 2) as f32, y as f32, argb);
    }

    /// `FontRenderer.drawString(text, x, y, color, dropShadow)`: the end x of the text.
    pub fn draw_string(&mut self, s: &str, x: f32, y: f32, argb: u32, shadow: bool) -> i32 {
        if shadow {
            let a = self.render_string(s, x + 1.0, y + 1.0, argb, true);
            a.max(self.render_string(s, x, y, argb, false))
        } else {
            self.render_string(s, x, y, argb, false)
        }
    }

    fn next_random(&mut self, n: usize) -> usize {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as usize % n
    }

    fn render_string(&mut self, text: &str, x: f32, y: f32, mut color: u32, shadow: bool) -> i32 {
        if color & 0xFC00_0000 == 0 {
            color |= 0xFF00_0000;
        }
        if shadow {
            color = (color & 0x00FC_FCFC) >> 2 | color & 0xFF00_0000;
        }
        let mut cur = color;
        let mut style = TextStyle::default();
        let mut px = x;
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\u{a7}' {
                if let Some(n) = chars.next() {
                    cur = style.apply_code(n, color, shadow).unwrap_or(cur);
                    continue;
                }
            }
            px += self.render_styled_char(c, style, px, y, cur, shadow) as i32 as f32;
        }
        px as i32
    }

    /// `func_181559_a` (obfuscation swap) plus the bold, strikethrough and underline passes.
    fn render_styled_char(
        &mut self,
        mut c: char,
        style: TextStyle,
        px: f32,
        py: f32,
        cur: u32,
        shadow: bool,
    ) -> f32 {
        let glyph = glyph_index(c);
        if style.random && glyph.is_some() {
            c = self.obfuscated(c);
        }
        let forced = self.font.0.is_some_and(|f| f.force_unicode);
        let step = if forced { 0.5 } else { 1.0 };
        let back = (c == '\0' || glyph.is_none() || forced) && shadow;
        let o = if back { -step } else { 0.0 };
        let mut f = self.render_char(c, style.italic, px + o, py + o, cur);
        if style.bold {
            self.render_char(c, style.italic, px + step + o, py + o, cur);
            f += 1.0;
        }
        if style.strike {
            self.rect_f("", [px, py + 3.0, px + f, py + 4.0], [0.0; 4], [cur; 4]);
        }
        if style.underline {
            self.rect_f(
                "",
                [px - 1.0, py + 8.0, px + f, py + 9.0],
                [0.0; 4],
                [cur; 4],
            );
        }
        f
    }

    /// A random glyph of the same width (`§k`); `c` when none is found in 1000 tries.
    fn obfuscated(&mut self, c: char) -> char {
        let w = self.font.char_width(c);
        for _ in 0..1000 {
            let r = GLYPH_CHARS[self.next_random(GLYPH_CHARS.len())];
            if self.font.char_width(r) == w {
                return r;
            }
        }
        c
    }

    /// `renderChar`: draws one glyph and returns its advance.
    fn render_char(&mut self, c: char, italic: bool, x: f32, y: f32, argb: u32) -> f32 {
        if c == ' ' {
            return 4.0;
        }
        let Some(font) = self.font.0 else { return 0.0 };
        let k = if italic { 1.0 } else { 0.0 };
        if let Some(i) = glyph_index(c).filter(|_| !font.force_unicode) {
            let (u, v) = ((i % 16 * 8) as f32, (i / 16 * 8) as f32);
            let l = font.ascii[i] as f32;
            let f = l - 0.01;
            let t = 1.0 / 128.0;
            self.quad(
                ASCII,
                [
                    [x + k, y],
                    [x + f - 1.0 + k, y],
                    [x + f - 1.0 - k, y + 7.99],
                    [x - k, y + 7.99],
                ],
                [
                    [u * t, v * t],
                    [(u + f - 1.0) * t, v * t],
                    [(u + f - 1.0) * t, (v + 7.99) * t],
                    [u * t, (v + 7.99) * t],
                ],
                [argb; 4],
            );
            return l;
        }
        let code = c as usize;
        let g = font.unicode.get(code).copied().unwrap_or(0);
        if g == 0 {
            return 0.0;
        }
        let (start, end) = ((g >> 4) as f32, ((g & 15) + 1) as f32);
        let u = (code % 16 * 16) as f32 + start;
        let v = ((code & 255) / 16 * 16) as f32;
        let f4 = end - start - 0.02;
        let t = 1.0 / 256.0;
        self.quad(
            UNICODE_PAGES[code / 256].as_str(),
            [
                [x + k, y],
                [x + f4 / 2.0 + k, y],
                [x + f4 / 2.0 - k, y + 7.99],
                [x - k, y + 7.99],
            ],
            [
                [u * t, v * t],
                [(u + f4) * t, v * t],
                [(u + f4) * t, (v + 15.98) * t],
                [u * t, (v + 15.98) * t],
            ],
            [argb; 4],
        );
        (end - start) / 2.0 + 1.0
    }
}

/// `GuiButton`: a 200x20 `widgets.png` button split in two halves to fit `w`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
}

impl Button {
    pub fn new(id: u32, x: i32, y: i32, w: i32, h: i32, label: impl Into<String>) -> Button {
        Button {
            id,
            x,
            y,
            w,
            h,
            label: label.into(),
            enabled: true,
            visible: true,
        }
    }

    pub fn hovered(&self, mx: i32, my: i32) -> bool {
        mx >= self.x && my >= self.y && mx < self.x + self.w && my < self.y + self.h
    }

    /// `mousePressed`: the click lands on an enabled, visible button.
    pub fn pressed(&self, mx: i32, my: i32) -> bool {
        self.enabled && self.visible && self.hovered(mx, my)
    }

    /// `drawButton`.
    pub fn draw(&self, gui: &mut Gui, mx: i32, my: i32) {
        if !self.visible {
            return;
        }
        let hovered = self.hovered(mx, my);
        let state = if !self.enabled {
            0
        } else if hovered {
            2
        } else {
            1
        };
        gui.set_color(0xFFFF_FFFF);
        gui.set_blend(Blend::Alpha);
        let v = 46 + state * 20;
        gui.blit(WIDGETS, self.x, self.y, 0, v, self.w / 2, self.h);
        gui.blit(
            WIDGETS,
            self.x + self.w / 2,
            self.y,
            200 - self.w / 2,
            v,
            self.w / 2,
            self.h,
        );
        let color = if !self.enabled {
            0xA0A0A0
        } else if hovered {
            0xFFFFA0
        } else {
            0xE0E0E0
        };
        gui.centered(
            &self.label,
            self.x + self.w / 2,
            self.y + (self.h - 8) / 2,
            color,
        );
    }
}

/// `GuiContainer` slot hover: the translucent white square over a 16x16 slot.
pub fn slot_highlight(gui: &mut Gui, x: i32, y: i32) {
    gui.gradient(x, y, x + 16, y + 16, 0x80FF_FFFF, 0x80FF_FFFF);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        let mut ascii = [6u8; 256];
        ascii[glyph_index('i').unwrap()] = 2;
        ascii[glyph_index('!').unwrap()] = 2;
        Font {
            ascii,
            unicode: vec![0; 65536],
            force_unicode: false,
        }
    }

    #[test]
    fn clip_trims_upright_quads_and_drops_outside_ones() {
        let mut g = Gui::new(GuiScale::new(854, 480, 2, false), None, 0);
        g.set_clip(Some([10.0, 10.0, 20.0, 20.0]));
        g.gradient(0, 0, 40, 40, 0xFF00_0000, 0xFFFF_FFFF);
        g.fill(40, 40, 50, 50, 0xFFFF_FFFF);
        g.blit_tinted(
            "t".to_owned(),
            0.0,
            10.0,
            40.0,
            10.0,
            [0.0, 0.0, 1.0, 1.0],
            0xFFFF_FFFF,
        );
        g.set_clip(None);
        g.fill(0, 0, 1, 1, 1);
        let q = g.into_quads();
        assert_eq!(q.len(), 3);
        assert_eq!(
            q[0].pos,
            [[20.0, 20.0], [40.0, 20.0], [40.0, 40.0], [20.0, 40.0]]
        );
        assert_eq!(
            q[0].color,
            [0xFF40_4040, 0xFF40_4040, 0xFF80_8080, 0xFF80_8080]
        );
        assert_eq!(
            q[1].pos,
            [[20.0, 20.0], [40.0, 20.0], [40.0, 40.0], [20.0, 40.0]]
        );
        assert_eq!(q[1].uv, [[0.25, 0.0], [0.5, 0.0], [0.5, 1.0], [0.25, 1.0]]);
    }

    #[test]
    fn an_inverted_or_empty_clip_drops_every_quad() {
        let mut g = Gui::new(GuiScale::new(854, 480, 2, false), None, 0);
        for clip in [
            [150.0, 0.0, 100.0, 300.0],
            [0.0, 150.0, 300.0, 100.0],
            [5.0, 5.0, 5.0, 50.0],
        ] {
            g.set_clip(Some(clip));
            g.fill(0, 0, 300, 300, 0xFFFF_FFFF);
        }
        g.set_clip(None);
        g.fill(0, 0, 1, 1, 1);
        assert_eq!(g.into_quads().len(), 1);
    }

    #[test]
    fn gui_scale_follows_scaled_resolution() {
        assert_eq!(GuiScale::new(854, 480, 0, false).factor, 2);
        assert_eq!(GuiScale::new(1920, 1080, 0, false).factor, 4);
        assert_eq!(GuiScale::new(1920, 1080, 2, false).factor, 2);
        assert_eq!(GuiScale::new(2560, 1440, 0, false).factor, 6);
        assert_eq!(GuiScale::new(1280, 720, 0, true).factor, 2);
        assert_eq!(GuiScale::new(854, 480, 0, true).factor, 2);
        let s = GuiScale::new(855, 481, 2, false);
        assert_eq!((s.width, s.height), (428, 241));
    }

    #[test]
    fn width_skips_codes_and_counts_bold() {
        let f = font();
        let m = Metrics(Some(&f));
        assert_eq!(m.width("ab c"), 6 + 6 + 4 + 6);
        assert_eq!(m.width("\u{a7}cab"), 12);
        assert_eq!(m.width("\u{a7}lab\u{a7}ri"), 7 + 7 + 2);
    }

    #[test]
    fn trim_and_wrap_match_font_renderer() {
        let f = font();
        let m = Metrics(Some(&f));
        assert_eq!(m.trim("abcdef", 13, false), "ab");
        assert_eq!(m.trim("abcdef", 13, true), "ef");
        assert_eq!(m.wrap("aa aa aa", 30), vec!["aa aa", "aa"]);
        assert_eq!(
            m.wrap("\u{a7}caaaaaa", 24),
            vec!["\u{a7}caaaa", "\u{a7}caa"]
        );
        assert_eq!(m.wrap("a\nb", 100), vec!["a", "b"]);
    }

    #[test]
    fn color_codes_match_vanilla() {
        assert_eq!(color_code(0), 0x000000);
        assert_eq!(color_code(6), 0xFFAA00);
        assert_eq!(color_code(12), 0xFF5555);
        assert_eq!(color_code(15), 0xFFFFFF);
        assert_eq!(color_code(31), 0x3F3F3F);
    }

    #[test]
    fn transforms_compose_like_gl() {
        let mut g = Gui::new(GuiScale::new(640, 480, 0, false), None, 1);
        g.translate(10.0, 0.0);
        g.scale(2.0, 2.0);
        g.fill(0, 0, 1, 1, 0xFFFFFFFF);
        assert_eq!(g.into_quads()[0].pos[2], [24.0, 4.0]);
    }
}
