//! Flattens a chat component (JSON string) to styled runs or plain text.

use serde_json::{Map, Value};

/// One text run of a chat component with the vanilla `ChatStyle.getFormattingCode()` of its
/// effective style, such as `"§c§l"`; `format` is empty for the default style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Styled {
    pub format: String,
    pub text: String,
    /// `ChatStyle.getChatClickEvent()`, inherited from the parent component like the other styles.
    pub click: Option<Click>,
    /// `ChatStyle.getChatHoverEvent()`.
    pub hover: Option<Hover>,
    /// `ChatStyle.getInsertion()`: the text a shift-click puts in the chat field.
    pub insertion: Option<String>,
}

/// `ClickEvent.Action` that chat may carry (`open_file` and `twitch_user_page` are dropped, as in vanilla).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    OpenUrl,
    RunCommand,
    SuggestCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    pub action: ClickAction,
    pub value: String,
}

/// `HoverEvent.Action`; `show_entity` is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverAction {
    ShowText,
    ShowItem,
    ShowAchievement,
}

/// A hover event; `value` is the event's component, flattened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub action: HoverAction,
    pub value: Vec<Styled>,
}

/// Runs of a chat component in vanilla iteration order. Text that is not JSON is one unstyled run.
pub fn styled(json: &str) -> Vec<Styled> {
    let mut out = Vec::new();
    match serde_json::from_str::<Value>(json) {
        Ok(v) => flatten(&v, Style::default(), &mut out),
        Err(_) => push(&mut out, &Style::default(), json),
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

#[derive(Clone, Default)]
struct Style {
    color: Option<char>,
    bold: bool,
    italic: bool,
    underlined: bool,
    obfuscated: bool,
    strikethrough: bool,
    click: Option<Click>,
    hover: Option<Hover>,
    insertion: Option<String>,
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
        if let Some(c) = o.get("clickEvent").and_then(click) {
            self.click = Some(c);
        }
        if let Some(h) = o.get("hoverEvent").and_then(hover) {
            self.hover = Some(h);
        }
        if let Some(i) = o.get("insertion").and_then(Value::as_str) {
            self.insertion = Some(i.to_owned());
        }
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

fn click(v: &Value) -> Option<Click> {
    let action = match v.get("action")?.as_str()?.to_ascii_lowercase().as_str() {
        "open_url" => ClickAction::OpenUrl,
        "run_command" => ClickAction::RunCommand,
        "suggest_command" => ClickAction::SuggestCommand,
        _ => return None,
    };
    let value = v.get("value")?.as_str()?.to_owned();
    Some(Click { action, value })
}

fn hover(v: &Value) -> Option<Hover> {
    let action = match v.get("action")?.as_str()?.to_ascii_lowercase().as_str() {
        "show_text" => HoverAction::ShowText,
        "show_item" => HoverAction::ShowItem,
        "show_achievement" => HoverAction::ShowAchievement,
        _ => return None,
    };
    let mut value = Vec::new();
    flatten(v.get("value")?, Style::default(), &mut value);
    Some(Hover { action, value })
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

fn push(out: &mut Vec<Styled>, style: &Style, text: &str) {
    if text.is_empty() {
        return;
    }
    out.push(Styled {
        format: style.code(),
        text: text.to_owned(),
        click: style.click.clone(),
        hover: style.hover.clone(),
        insertion: style.insertion.clone(),
    });
}

const TRANSLATIONS: &[(&str, &str)] = &[
    ("chat.type.text", "<%s> %s"),
    ("chat.type.emote", "* %s %s"),
    ("chat.type.announcement", "[%s] %s"),
    ("chat.type.admin", "[%s: %s]"),
    ("multiplayer.player.joined", "%s joined the game"),
    ("multiplayer.player.left", "%s left the game"),
    (
        "chat.type.achievement",
        "%s has just earned the achievement %s",
    ),
    (
        "chat.type.achievement.taken",
        "%s has lost the achievement %s",
    ),
    ("chat.stream.emote", "(%s) * %s %s"),
    ("chat.stream.text", "(%s) <%s> %s"),
    (
        "commands.generic.notFound",
        "Unknown command. Try /help for a list of commands",
    ),
    (
        "commands.generic.permission",
        "You do not have permission to use this command",
    ),
    ("commands.generic.usage", "Usage: %s"),
    (
        "commands.generic.player.notFound",
        "That player cannot be found",
    ),
    (
        "commands.generic.exception",
        "An unknown error occurred while attempting to perform this command",
    ),
    (
        "commands.message.display.incoming",
        "%s whispers to you: %s",
    ),
    ("commands.message.display.outgoing", "You whisper to %s: %s"),
    ("death.attack.inFire", "%1$s went up in flames"),
    ("death.attack.onFire", "%1$s burned to death"),
    ("death.attack.lava", "%1$s tried to swim in lava"),
    ("death.attack.inWall", "%1$s suffocated in a wall"),
    ("death.attack.drown", "%1$s drowned"),
    ("death.attack.starve", "%1$s starved to death"),
    ("death.attack.cactus", "%1$s was pricked to death"),
    ("death.attack.generic", "%1$s died"),
    ("death.attack.explosion", "%1$s blew up"),
    ("death.attack.explosion.player", "%1$s was blown up by %2$s"),
    ("death.attack.magic", "%1$s was killed by magic"),
    ("death.attack.wither", "%1$s withered away"),
    ("death.attack.anvil", "%1$s was squashed by a falling anvil"),
    (
        "death.attack.fallingBlock",
        "%1$s was squashed by a falling block",
    ),
    ("death.attack.mob", "%1$s was slain by %2$s"),
    ("death.attack.player", "%1$s was slain by %2$s"),
    ("death.attack.arrow", "%1$s was shot by %2$s"),
    ("death.attack.fireball", "%1$s was fireballed by %2$s"),
    ("death.attack.thrown", "%1$s was pummeled by %2$s"),
    (
        "death.attack.indirectMagic",
        "%1$s was killed by %2$s using magic",
    ),
    ("death.attack.thorns", "%1$s was killed trying to hurt %2$s"),
    ("death.attack.fall", "%1$s hit the ground too hard"),
    ("death.attack.outOfWorld", "%1$s fell out of the world"),
    ("death.fell.accident.generic", "%1$s fell from a high place"),
    ("death.fell.accident.ladder", "%1$s fell off a ladder"),
    ("death.fell.accident.vines", "%1$s fell off some vines"),
    ("death.fell.accident.water", "%1$s fell out of the water"),
    ("death.fell.killer", "%1$s was doomed to fall"),
];

fn translation(key: &str) -> Option<&'static str> {
    TRANSLATIONS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|&(_, v)| v)
}

fn flatten(v: &Value, parent: Style, out: &mut Vec<Styled>) {
    match v {
        Value::String(s) => push(out, &parent, s),
        Value::Array(items) => {
            let Some((first, rest)) = items.split_first() else {
                return;
            };
            flatten(first, parent.clone(), out);
            let style = first
                .as_object()
                .map_or_else(|| parent.clone(), |o| parent.clone().child(o));
            rest.iter().for_each(|i| flatten(i, style.clone(), out));
        }
        Value::Object(o) => {
            let style = parent.child(o);
            if let Some(t) = o.get("text").and_then(Value::as_str) {
                push(out, &style, t);
            } else if let Some(key) = o.get("translate").and_then(Value::as_str) {
                let args = o
                    .get("with")
                    .and_then(Value::as_array)
                    .map_or(&[][..], |a| a);
                translate(key, args, &style, out);
            }
            if let Some(extra) = o.get("extra").and_then(Value::as_array) {
                extra.iter().for_each(|e| flatten(e, style.clone(), out));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn translate(key: &str, args: &[Value], style: &Style, out: &mut Vec<Styled>) {
    let Some(pattern) = translation(key) else {
        push(out, style, key);
        for a in args {
            push(out, style, " ");
            flatten(a, style.clone(), out);
        }
        return;
    };
    let mut next = 0;
    let mut rest = pattern;
    while let Some(i) = rest.find('%') {
        push(out, style, &rest[..i]);
        let spec = &rest[i + 1..];
        let (index, len) = match spec.find("$s") {
            Some(d) if d > 0 && spec[..d].bytes().all(|b| b.is_ascii_digit()) => {
                (spec[..d].parse::<usize>().unwrap_or(1) - 1, d + 2)
            }
            _ if spec.starts_with('s') => {
                next += 1;
                (next - 1, 1)
            }
            _ => {
                push(out, style, "%");
                rest = spec;
                continue;
            }
        };
        if let Some(a) = args.get(index) {
            flatten(a, style.clone(), out);
        }
        rest = &spec[len..];
    }
    push(out, style, rest);
}

#[cfg(test)]
mod tests {
    use super::{formatted_text, plain_text, styled};

    #[test]
    fn array_siblings_inherit_first_style() {
        let runs = styled(r#"[{"text":"a","color":"red"},"b"]"#);
        assert_eq!(formatted_text(&runs), "\u{a7}ca\u{a7}r\u{a7}cb\u{a7}r");
    }

    #[test]
    fn empty_translate_is_empty() {
        assert_eq!(formatted_text(&styled(r#"{"translate":""}"#)), "");
        assert_eq!(formatted_text(&styled(r#"{"text":""}"#)), "");
    }

    #[test]
    fn positional_death_message() {
        let j = r#"{"translate":"death.attack.player","with":["Steve","Alex"]}"#;
        assert_eq!(plain_text(j), "Steve was slain by Alex");
        let j = r#"{"translate":"chat.type.achievement","with":["Steve","[Taking Inventory]"]}"#;
        assert_eq!(
            plain_text(j),
            "Steve has just earned the achievement [Taking Inventory]"
        );
    }

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

    #[test]
    fn events_are_kept_and_inherited() {
        use super::{Click, ClickAction, HoverAction};
        let j = r#"{"text":"a","clickEvent":{"action":"run_command","value":"/x"},"hoverEvent":{"action":"show_text","value":{"text":"tip","color":"red"}},"extra":[{"text":"b"},{"text":"c","clickEvent":{"action":"open_file","value":"/etc"}}]}"#;
        let runs = styled(j);
        let click = Some(Click {
            action: ClickAction::RunCommand,
            value: "/x".into(),
        });
        assert_eq!(runs.len(), 3);
        // The child inherits the parent's events; `open_file` is not allowed in chat.
        assert!(runs.iter().all(|r| r.click == click));
        let hover = runs[1].hover.as_ref().unwrap();
        assert_eq!(hover.action, HoverAction::ShowText);
        assert_eq!(formatted_text(&hover.value), "\u{a7}ctip\u{a7}r");
    }
}
