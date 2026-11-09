//! `GuiNewChat` (the chat lines over the hotbar) and the `GuiChat` input line.

use crate::toolkit::{strip_codes, Gui, Metrics, FONT_HEIGHT};
use ferrocraft_net_client::Styled;

const MAX_LINES: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Full,
    System,
    Hidden,
}

/// The chat options of `GameSettings`, with vanilla defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatSettings {
    pub visibility: Visibility,
    pub colors: bool,
    pub opacity: f32,
    pub scale: f32,
    pub width: f32,
    pub height_focused: f32,
    pub height_unfocused: f32,
}

impl Default for ChatSettings {
    fn default() -> Self {
        ChatSettings {
            visibility: Visibility::Full,
            colors: true,
            opacity: 1.0,
            scale: 1.0,
            width: 1.0,
            height_focused: 1.0,
            height_unfocused: 0.443_662,
        }
    }
}

impl ChatSettings {
    /// `getChatWidth`.
    pub fn chat_width(&self) -> i32 {
        (self.width * 280.0 + 40.0).floor() as i32
    }

    /// `getLineCount`: 20 lines open, 10 closed with the defaults.
    pub fn line_count(&self, open: bool) -> i32 {
        let h = if open {
            self.height_focused
        } else {
            self.height_unfocused
        };
        (h * 160.0 + 20.0).floor() as i32 / 9
    }

    fn wrap_width(&self) -> i32 {
        (self.chat_width() as f32 / self.scale).floor() as i32
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Line<T> {
    counter: i32,
    id: i32,
    content: T,
}

/// Chat history and scroll state of `GuiNewChat`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chat {
    pub settings: ChatSettings,
    /// Whole messages, newest first.
    messages: Vec<Line<Vec<Styled>>>,
    /// Wrapped lines as formatted text, newest first.
    drawn: Vec<Line<String>>,
    scroll_pos: i32,
    scrolled: bool,
}

impl Chat {
    /// `printChatMessageWithOptionalDeletion`: a non-zero `id` replaces the earlier line with that id.
    pub fn print(&mut self, runs: Vec<Styled>, id: i32, counter: i32, open: bool, m: Metrics) {
        if id != 0 {
            self.delete(id);
        }
        self.set_line(&runs, id, counter, open, m);
        self.messages.insert(
            0,
            Line {
                counter,
                id,
                content: runs,
            },
        );
        self.messages.truncate(MAX_LINES);
    }

    fn set_line(&mut self, runs: &[Styled], id: i32, counter: i32, open: bool, m: Metrics) {
        for content in split(runs, self.settings.wrap_width(), m, self.settings.colors) {
            if open && self.scroll_pos > 0 {
                self.scrolled = true;
                self.scroll(1, open);
            }
            self.drawn.insert(
                0,
                Line {
                    counter,
                    id,
                    content,
                },
            );
        }
        self.drawn.truncate(MAX_LINES);
    }

    /// `refreshChat`: wrap every message again after a width or scale change.
    pub fn refresh(&mut self, m: Metrics) {
        self.drawn.clear();
        self.reset_scroll();
        let messages = std::mem::take(&mut self.messages);
        for l in messages.iter().rev() {
            self.set_line(&l.content, l.id, l.counter, false, m);
        }
        self.messages = messages;
    }

    pub fn clear(&mut self) {
        self.drawn.clear();
        self.messages.clear();
    }

