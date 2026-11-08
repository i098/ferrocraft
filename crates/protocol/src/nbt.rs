//! Uncompressed NBT as sent inside packets (named root compound).

use crate::codec::{Error, Reader, Result, Wire};

/// Ordered compound; order is kept so decode then encode is byte-exact.
pub type Compound = Vec<(String, Tag)>;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// Elements must share one tag type. An empty list is written with element type 0.
    List(Vec<Tag>),
    Compound(Compound),
    IntArray(Vec<i32>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Nbt {
    pub name: String,
    pub root: Compound,
}

const MAX_DEPTH: usize = 512;

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
        }
    }

    fn read(id: u8, r: &mut Reader, depth: usize) -> Result<Tag> {
        if depth > MAX_DEPTH {
            return Err(Error::Nbt("nesting too deep"));
        }
        Ok(match id {
            1 => Tag::Byte(Wire::read(r)?),
            2 => Tag::Short(Wire::read(r)?),
            3 => Tag::Int(Wire::read(r)?),
            4 => Tag::Long(Wire::read(r)?),
            5 => Tag::Float(Wire::read(r)?),
            6 => Tag::Double(Wire::read(r)?),
            7 => {
                let n = array_len(r, 1)?;
                Tag::ByteArray(r.take(n)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::String(read_mutf8(r)?),
            9 => read_list(r, depth)?,
            10 => Tag::Compound(read_compound(r, depth)?),
            11 => {
                let n = array_len(r, 4)?;
                Tag::IntArray((0..n).map(|_| i32::read(r)).collect::<Result<_>>()?)
            }
            _ => return Err(Error::Nbt("unknown tag type")),
        })
    }

    fn write(&self, w: &mut Vec<u8>) {
        match self {
            Tag::Byte(v) => v.write(w),
            Tag::Short(v) => v.write(w),
            Tag::Int(v) => v.write(w),
            Tag::Long(v) => v.write(w),
            Tag::Float(v) => v.write(w),
            Tag::Double(v) => v.write(w),
            Tag::ByteArray(v) => {
                (v.len() as i32).write(w);
                w.extend(v.iter().map(|&b| b as u8));
            }
            Tag::String(s) => write_mutf8(s, w),
            Tag::List(items) => {
                let ty = items.first().map_or(0, Tag::id);
                debug_assert!(items.iter().all(|t| t.id() == ty), "heterogeneous nbt list");
                ty.write(w);
                (items.len() as i32).write(w);
                items.iter().for_each(|t| t.write(w));
            }
            Tag::Compound(c) => write_compound(c, w),
            Tag::IntArray(v) => {
                (v.len() as i32).write(w);
                v.iter().for_each(|x| x.write(w));
            }
        }
    }
}

fn read_list(r: &mut Reader, depth: usize) -> Result<Tag> {
    let ty = u8::read(r)?;
    let n = i32::read(r)?.max(0) as usize;
    if n > 0 && ty == 0 {
        return Err(Error::Nbt("list without element type"));
    }
    if n > r.remaining() {
        return Err(Error::Eof);
    }
    Ok(Tag::List(
        (0..n)
            .map(|_| Tag::read(ty, r, depth + 1))
            .collect::<Result<_>>()?,
    ))
}

fn array_len(r: &mut Reader, elem_size: usize) -> Result<usize> {
    let n = i32::read(r)?;
    let n = usize::try_from(n).map_err(|_| Error::NegativeLength(i64::from(n)))?;
    if n.saturating_mul(elem_size) > r.remaining() {
        return Err(Error::Eof);
    }
    Ok(n)
}

fn read_compound(r: &mut Reader, depth: usize) -> Result<Compound> {
    let mut out = Vec::new();
    loop {
        let id = u8::read(r)?;
        if id == 0 {
            return Ok(out);
        }
        let name = read_mutf8(r)?;
        out.push((name, Tag::read(id, r, depth + 1)?));
    }
}

fn write_compound(c: &Compound, w: &mut Vec<u8>) {
    for (name, tag) in c {
        tag.id().write(w);
        write_mutf8(name, w);
        tag.write(w);
    }
    w.push(0);
}

/// Java modified UTF-8 with a `u16` byte-length prefix.
fn read_mutf8(r: &mut Reader) -> Result<String> {
    let n = usize::from(u16::read(r)?);
    let b = r.take(n)?;
    let mut units: Vec<u16> = Vec::with_capacity(n);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let cont = |k: usize| -> Result<u16> {
            match b.get(i + k) {
                Some(&x) if x & 0xC0 == 0x80 => Ok(u16::from(x & 0x3F)),
                _ => Err(Error::InvalidUtf8),
            }
        };
        let (unit, len) = match c {
            0x00..=0x7F => (u16::from(c), 1),
            0xC0..=0xDF => (u16::from(c & 0x1F) << 6 | cont(1)?, 2),
            0xE0..=0xEF => (u16::from(c & 0x0F) << 12 | cont(1)? << 6 | cont(2)?, 3),
            _ => return Err(Error::InvalidUtf8),
        };
        units.push(unit);
        i += len;
    }
    String::from_utf16(&units).map_err(|_| Error::InvalidUtf8)
}

fn write_mutf8(s: &str, w: &mut Vec<u8>) {
    let start = w.len();
    w.extend_from_slice(&[0, 0]);
    for u in s.encode_utf16() {
        match u {
            0x01..=0x7F => w.push(u as u8),
            0 | 0x80..=0x7FF => {
                w.push(0xC0 | (u >> 6) as u8);
                w.push(0x80 | (u & 0x3F) as u8);
            }
            _ => {
                w.push(0xE0 | (u >> 12) as u8);
                w.push(0x80 | ((u >> 6) & 0x3F) as u8);
                w.push(0x80 | (u & 0x3F) as u8);
            }
        }
    }
    let len = w.len() - start - 2;
    let len = u16::try_from(len).expect("nbt string longer than 65535 bytes");
    w[start..start + 2].copy_from_slice(&len.to_be_bytes());
}

impl Wire for Nbt {
    fn read(r: &mut Reader) -> Result<Self> {
        if u8::read(r)? != 10 {
            return Err(Error::Nbt("root tag must be a compound"));
        }
        let name = read_mutf8(r)?;
        Ok(Nbt {
            name,
            root: read_compound(r, 0)?,
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        w.push(10);
        write_mutf8(&self.name, w);
        write_compound(&self.root, w);
    }
}
