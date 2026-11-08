//! Flattens a chat component (JSON string) to plain text.

use serde_json::Value;

/// Plain text of a chat component. Text that is not JSON is returned unchanged.
pub fn plain_text(json: &str) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(v) => {
            let mut out = String::new();
            flatten(&v, &mut out);
            out
        }
        Err(_) => json.to_owned(),
    }
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

fn flatten(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => out.push_str(s),
        Value::Array(items) => items.iter().for_each(|i| flatten(i, out)),
        Value::Object(o) => {
            if let Some(t) = o.get("text").and_then(Value::as_str) {
                out.push_str(t);
            } else if let Some(key) = o.get("translate").and_then(Value::as_str) {
                let args = o
                    .get("with")
                    .and_then(Value::as_array)
                    .map_or(&[][..], |a| a);
                translate(key, args, out);
            }
            if let Some(extra) = o.get("extra") {
                flatten(extra, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn translate(key: &str, args: &[Value], out: &mut String) {
    let Some(pattern) = translation(key) else {
        out.push_str(key);
        for a in args {
            out.push(' ');
            flatten(a, out);
        }
        return;
    };
    let mut next = args.iter();
    let mut rest = pattern;
    while let Some(i) = rest.find("%s") {
        out.push_str(&rest[..i]);
        if let Some(a) = next.next() {
            flatten(a, out);
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
}

#[cfg(test)]
mod tests {
    use super::plain_text;

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
}