    fn delete(&mut self, id: i32) {
        self.drawn.retain(|l| l.id != id);
        if let Some(i) = self.messages.iter().position(|l| l.id == id) {
            self.messages.remove(i);
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_pos = 0;
        self.scrolled = false;
    }

    /// `scroll`: the mouse wheel and page keys while the chat is open.
    pub fn scroll(&mut self, amount: i32, open: bool) {
        self.scroll_pos += amount;
        let max = self.drawn.len() as i32 - self.settings.line_count(open);
        if self.scroll_pos > max {
            self.scroll_pos = max;
        }
        if self.scroll_pos <= 0 {
            self.scroll_pos = 0;
            self.scrolled = false;
        }
    }

    /// `drawChat`: call with the GUI translated to `(0, height - 48)`.
    pub fn draw(&self, gui: &mut Gui, counter: i32, open: bool) {
        let s = &self.settings;
        if s.visibility == Visibility::Hidden || self.drawn.is_empty() {
            return;
        }
        let lines = s.line_count(open);
        let opacity = s.opacity * 0.9 + 0.1;
        let width = (s.chat_width() as f32 / s.scale).ceil() as i32;
        let total = self.drawn.len() as i32;
        let mut shown = 0;
        gui.push();
        gui.translate(2.0, 20.0);
        gui.scale(s.scale, s.scale);
        let mut i = 0;
        while i + self.scroll_pos < total && i < lines {
            let line = &self.drawn[(i + self.scroll_pos) as usize];
            let age = counter - line.counter;
            if age < 200 || open {
                let d = ((1.0 - age as f64 / 200.0) * 10.0).clamp(0.0, 1.0);
                let mut alpha = (255.0 * d * d) as i32;
                if open {
                    alpha = 255;
                }
                alpha = (alpha as f32 * opacity) as i32;
                shown += 1;
                if alpha > 3 {
                    let y = -i * 9;
                    gui.fill(0, y - 9, width + 4, y, ((alpha / 2) as u32) << 24);
                    gui.text(
                        &line.content,
                        0.0,
                        (y - 8) as f32,
                        0xFFFFFF | (alpha as u32) << 24,
                    );
                }
            }
            i += 1;
        }
        if open {
            gui.translate(-3.0, 0.0);
            let all = total * FONT_HEIGHT + total;
            let visible = shown * FONT_HEIGHT + shown;
            let top = self.scroll_pos * visible / total;
            let bar = visible * visible / all;
            if all != visible {
                let a: u32 = if top > 0 { 170 } else { 96 };
                let color = if self.scrolled { 0xCC3333 } else { 0x3333AA };
                gui.fill(0, -top, 2, -top - bar, color + (a << 24));
                gui.fill(2, -top, 1, -top - bar, 0xCCCCCC + (a << 24));
            }
        }
        gui.pop();
    }
}

/// `GuiUtilRenderComponents.splitText(component, width, font, false, false)`, each line as
/// `getFormattedText()`.
fn split(runs: &[Styled], width: i32, m: Metrics, colors: bool) -> Vec<String> {
    let mut queue: Vec<(String, String)> = runs
        .iter()
        .map(|r| (r.format.clone(), r.text.clone()))
        .collect();
    let mut lines = Vec::new();
    let mut line = String::from("\u{a7}r");
    let mut used = 0;
    let mut j = 0;
    while j < queue.len() {
        let (format, mut s) = queue[j].clone();
        let mut flush = false;
        if let Some(k) = s.find('\n') {
            queue.insert(j + 1, (format.clone(), s[k + 1..].to_owned()));
            s.truncate(k + 1);
            flush = true;
        }
        let mut s4 = format.clone() + &s;
        if !colors {
            s4 = strip_codes(&s4);
        }
        let mut piece = s4.strip_suffix('\n').unwrap_or(&s4).to_owned();
        let mut w = m.width(&piece);
        if used + w > width {
            let mut s2 = m.trim(&s4, width - used, false);
            if s2.len() < s4.len() {
                let mut s3 = s4[s2.len()..].to_owned();
                if !s3.is_empty() {
                    match s2.rfind(' ') {
                        Some(l) if m.width(&s4[..l]) > 0 => {
                            s2 = s4[..l].to_owned();
                            s3 = s4[l..].to_owned();
                        }
                        _ if used > 0 && !s4.contains(' ') => {
                            s2 = String::new();
                            s3 = s4.clone();
                        }
                        _ => {}
                    }
                    queue.insert(j + 1, (format.clone(), s3));
                }
            }
            w = m.width(&s2);
            piece = s2;
            flush = true;
        }
        if used + w <= width {
            used += w;
            line.push_str(&format);
            line.push_str(&piece);
            line.push_str("\u{a7}r");
        } else {
            flush = true;
        }
        if flush {
            lines.push(std::mem::replace(&mut line, String::from("\u{a7}r")));
            used = 0;
        }
        j += 1;
    }
    lines.push(line);
    lines
}

/// `GuiChat.drawScreen` input line: the dark bar and the text field with its blinking `_`.
pub fn draw_input(gui: &mut Gui, text: &str, cursor_counter: i32) {
    let (w, h) = (gui.scale.width, gui.scale.height);
    gui.fill(2, h - 14, w - 2, h - 2, 0x8000_0000);
    let field = w - 4;
    let shown = if gui.width(text) > field {
        gui.metrics().trim(text, field, true)
    } else {
        text.to_owned()
    };
    let end = gui.text(&shown, 4.0, (h - 12) as f32, 0xE0E0E0);
    if cursor_counter / 6 % 2 == 0 {
        gui.text("_", end as f32, (h - 12) as f32, 0xE0E0E0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use render::Font;

    fn runs(parts: &[(&str, &str)]) -> Vec<Styled> {
        parts
            .iter()
            .map(|(f, t)| Styled {
                format: f.to_string(),
                text: t.to_string(),
            })
            .collect()
    }

    #[test]
    fn split_breaks_at_spaces_and_keeps_styles() {
        let f = Font {
            ascii: [6; 256],
            unicode: vec![0; 65536],
        };
        let m = Metrics(Some(&f));
        // "aaa bbb" is 6*6 + 4 = 40 wide; 30 keeps "aaa" and moves " bbb" down.
        let lines = split(&runs(&[("\u{a7}c", "aaa bbb")]), 30, m, true);
        assert_eq!(
            lines,
            vec![
                "\u{a7}r\u{a7}c\u{a7}caaa\u{a7}r",
                "\u{a7}r\u{a7}c\u{a7}c bbb\u{a7}r"
            ]
        );
        let lines = split(&runs(&[("", "a\nb")]), 100, m, true);
        assert_eq!(lines, vec!["\u{a7}ra\u{a7}r", "\u{a7}rb\u{a7}r"]);
    }

    #[test]
    fn lines_cap_at_one_hundred_and_scroll_clamps() {
        let mut c = Chat::default();
        for i in 0..120 {
            c.print(runs(&[("", "x")]), 0, i, false, Metrics(None));
        }
        assert_eq!(c.drawn.len(), 100);
        c.scroll(500, true);
        assert_eq!(c.scroll_pos, 80);
        c.scroll(-500, true);
        assert_eq!((c.scroll_pos, c.scrolled), (0, false));
    }

    #[test]
    fn default_line_counts() {
        let s = ChatSettings::default();
        assert_eq!((s.line_count(false), s.line_count(true)), (10, 20));
        assert_eq!(s.chat_width(), 320);
    }
}
