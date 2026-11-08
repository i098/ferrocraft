//! Flattens a chat component (JSON string) to styled runs or plain text.

use serde_json::{Map, Value};

/// One text run of a chat component with the vanilla `ChatStyle.getFormattingCode()` of its
/// effective style, such as `"§c§l"`; `format` is empty for the default style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Styled {
    pub format: String,
    pub text: String,
}

/// Runs of a chat component in vanilla iteration order. Text that is not JSON is one unstyled run.
pub fn styled(json: &str) -> Vec<Styled> {
    let mut out = Vec::new();
    match serde_json::from_str::<Value>(json) {
        Ok(v) => flatten(&v, Style::default(), &mut out),
        Err(_) => push(&mut out, Style::default(), json),
    }
    out
}

/// Plain text of a chat component. Text that is not JSON is returned unchanged.
pub fn plain_text(json: &str) -> String {
    styled(json).into_iter().map(|s| s.text).collect()
}

/// `IChatComponent.getFormattedText()`: each run's format code, its text, then `§r`.
pub fn formatted_text(runs: &[Styled]) -> String {
    runs.iter()
        .map(|r| format!("{}{}\u{a7}r", r.format, r.text))
        .collect()
}

#[derive(Clone, Copy, Default)]
struct Style {
    color: Option<char>,
    bold: bool,
    italic: bool,
    underlined: bool,
    obfuscated: bool,
    strikethrough: bool,
}

impl Style {
    /// The child style: fields the component sets override the parent's.
    fn child(mut self, o: &Map<String, Value>) -> Style {
        if let Some(c) = o.get("color").and_then(Value::as_str).and_then(color_code) {
            self.color = Some(c);
        }
        let flag = |key: &str, parent: bool| o.get(key).and_then(Value::as_bool).unwrap_or(parent);
        self.bold = flag("bold", self.bold);
        self.italic = flag("italic", self.italic);
        self.underlined = flag("underlined", self.underlined);
        self.obfuscated = flag("obfuscated", self.obfuscated);
        self.strikethrough = flag("strikethrough", self.strikethrough);
        self
    }

    fn code(&self) -> String {
        let mut s = String::new();
        let mut add = |on: bool, c: char| {
            if on {
                s.push('\u{a7}');
                s.push(c);
            }
        };
        add(self.color.is_some(), self.color.unwrap_or('f'));
        add(self.bold, 'l');
        add(self.italic, 'o');
        add(self.underlined, 'n');
        add(self.obfuscated, 'k');
        add(self.strikethrough, 'm');
        s
    }
}

fn color_code(name: &str) -> Option<char> {
    const NAMES: [&str; 16] = [
        "black",
        "dark_blue",
        "dark_green",
        "dark_aqua",
        "dark_red",
        "dark_purple",
        "gold",
        "gray",
        "dark_gray",
        "blue",
        "green",
        "aqua",
        "red",
        "light_purple",
        "yellow",
        "white",
    ];
    let i = NAMES.iter().position(|n| *n == name)?;
    char::from_digit(i as u32, 16)
}

fn push(out: &mut Vec<Styled>, style: Style, text: &str) {
    out.push(Styled {
        format: style.code(),
        text: text.to_owned(),
    });
}

fn translation(key: &str) -> Option<&'static str> {
    Some(match key {
        "chat.type.text" => "<%s> %s",
        "chat.type.emote" => "* %s %s",
        "chat.type.announcement" => "[%s] %s",
        "chat.type.admin" => "[%s: %s]",
        "multiplayer.player.joined" => "%s joined the game",
        "multiplayer.player.left" => "%s left the game",
        _ => return None,
    })
}

fn flatten(v: &Value, parent: Style, out: &mut Vec<Styled>) {
    match v {
        Value::String(s) => push(out, parent, s),
        Value::Array(items) => items.iter().for_each(|i| flatten(i, parent, out)),
        Value::Object(o) => {
            let style = parent.child(o);
            if let Some(t) = o.get("text").and_then(Value::as_str) {
                push(out, style, t);
            } else if let Some(key) = o.get("translate").and_then(Value::as_str) {
                let args = o
                    .get("with")
                    .and_then(Value::as_array)
                    .map_or(&[][..], |a| a);
                translate(key, args, style, out);
            }
            if let Some(extra) = o.get("extra") {
                flatten(extra, style, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn translate(key: &str, args: &[Value], style: Style, out: &mut Vec<Styled>) {
    let Some(pattern) = translation(key) else {
        push(out, style, key);
        for a in args {
            push(out, style, " ");
            flatten(a, style, out);
        }
        return;
    };
    let mut next = args.iter();
    let mut rest = pattern;
    while let Some(i) = rest.find("%s") {
        push(out, style, &rest[..i]);
        if let Some(a) = next.next() {
            flatten(a, style, out);
        }
        rest = &rest[i + 2..];
    }
    push(out, style, rest);
}

#[cfg(test)]
mod tests {
    use super::{formatted_text, plain_text, styled};

    #[test]
    fn vanilla_chat_echo() {
        let j = r#"{"translate":"chat.type.text","with":[{"text":"Bob","clickEvent":{"action":"suggest_command","value":"/msg Bob "}},"hi there"]}"#;
        assert_eq!(plain_text(j), "<Bob> hi there");
    }

    #[test]
    fn text_with_extra_and_unknown_key() {
        assert_eq!(
            plain_text(r#"{"text":"a","extra":[{"text":"b"},"c"]}"#),
            "abc"
        );
        assert_eq!(
            plain_text(r#"{"translate":"x.y","with":["1","2"]}"#),
            "x.y 1 2"
        );
        assert_eq!(plain_text("not json"), "not json");
    }

    #[test]
    fn styles_inherit_and_override() {
        let j = r#"{"text":"a","color":"red","bold":true,"extra":[{"text":"b","bold":false},{"text":"c","color":"gold","italic":true}]}"#;
        assert_eq!(
            formatted_text(&styled(j)),
            "\u{a7}c\u{a7}la\u{a7}r\u{a7}cb\u{a7}r\u{a7}6\u{a7}l\u{a7}oc\u{a7}r"
        );
    }
}
