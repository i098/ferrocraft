//! `GuiNewChat` (the chat lines over the hotbar) and the `GuiChat` input line.

use crate::canvas::GuiCanvas;
use crate::screens::TextField;
use crate::toolkit::{strip_codes, Gui, Metrics, FONT_HEIGHT};
use ferrocraft_net_client::{Click, ClickAction, Hover, Styled};

const MAX_LINES: usize = 100;

/// One text component of a wrapped line: `ChatComponentText` with its `ChatStyle` events.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub click: Option<Click>,
    pub hover: Option<Hover>,
    pub insertion: Option<String>,
}

/// A wrapped chat line: `getFormattedText()` for drawing, and its components for the hit test.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawnLine {
    pub formatted: String,
    pub pieces: Vec<Piece>,
}

impl DrawnLine {
    /// `getUnformattedText()`: the component texts without the added `§r` codes.
    pub fn unformatted(&self) -> String {
        self.pieces.iter().map(|p| p.text.as_str()).collect()
    }
}

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
    drawn: Vec<Line<DrawnLine>>,
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
                let alpha = line_alpha(age, open, opacity);
                shown += 1;
                if alpha > 3 {
                    let y = -i * 9;
                    gui.fill(0, y - 9, width + 4, y, ((alpha / 2) as u32) << 24);
                    gui.text(
                        &line.content.formatted,
                        0.0,
                        (y - 8) as f32,
                        0xFFFFFF | (alpha as u32) << 24,
                    );
                }
            }
            i += 1;
        }
        if open {
            self.draw_bar(gui, total, shown);
        }
        gui.pop();
    }

    fn draw_bar(&self, gui: &mut Gui, total: i32, shown: i32) {
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

    /// `getChatComponent`, for an open chat: the component under the window pixel `(px, py)`,
    /// where `py` counts up from the bottom edge, like LWJGL's `Mouse.getY`.
    pub fn component_at(&self, px: i32, py: i32, factor: i32, m: Metrics) -> Option<&Piece> {
        let s = &self.settings;
        let j = ((px / factor - 3) as f32 / s.scale).floor() as i32;
        let k = ((py / factor - 27) as f32 / s.scale).floor() as i32;
        if j < 0 || k < 0 {
            return None;
        }
        let lines = s.line_count(true).min(self.drawn.len() as i32);
        if j > (s.chat_width() as f32 / s.scale).floor() as i32 || k >= FONT_HEIGHT * lines + lines
        {
            return None;
        }
        let line = self
            .drawn
            .get(usize::try_from(k / FONT_HEIGHT + self.scroll_pos).ok()?)?;
        let mut x = 0;
        line.content.pieces.iter().find(|p| {
            x += m.width(&p.text);
            x > j
        })
    }
}

/// What a click on a chat component asks the screen to do (`GuiScreen.handleComponentClick`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatClick {
    /// The click is used up and nothing else happens.
    Handled,
    /// `run_command`: send it as a chat message.
    Command(String),
    /// `suggest_command`: replace the chat field text.
    Suggest(String),
    /// Shift-click on an insertion: write at the cursor.
    Insert(String),
    /// `open_url` with `chatLinksPrompt`: ask first (`GuiConfirmOpenLink`).
    ConfirmLink(String),
    /// `open_url` without the prompt: open the system browser.
    OpenLink(String),
}

/// `GuiScreen.handleComponentClick`; `None` means the click is not used (no event, or links are off).
pub fn click(piece: &Piece, shift: bool, links: bool, prompt: bool) -> Option<ChatClick> {
    if shift {
        return Some(
            piece
                .insertion
                .clone()
                .map_or(ChatClick::Handled, ChatClick::Insert),
        );
    }
    let c = piece.click.as_ref()?;
    Some(match c.action {
        ClickAction::OpenUrl if !links => return None,
        ClickAction::OpenUrl if !web_url(&c.value) => ChatClick::Handled,
        ClickAction::OpenUrl if prompt => ChatClick::ConfirmLink(c.value.clone()),
        ClickAction::OpenUrl => ChatClick::OpenLink(c.value.clone()),
        ClickAction::RunCommand => ChatClick::Command(c.value.clone()),
        ClickAction::SuggestCommand => ChatClick::Suggest(c.value.clone()),
    })
}

