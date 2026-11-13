//! Chat lines for the mods: Rust sends each received line, Java posts `ClientChatReceivedEvent` and answers
//! whether a mod cancelled it or changed its text. Wire, after the tag `'C'`: u8 kind (0 chat, 1 system,
//! 2 action bar), u32 length and UTF-8 bytes of the formatted text (`§` codes). Reply: u8 verdict (0 keep,
//! 1 cancel, 2 replace), and for 2 the new formatted text in the same string form.

use std::io::{self, Read, Write};

pub(crate) const MAX_TEXT: usize = 1 << 16;

/// The `type` byte of `ClientChatReceivedEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Chat = 0,
    System = 1,
    ActionBar = 2,
}

/// What the mods did to a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    Cancel,
    Replace(String),
}

pub fn write_line(w: &mut impl Write, kind: Kind, text: &str) -> io::Result<()> {
    let len = u32::try_from(text.len()).map_err(|_| io::Error::other("chat line too long"))?;
    let mut b = Vec::with_capacity(6 + text.len());
    b.extend([b'C', kind as u8]);
    b.extend(len.to_be_bytes());
    b.extend(text.as_bytes());
    w.write_all(&b)?;
    w.flush()
}

pub fn read_verdict(r: &mut impl Read) -> io::Result<Verdict> {
    let mut tag = [0; 1];
    r.read_exact(&mut tag)?;
    match tag[0] {
        0 => Ok(Verdict::Keep),
        1 => Ok(Verdict::Cancel),
        2 => {
            let mut n = [0; 4];
            r.read_exact(&mut n)?;
            let len = u32::from_be_bytes(n) as usize;
            if len > MAX_TEXT {
                return Err(io::Error::other(format!("{len} byte chat line")));
            }
            let mut s = vec![0; len];
            r.read_exact(&mut s)?;
            Ok(Verdict::Replace(String::from_utf8_lossy(&s).into_owned()))
        }
        t => Err(io::Error::other(format!("unknown chat verdict {t}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_line_java_reads() {
        let mut out = Vec::new();
        write_line(&mut out, Kind::System, "§aé").unwrap();
        assert_eq!(out, [b'C', 1, 0, 0, 0, 5, 0xc2, 0xa7, b'a', 0xc3, 0xa9]);
    }

    #[test]
    fn reads_the_three_verdicts_and_rejects_the_rest() {
        assert_eq!(read_verdict(&mut &[0u8][..]).unwrap(), Verdict::Keep);
        assert_eq!(read_verdict(&mut &[1u8][..]).unwrap(), Verdict::Cancel);
        assert_eq!(
            read_verdict(&mut &[2u8, 0, 0, 0, 2, b'h', b'i'][..]).unwrap(),
            Verdict::Replace("hi".into())
        );
        assert!(read_verdict(&mut &[3u8][..]).is_err());
        assert!(read_verdict(&mut &[2u8, 0, 0, 0, 2, b'h'][..]).is_err());
    }
}
