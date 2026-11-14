//! The hook table: the vanilla methods that a mod changed, as `HookIndex.java` lists them after Mixin ran. The JVM sends
//! it once, inside the hello, after the brand: u32 count, then per method three `writeUTF` strings (u16 length, bytes):
//! the owner class as an internal name, the method name and the descriptor, all in the SRG names mods see. Rust ports
//! of vanilla behavior ask the table before they run: a method in it belongs to Java.

use std::collections::HashSet;
use std::io::{self, Read};

/// A count above this means a corrupt stream; the 24 OneClient jars change under 100 methods.
const MAX_HOOKS: u32 = 1 << 16;

/// One changed vanilla method.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hook {
    pub owner: String,
    pub name: String,
    pub desc: String,
}

/// Every method that a mod changed. Empty when no mod changed vanilla code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hooks(HashSet<Hook>);

impl Hooks {
    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        let mut count = [0; 4];
        r.read_exact(&mut count)?;
        let count = u32::from_be_bytes(count);
        if count > MAX_HOOKS {
            return Err(io::Error::other("hook table too long"));
        }
        let mut set = HashSet::new();
        for _ in 0..count {
            set.insert(Hook {
                owner: utf(r)?,
                name: utf(r)?,
                desc: utf(r)?,
            });
        }
        Ok(Self(set))
    }

    /// Whether a mod changed the method, for example `("net/minecraft/client/gui/GuiIngame", "func_175180_a", "(F)V")`.
    pub fn changed(&self, owner: &str, name: &str, desc: &str) -> bool {
        self.0
            .iter()
            .any(|h| h.owner == owner && h.name == name && h.desc == desc)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A Java `writeUTF` string. Names and descriptors are ASCII, which modified UTF-8 shares with UTF-8.
fn utf(r: &mut impl Read) -> io::Result<String> {
    let mut len = [0; 2];
    r.read_exact(&mut len)?;
    let mut b = vec![0; usize::from(u16::from_be_bytes(len))];
    r.read_exact(&mut b)?;
    String::from_utf8(b).map_err(|_| io::Error::other("bad hook string"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(hooks: &[(&str, &str, &str)]) -> Vec<u8> {
        let mut b = (hooks.len() as u32).to_be_bytes().to_vec();
        for (owner, name, desc) in hooks {
            for s in [owner, name, desc] {
                b.extend((s.len() as u16).to_be_bytes());
                b.extend(s.as_bytes());
            }
        }
        b
    }

    #[test]
    fn reads_the_list_and_answers_by_method() {
        let b = wire(&[
            (
                "net/minecraft/client/gui/GuiIngame",
                "func_175180_a",
                "(F)V",
            ),
            (
                "net/minecraft/client/gui/GuiNewChat",
                "func_146227_a",
                "(Lnet/minecraft/util/IChatComponent;)V",
            ),
        ]);
        let mut r = &b[..];
        let h = Hooks::read(&mut r).unwrap();
        assert!(r.is_empty());
        assert!(h.changed(
            "net/minecraft/client/gui/GuiIngame",
            "func_175180_a",
            "(F)V"
        ));
        assert!(!h.changed(
            "net/minecraft/client/gui/GuiIngame",
            "func_175180_a",
            "(I)V"
        ));
    }

    #[test]
    fn no_mod_gives_an_empty_table() {
        let h = Hooks::read(&mut &wire(&[])[..]).unwrap();
        assert!(h.is_empty());
    }

    #[test]
    fn a_corrupt_count_or_cut_stream_is_an_error() {
        assert!(Hooks::read(&mut &u32::MAX.to_be_bytes()[..]).is_err());
        let b = wire(&[("a", "b", "c")]);
        assert!(Hooks::read(&mut &b[..b.len() - 1]).is_err());
    }
}