/// `new URI(value)` with a scheme of `http` or `https`; vanilla only logs anything else.
fn web_url(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    let ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    ok && ["http", "https"].contains(&scheme.to_ascii_lowercase().as_str())
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn line_alpha(age: i32, open: bool, opacity: f32) -> i32 {
    let d = ((1.0 - age as f64 / 200.0) * 10.0).clamp(0.0, 1.0);
    let alpha = if open { 255 } else { (255.0 * d * d) as i32 };
    (alpha as f32 * opacity) as i32
}

/// `GuiUtilRenderComponents.splitText(component, width, font, false, false)`: each line as
/// `getFormattedText()` plus its components.
pub(crate) fn split(runs: &[Styled], width: i32, m: Metrics, colors: bool) -> Vec<DrawnLine> {
    let mut queue: Vec<Styled> = runs.to_vec();
    let mut lines = Vec::new();
    let mut line = DrawnLine {
        formatted: String::from("\u{a7}r"),
        pieces: Vec::new(),
    };
    let mut used = 0;
    let mut j = 0;
    while j < queue.len() {
        let run = queue[j].clone();
        let (format, mut s) = (run.format.clone(), run.text.clone());
        let rest_of = |text: String| Styled {
            text,
            ..run.clone()
        };
        let mut flush = false;
        if let Some(k) = s.find('\n') {
            queue.insert(j + 1, rest_of(s[k + 1..].to_owned()));
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
            let (s2, rest) = break_line(m, &s4, width - used, used > 0);
            if let Some(s3) = rest {
                queue.insert(j + 1, rest_of(s3));
            }
            w = m.width(&s2);
            piece = s2;
            flush = true;
        }
        if used + w <= width {
            used += w;
            line.formatted.push_str(&format);
            line.formatted.push_str(&piece);
            line.formatted.push_str("\u{a7}r");
            line.pieces.push(Piece {
                text: piece,
                click: run.click,
                hover: run.hover,
                insertion: run.insertion,
            });
        } else {
            flush = true;
        }
        if flush {
            let fresh = DrawnLine {
                formatted: String::from("\u{a7}r"),
                pieces: Vec::new(),
            };
            lines.push(std::mem::replace(&mut line, fresh));
            used = 0;
        }
        j += 1;
    }
    lines.push(line);
    lines
}

/// The part of `s4` that fits `room`, and the rest to carry to the next line.
fn break_line(m: Metrics, s4: &str, room: i32, line_used: bool) -> (String, Option<String>) {
    let s2 = m.trim(s4, room, false);
    if s2.len() >= s4.len() {
        return (s2, None);
    }
    match s2.rfind(' ') {
        Some(l) if m.width(&s4[..l]) > 0 => (s4[..l].to_owned(), Some(s4[l..].to_owned())),
        _ if line_used && !s4.contains(' ') => (String::new(), Some(s4.to_owned())),
        _ => {
            let s3 = s4[s2.len()..].to_owned();
            (s2, Some(s3))
        }
    }
}

/// `GuiChat.drawScreen` input line: the dark bar and the text field with its caret and selection.
pub fn draw_input(gui: &mut Gui, field: &TextField) {
    let (w, h) = (gui.scale.width, gui.scale.height);
    gui.fill(2, h - 14, w - 2, h - 2, 0x8000_0000);
    field.draw(&mut GuiCanvas::new(gui));
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
                ..Styled::default()
            })
            .collect()
    }

    fn mono() -> Font {
        Font {
            ascii: [6; 256],
            unicode: vec![0; 65536],
            force_unicode: false,
        }
    }

    fn linked(action: ClickAction, value: &str) -> Styled {
        Styled {
            text: "CLICK".into(),
            click: Some(Click {
                action,
                value: value.into(),
            }),
            ..Styled::default()
        }
    }

    fn first_piece(runs: &[Styled]) -> Piece {
        split(runs, 100, Metrics(None), true)
            .remove(0)
            .pieces
            .remove(0)
    }

    #[test]
    fn hit_test_finds_the_clicked_component() {
        let f = mono();
        let m = Metrics(Some(&f));
        let mut c = Chat::default();
        let mut msg = runs(&[("", "ab ")]);
        msg.push(linked(ClickAction::RunCommand, "/party accept"));
        c.print(msg, 0, 0, false, m);
        // The newest line is the bottom one: window Y 27..36 counted from the bottom.
        let at = |x, y| c.component_at(x, y, 1, m).map(|p| p.click.is_some());
        assert_eq!(at(3 + 5, 30), Some(false));
        assert_eq!(at(3 + 20, 30), Some(true));
        assert_eq!(at(3 + 45, 30), Some(true));
        // Past the text, above the last line, left of the box and below the chat field.
        assert_eq!(at(3 + 46, 30), None);
        assert_eq!(at(3 + 20, 27 + 9), None);
        assert_eq!(at(2, 30), None);
        assert_eq!(at(3 + 20, 26), None);
    }

    #[test]
    fn click_actions_follow_vanilla() {
        let cmd = first_piece(&[linked(ClickAction::RunCommand, "/party accept")]);
        assert_eq!(
            click(&cmd, false, true, true),
            Some(ChatClick::Command("/party accept".into()))
        );
        // Shift suppresses the event; no insertion leaves the click used up.
        assert_eq!(click(&cmd, true, true, true), Some(ChatClick::Handled));
        let sug = first_piece(&[linked(ClickAction::SuggestCommand, "/msg Bob ")]);
        assert_eq!(
            click(&sug, false, true, true),
            Some(ChatClick::Suggest("/msg Bob ".into()))
        );
        let url = first_piece(&[linked(ClickAction::OpenUrl, "https://example.com/a")]);
        assert_eq!(
            click(&url, false, true, true),
            Some(ChatClick::ConfirmLink("https://example.com/a".into()))
        );
        assert_eq!(
            click(&url, false, true, false),
            Some(ChatClick::OpenLink("https://example.com/a".into()))
        );
        assert_eq!(click(&url, false, false, true), None);
        for bad in ["example.com", "ftp://example.com", "http://a b"] {
            let p = first_piece(&[linked(ClickAction::OpenUrl, bad)]);
            assert_eq!(
                click(&p, false, true, true),
                Some(ChatClick::Handled),
                "{bad}"
            );
        }
        assert_eq!(
            click(&first_piece(&runs(&[("", "x")])), false, true, true),
            None
        );
    }

    #[test]
    fn split_breaks_at_spaces_and_keeps_styles() {
        let f = mono();
        let m = Metrics(Some(&f));
        // "aaa bbb" is 6*6 + 4 = 40 wide; 30 keeps "aaa" and moves " bbb" down.
        let lines = split(&runs(&[("\u{a7}c", "aaa bbb")]), 30, m, true);
        let formatted: Vec<_> = lines.iter().map(|l| l.formatted.as_str()).collect();
        assert_eq!(
            formatted,
            vec![
                "\u{a7}r\u{a7}c\u{a7}caaa\u{a7}r",
                "\u{a7}r\u{a7}c\u{a7}c bbb\u{a7}r"
            ]
        );
        let lines = split(&runs(&[("", "a\nb")]), 100, m, true);
        let formatted: Vec<_> = lines.iter().map(|l| l.formatted.as_str()).collect();
        assert_eq!(formatted, vec!["\u{a7}ra\u{a7}r", "\u{a7}rb\u{a7}r"]);
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
