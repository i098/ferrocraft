//! `GuiChat` tab completion: `autocompletePlayerNames`, `onAutocompleteResponse` and the Tab cycle.
//!
//! The chat field has no cursor movement, so the cursor is always at the end of the text and the
//! word before it starts after the last space (`GuiTextField.func_146197_a(-1, cursor, false)`).

/// The chat field holds 100 UTF-16 units.
const MAX_CHARS: usize = 100;

#[derive(Debug, Default)]
pub struct Autocomplete {
    /// `waitingOnAutocomplete`: a request is out and its reply still counts.
    waiting: bool,
    /// `playerNamesFound`: the field holds one of `found`, and Tab goes to the next.
    names_found: bool,
    index: usize,
    found: Vec<String>,
    /// The text before the cursor of a request the engine must send as `C14PacketTabComplete`.
    request: Option<String>,
}

impl Autocomplete {
    /// `GuiChat.keyTyped` start: any key ends the wait; Tab asks or cycles, other keys end the cycle.
    /// Returns the chat line the cycle prints (`printChatMessageWithOptionalDeletion(.., 1)`).
    pub fn key(&mut self, tab: bool, field: &mut String) -> Option<String> {
        self.waiting = false;
        if !tab {
            self.names_found = false;
            return None;
        }
        if !self.names_found {
            // `sendAutocompleteRequest`: nothing to complete yet, the reply fills `found`.
            self.found.clear();
            self.index = 0;
            if !field.is_empty() {
                self.request = Some(field.clone());
                self.waiting = true;
            }
            return None;
        }
        self.cycle(field)
    }

    /// The request of the last Tab, for the engine to send.
    pub fn take_request(&mut self) -> Option<String> {
        self.request.take()
    }

    /// `onAutocompleteResponse`: complete the common prefix, or start cycling the names.
    pub fn response(&mut self, matches: &[String], field: &mut String) -> Option<String> {
        if !self.waiting {
            return None;
        }
        self.names_found = false;
        self.found = matches.iter().filter(|s| !s.is_empty()).cloned().collect();
        let start = word_start(field);
        let prefix = common_prefix(matches);
        if !prefix.is_empty() && field[start..].to_lowercase() != prefix.to_lowercase() {
            field.truncate(start);
            write(field, &prefix);
            None
        } else if self.found.is_empty() {
            None
        } else {
            self.names_found = true;
            self.cycle(field)
        }
    }

    /// The `playerNamesFound` half of `autocompletePlayerNames`.
    fn cycle(&mut self, field: &mut String) -> Option<String> {
        field.truncate(word_start(field));
        if self.index >= self.found.len() {
            self.index = 0;
        }
        let note = (self.found.len() > 1).then(|| self.found.join(", "));
        write(field, &self.found[self.index]);
        self.index += 1;
        note
    }
}

/// Start of the word before the cursor.
fn word_start(field: &str) -> usize {
    field.rfind(' ').map_or(0, |i| i + 1)
}

/// `GuiTextField.writeText`: the text that still fits.
fn write(field: &mut String, text: &str) {
    for c in text.chars() {
        if field.encode_utf16().count() + c.len_utf16() <= MAX_CHARS {
            field.push(c);
        }
    }
}

/// `StringUtils.getCommonPrefix`: empty when there is no reply or any reply is empty.
fn common_prefix(all: &[String]) -> String {
    let Some(first) = all.first() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for s in &all[1..] {
        let same = prefix
            .iter()
            .zip(s.chars())
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(same);
    }
    prefix.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn slash_command_completes_from_the_reply() {
        let mut ac = Autocomplete::default();
        let mut field = "/he".to_string();
        assert_eq!(ac.key(true, &mut field), None);
        assert_eq!(ac.take_request().as_deref(), Some("/he"));
        // One reply: the common prefix is the reply itself and replaces the word.
        assert_eq!(ac.response(&names(&["/help"]), &mut field), None);
        assert_eq!(field, "/help");
    }

    #[test]
    fn names_cycle_and_list_in_chat() {
        let mut ac = Autocomplete::default();
        let mut field = "hi Al".to_string();
        ac.key(true, &mut field);
        assert_eq!(ac.take_request().as_deref(), Some("hi Al"));
        // No common prefix beyond what is typed: list the names and write the first.
        let note = ac.response(&names(&["Alex", "Alice"]), &mut field);
        assert_eq!(note.as_deref(), Some("Alex, Alice"));
        assert_eq!(field, "hi Alex");
        // Tab again: the next name, and no new request.
        ac.key(true, &mut field);
        assert_eq!(field, "hi Alice");
        assert_eq!(ac.take_request(), None);
        ac.key(true, &mut field);
        assert_eq!(field, "hi Alex");
        // Another key ends the cycle: the next Tab asks again.
        ac.key(false, &mut field);
        ac.key(true, &mut field);
        assert_eq!(ac.take_request().as_deref(), Some("hi Alex"));
    }

    #[test]
    fn common_prefix_is_written_first() {
        let mut ac = Autocomplete::default();
        let mut field = "/g".to_string();
        ac.key(true, &mut field);
        assert_eq!(
            ac.response(&names(&["/gamemode", "/gamerule"]), &mut field),
            None
        );
        assert_eq!(field, "/game");
    }

    #[test]
    fn replies_without_a_request_or_empty_are_ignored() {
        let mut ac = Autocomplete::default();
        let mut field = "x".to_string();
        assert_eq!(ac.response(&names(&["xa"]), &mut field), None);
        assert_eq!(field, "x");
        ac.key(true, &mut field);
        // A key typed after the Tab drops the reply, like `waitingOnAutocomplete`.
        ac.key(false, &mut field);
        assert_eq!(ac.response(&names(&["xa"]), &mut field), None);
        assert_eq!(field, "x");
        let mut empty = String::new();
        ac.take_request();
        assert_eq!(ac.take_request(), None);
        ac.key(true, &mut empty);
        assert_eq!(ac.take_request(), None);
    }
}
