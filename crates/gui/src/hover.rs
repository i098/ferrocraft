//! `GuiScreen.handleComponentHover`: the tooltip lines of a chat hover event.

use crate::inventory::tooltip;
use crate::screens::{wrap, Metrics, Texts};
use ferrocraft_net_client::{formatted_text, Hover, HoverAction, Styled};
use world::inventory::{Compound, ItemStack, Nbt, Tag};

const RED: &str = "\u{a7}c";

/// The lines to draw with `drawHoveringText`.
pub fn lines(h: &Hover, texts: &Texts, advanced: bool, m: &dyn Metrics) -> Vec<String> {
    match h.action {
        HoverAction::ShowText => formatted_text(&h.value)
            .split('\n')
            .map(str::to_owned)
            .collect(),
        HoverAction::ShowItem => item_lines(&h.value, advanced),
        HoverAction::ShowAchievement => stat_lines(&h.value, texts, m),
    }
}

/// `getUnformattedText`.
fn unformatted(runs: &[Styled]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

fn item_lines(value: &[Styled], advanced: bool) -> Vec<String> {
    match item(&unformatted(value)) {
        Some(stack) => tooltip::lines(&stack, advanced),
        None => vec![format!("{RED}Invalid Item!")],
    }
}

/// `ItemStack.loadItemStackFromNBT(JsonToNBT.getTagFromJson(text))`.
// shortcut: only numeric item ids load; 1.8.9 itself writes numeric ids, name ids need an item name table.
fn item(snbt: &str) -> Option<ItemStack> {
    let Tag::Compound(root) = parse(snbt)? else {
        return None;
    };
    let id = i16::try_from(number(tooltip::get(&root, "id")?)?).ok()?;
    if id <= 0 {
        return None;
    }
    let count = tooltip::get(&root, "Count").and_then(number).unwrap_or(0);
    let damage = tooltip::get(&root, "Damage").and_then(number).unwrap_or(0);
    let nbt = match tooltip::get(&root, "tag") {
        Some(Tag::Compound(c)) => Some(Nbt {
            name: String::new(),
            root: c.clone(),
        }),
        _ => None,
    };
    Some(ItemStack {
        id,
        count: count as i8,
        damage: damage.max(0) as i16,
        nbt,
    })
}

fn number(t: &Tag) -> Option<i64> {
    match *t {
        Tag::Byte(v) => Some(i64::from(v)),
        Tag::Short(v) => Some(i64::from(v)),
        Tag::Int(v) => Some(i64::from(v)),
        Tag::Long(v) => Some(v),
        _ => None,
    }
}

/// The `show_achievement` tooltip: name, type and description, or the invalid text.
// shortcut: every achievement name is green (vanilla colors the special ones gold) and the inventory key is `E`.
fn stat_lines(value: &[Styled], texts: &Texts, m: &dyn Metrics) -> Vec<String> {
    let id = unformatted(value);
    if !id.starts_with("achievement.") && !id.starts_with("stat.") || texts.tr(&id) == id {
        return vec![format!("{RED}Invalid statistic/achievement!")];
    }
    let achievement = id.starts_with("achievement.");
    let name = texts.tr(&id);
    let name = if achievement {
        format!("\u{a7}a{name}\u{a7}r")
    } else {
        format!("{name}\u{a7}r")
    };
    let kind = if achievement {
        "achievement"
    } else {
        "statistic"
    };
    let mut out = vec![
        name,
        format!(
            "\u{a7}o{}\u{a7}r",
            texts.tr(&format!("stats.tooltip.type.{kind}"))
        ),
    ];
    if achievement {
        let desc = texts.fmt(&format!("{id}.desc"), &["E"]);
        out.extend(wrap(m, &desc, 150));
    }
    out
}

/// `JsonToNBT.getTagFromJson` for the forms `ItemStack.writeToNBT` and the commands produce.
fn parse(s: &str) -> Option<Tag> {
    let mut p = Parser {
        s: s.trim(),
        pos: 0,
    };
    let tag = p.value()?;
    (p.pos == p.s.len()).then_some(tag)
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(c);
        if hit {
            self.pos += c.len_utf8();
        }
        hit
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
    }

    fn value(&mut self) -> Option<Tag> {
        self.space();
        let tag = match self.peek()? {
            '{' => Tag::Compound(self.compound()?),
            '[' => self.list()?,
            '"' => Tag::String(self.quoted()?),
            _ => scalar(self.bare()),
        };
        self.space();
        Some(tag)
    }

    fn compound(&mut self) -> Option<Compound> {
        self.eat('{');
        let mut out = Compound::new();
        self.space();
        if self.eat('}') {
            return Some(out);
        }
        loop {
            self.space();
            let key = if self.peek()? == '"' {
                self.quoted()?
            } else {
                self.bare().to_owned()
            };
            self.space();
            if key.is_empty() || !self.eat(':') {
                return None;
            }
            out.push((key, self.value()?));
            if self.eat('}') {
                return Some(out);
            }
            if !self.eat(',') {
                return None;
            }
        }
    }

    fn list(&mut self) -> Option<Tag> {
        self.eat('[');
        let mut items = Vec::new();
        self.space();
        if self.eat(']') {
            return Some(Tag::List(items));
        }
        loop {
            self.space();
            // `[0:a,1:b]`: vanilla ignores the indices.
            let save = self.pos;
            let _ = self.bare();
            self.space();
            if !self.eat(':') {
                self.pos = save;
            }
            items.push(self.value()?);
            if self.eat(']') {
                break;
            }
            if !self.eat(',') {
                return None;
            }
        }
        if let Some(ints) = items
            .iter()
            .map(|t| if let Tag::Int(v) = t { Some(*v) } else { None })
            .collect::<Option<Vec<i32>>>()
        {
            return Some(Tag::IntArray(ints));
        }
        let first = std::mem::discriminant(&items[0]);
        items
            .iter()
            .all(|t| std::mem::discriminant(t) == first)
            .then_some(Tag::List(items))
    }

    fn quoted(&mut self) -> Option<String> {
        self.eat('"');
        let mut out = String::new();
        loop {
            match self.peek()? {
                '"' => {
                    self.pos += 1;
                    return Some(out);
                }
                '\\' => {
                    self.pos += 1;
                    out.push(self.peek()?);
                    self.pos += self.peek()?.len_utf8();
                }
                c => {
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn bare(&mut self) -> &str {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| !matches!(c, ',' | ':' | '}' | ']' | '{' | '[' | '"'))
        {
            self.pos += self.peek().map_or(0, char::len_utf8);
        }
        self.s[start..self.pos].trim()
    }
}

fn digits(b: &str) -> bool {
    let t = b.strip_prefix(['-', '+']).unwrap_or(b);
    !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit())
}

fn decimal(b: &str) -> bool {
    let t = b.strip_prefix(['-', '+']).unwrap_or(b);
    t.bytes().any(|c| c.is_ascii_digit())
        && t.bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && t.bytes().filter(|&c| c == b'.').count() <= 1
}

/// A number with a type suffix such as `1b` or `1.5f`.
fn suffixed(body: &str, suffix: &str) -> Option<Tag> {
    match suffix {
        "b" | "B" if digits(body) => body.parse().map(Tag::Byte).ok(),
        "s" | "S" if digits(body) => body.parse().map(Tag::Short).ok(),
        "l" | "L" if digits(body) => body.parse().map(Tag::Long).ok(),
        "f" | "F" if decimal(body) => body.parse().map(Tag::Float).ok(),
        "d" | "D" if decimal(body) => body.parse().map(Tag::Double).ok(),
        _ => None,
    }
}

/// `JsonToNBT.Any.parse`: the suffixed numbers, plain ints, booleans, else a string.
fn scalar(s: &str) -> Tag {
    let cut = s.char_indices().last().map_or(0, |(i, _)| i);
    let (body, suffix) = s.split_at(cut);
    suffixed(body, suffix)
        .or_else(|| match s {
            _ if digits(s) => s.parse().map(Tag::Int).ok(),
            _ if decimal(s) && s.contains('.') => s.parse().map(Tag::Double).ok(),
            _ if s.eq_ignore_ascii_case("true") => Some(Tag::Byte(1)),
            _ if s.eq_ignore_ascii_case("false") => Some(Tag::Byte(0)),
            _ => None,
        })
        .unwrap_or_else(|| Tag::String(s.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_net_client::styled;

    struct Mono;
    impl Metrics for Mono {
        fn width(&self, s: &str) -> i32 {
            6 * s.chars().filter(|&c| c != '\u{a7}').count() as i32
        }
    }

    fn hover(action: HoverAction, text: &str) -> Hover {
        Hover {
            action,
            value: styled(&format!("{{\"text\":{text:?}}}")),
        }
    }

    #[test]
    fn show_text_splits_lines_and_keeps_colors() {
        let h = Hover {
            action: HoverAction::ShowText,
            value: styled(r#"{"text":"a\nb","color":"red"}"#),
        };
        assert_eq!(
            lines(&h, &Texts::default(), false, &Mono),
            ["\u{a7}ca", "b\u{a7}r"]
        );
    }

    #[test]
    fn show_item_reads_snbt_like_vanilla() {
        let h = hover(
            HoverAction::ShowItem,
            "{id:276s,Count:1b,Damage:0s,tag:{display:{Name:\"Fang\"}}}",
        );
        let l = lines(&h, &Texts::default(), false, &Mono);
        assert!(l[0].contains("Fang"), "{l:?}");
        for bad in ["{id:276s", "{id:0s,Count:1b}", "nope"] {
            let h = hover(HoverAction::ShowItem, bad);
            assert_eq!(
                lines(&h, &Texts::default(), false, &Mono),
                ["\u{a7}cInvalid Item!"],
                "{bad}"
            );
        }
    }

    #[test]
    fn snbt_forms() {
        let Some(Tag::Compound(c)) =
            parse("{a:1b,b:2s,c:3,d:4L,e:1.5f,f:true,h:[1,2],i:[\"a\",\"b\"],j:\"q\\\"\"}")
        else {
            panic!("parse")
        };
        let get = |k| tooltip::get(&c, k).cloned();
        assert_eq!(get("a"), Some(Tag::Byte(1)));
        assert_eq!(get("b"), Some(Tag::Short(2)));
        assert_eq!(get("c"), Some(Tag::Int(3)));
        assert_eq!(get("d"), Some(Tag::Long(4)));
        assert_eq!(get("e"), Some(Tag::Float(1.5)));
        assert_eq!(get("f"), Some(Tag::Byte(1)));
        assert_eq!(get("h"), Some(Tag::IntArray(vec![1, 2])));
        assert_eq!(
            get("i"),
            Some(Tag::List(vec![
                Tag::String("a".into()),
                Tag::String("b".into())
            ]))
        );
        assert_eq!(get("j"), Some(Tag::String("q\"".into())));
    }

    #[test]
    fn snbt_skips_multibyte_whitespace() {
        assert!(matches!(
            parse("{id:1,\u{a0}Count:1b}"),
            Some(Tag::Compound(_))
        ));
        assert!(matches!(
            parse("{id:1,\u{3000}Count:1b}"),
            Some(Tag::Compound(_))
        ));
        assert!(matches!(
            parse("{id:1,Count:\u{e9}}"),
            Some(Tag::Compound(_))
        ));
    }

    #[test]
    fn achievements_use_the_language_texts() {
        let t = Texts::parse(
            "achievement.mineWood=Getting Wood\nachievement.mineWood.desc=Attack a tree\nstats.tooltip.type.achievement=Achievement\n",
            "",
        );
        let h = hover(HoverAction::ShowAchievement, "achievement.mineWood");
        assert_eq!(
            lines(&h, &t, false, &Mono),
            [
                "\u{a7}aGetting Wood\u{a7}r",
                "\u{a7}oAchievement\u{a7}r",
                "Attack a tree"
            ]
        );
        let h = hover(HoverAction::ShowAchievement, "achievement.nope");
        assert_eq!(
            lines(&h, &t, false, &Mono),
            ["\u{a7}cInvalid statistic/achievement!"]
        );
    }
}
