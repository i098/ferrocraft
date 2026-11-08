//! Primitive wire types for protocol 47, plus the macros that define packets.

use crate::nbt::Nbt;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Eof,
    VarIntTooLong,
    InvalidUtf8,
    StringTooLong(usize),
    NegativeLength(i64),
    UnknownPacket(i32),
    InvalidValue(&'static str),
    TrailingBytes(usize),
    Nbt(&'static str),
    FrameTooLarge(usize),
    BadCompression(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Eof => write!(f, "unexpected end of data"),
            Error::VarIntTooLong => write!(f, "varint too long"),
            Error::InvalidUtf8 => write!(f, "invalid utf-8 string"),
            Error::StringTooLong(n) => write!(f, "string too long ({n})"),
            Error::NegativeLength(n) => write!(f, "negative length {n}"),
            Error::UnknownPacket(id) => write!(f, "unknown packet id {id:#04x}"),
            Error::InvalidValue(what) => write!(f, "invalid value: {what}"),
            Error::TrailingBytes(n) => write!(f, "{n} trailing bytes after packet"),
            Error::Nbt(what) => write!(f, "bad nbt: {what}"),
            Error::FrameTooLarge(n) => write!(f, "frame too large ({n})"),
            Error::BadCompression(m) => write!(f, "bad compression: {m}"),
            Error::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Cursor over one whole packet body held in memory.
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len()
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.buf.len() {
            return Err(Error::Eof);
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }

    pub fn peek_u8(&self) -> Result<u8> {
        self.buf.first().copied().ok_or(Error::Eof)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.buf)
    }

    pub fn finish(&self) -> Result<()> {
        match self.buf.len() {
            0 => Ok(()),
            n => Err(Error::TrailingBytes(n)),
        }
    }
}

/// A type with a fixed wire encoding.
pub trait Wire: Sized {
    fn read(r: &mut Reader) -> Result<Self>;
    fn write(&self, w: &mut Vec<u8>);
}

macro_rules! int_wire {
    ($($t:ty),*) => {$(
        impl Wire for $t {
            fn read(r: &mut Reader) -> Result<Self> {
                let b = r.take(std::mem::size_of::<$t>())?;
                Ok(<$t>::from_be_bytes(b.try_into().unwrap()))
            }
            fn write(&self, w: &mut Vec<u8>) {
                w.extend_from_slice(&self.to_be_bytes());
            }
        }
    )*};
}
int_wire!(u8, i8, u16, i16, i32, i64, u64, f32, f64);

impl Wire for bool {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(u8::read(r)? != 0)
    }
    fn write(&self, w: &mut Vec<u8>) {
        w.push(*self as u8);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarInt(pub i32);

impl Wire for VarInt {
    fn read(r: &mut Reader) -> Result<Self> {
        let mut v = 0u32;
        for i in 0..5 {
            let b = u8::read(r)?;
            v |= u32::from(b & 0x7F) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(VarInt(v as i32));
            }
        }
        Err(Error::VarIntTooLong)
    }
    fn write(&self, w: &mut Vec<u8>) {
        let mut v = self.0 as u32;
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                w.push(b);
                return;
            }
            w.push(b | 0x80);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarLong(pub i64);

impl Wire for VarLong {
    fn read(r: &mut Reader) -> Result<Self> {
        let mut v = 0u64;
        for i in 0..10 {
            let b = u8::read(r)?;
            v |= u64::from(b & 0x7F) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(VarLong(v as i64));
            }
        }
        Err(Error::VarIntTooLong)
    }
    fn write(&self, w: &mut Vec<u8>) {
        let mut v = self.0 as u64;
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                w.push(b);
                return;
            }
            w.push(b | 0x80);
        }
    }
}

/// Reads a non-negative VarInt length.
fn read_len(r: &mut Reader) -> Result<usize> {
    let n = VarInt::read(r)?.0;
    usize::try_from(n).map_err(|_| Error::NegativeLength(i64::from(n)))
}

/// Maximum string length in UTF-16 units, as in vanilla.
const MAX_STRING_UNITS: usize = 32767;

impl Wire for String {
    fn read(r: &mut Reader) -> Result<Self> {
        let len = read_len(r)?;
        if len > MAX_STRING_UNITS * 4 {
            return Err(Error::StringTooLong(len));
        }
        let s = std::str::from_utf8(r.take(len)?).map_err(|_| Error::InvalidUtf8)?;
        let units = s.encode_utf16().count();
        if units > MAX_STRING_UNITS {
            return Err(Error::StringTooLong(units));
        }
        Ok(s.to_owned())
    }
    fn write(&self, w: &mut Vec<u8>) {
        VarInt(self.len() as i32).write(w);
        w.extend_from_slice(self.as_bytes());
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn read(r: &mut Reader) -> Result<Self> {
        let len = read_len(r)?;
        read_items(r, len)
    }
    fn write(&self, w: &mut Vec<u8>) {
        VarInt(self.len() as i32).write(w);
        self.iter().for_each(|x| x.write(w));
    }
}

fn read_items<T: Wire>(r: &mut Reader, len: usize) -> Result<Vec<T>> {
    // Every element is at least one byte, so a larger count cannot be valid.
    if len > r.remaining() {
        return Err(Error::Eof);
    }
    (0..len).map(|_| T::read(r)).collect()
}

/// Vector with an `i16` count prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct Vec16<T>(pub Vec<T>);

impl<T: Wire> Wire for Vec16<T> {
    fn read(r: &mut Reader) -> Result<Self> {
        let n = i16::read(r)?;
        let n = usize::try_from(n).map_err(|_| Error::NegativeLength(i64::from(n)))?;
        read_items(r, n).map(Vec16)
    }
    fn write(&self, w: &mut Vec<u8>) {
        (self.0.len() as i16).write(w);
        self.0.iter().for_each(|x| x.write(w));
    }
}

/// Vector with an `i32` count prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct Vec32<T>(pub Vec<T>);

impl<T: Wire> Wire for Vec32<T> {
    fn read(r: &mut Reader) -> Result<Self> {
        let n = i32::read(r)?;
        let n = usize::try_from(n).map_err(|_| Error::NegativeLength(i64::from(n)))?;
        read_items(r, n).map(Vec32)
    }
    fn write(&self, w: &mut Vec<u8>) {
        (self.0.len() as i32).write(w);
        self.0.iter().for_each(|x| x.write(w));
    }
}

impl<T: Wire, const N: usize> Wire for [T; N] {
    fn read(r: &mut Reader) -> Result<Self> {
        let v: Vec<T> = (0..N).map(|_| T::read(r)).collect::<Result<_>>()?;
        Ok(v.try_into().unwrap_or_else(|_| unreachable!()))
    }
    fn write(&self, w: &mut Vec<u8>) {
        self.iter().for_each(|x| x.write(w));
    }
}

/// `bool`-prefixed optional value.
#[derive(Debug, Clone, PartialEq)]
pub struct Opt<T>(pub Option<T>);

impl<T: Wire> Wire for Opt<T> {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(Opt(if bool::read(r)? {
            Some(T::read(r)?)
        } else {
            None
        }))
    }
    fn write(&self, w: &mut Vec<u8>) {
        self.0.is_some().write(w);
        if let Some(v) = &self.0 {
            v.write(w);
        }
    }
}

/// VarInt-length-prefixed byte array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Wire for Bytes {
    fn read(r: &mut Reader) -> Result<Self> {
        let len = read_len(r)?;
        Ok(Bytes(r.take(len)?.to_vec()))
    }
    fn write(&self, w: &mut Vec<u8>) {
        VarInt(self.0.len() as i32).write(w);
        w.extend_from_slice(&self.0);
    }
}

/// All bytes left in the packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rest(pub Vec<u8>);

impl Wire for Rest {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(Rest(r.rest().to_vec()))
    }
    fn write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(&self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uuid(pub u128);

impl Wire for Uuid {
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(Uuid(u128::from_be_bytes(r.take(16)?.try_into().unwrap())))
    }
    fn write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(&self.0.to_be_bytes());
    }
}

/// Block position, packed into one `u64` (x: 26 bits, y: 12 bits, z: 26 bits, signed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Wire for Position {
    fn read(r: &mut Reader) -> Result<Self> {
        let v = u64::read(r)? as i64;
        Ok(Position {
            x: (v >> 38) as i32,
            y: (v << 26 >> 52) as i32,
            z: (v << 38 >> 38) as i32,
        })
    }
    fn write(&self, w: &mut Vec<u8>) {
        let v = ((self.x as u64 & 0x3FF_FFFF) << 38)
            | ((self.y as u64 & 0xFFF) << 26)
            | (self.z as u64 & 0x3FF_FFFF);
        v.write(w);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    pub id: i16,
    pub count: i8,
    pub damage: i16,
    pub nbt: Option<Nbt>,
}

/// Inventory slot: `None` is an empty slot (item id -1).
pub type Slot = Option<ItemStack>;

impl Wire for Option<ItemStack> {
    fn read(r: &mut Reader) -> Result<Self> {
        let id = i16::read(r)?;
        if id < 0 {
            return Ok(None);
        }
        Ok(Some(ItemStack {
            id,
            count: i8::read(r)?,
            damage: i16::read(r)?,
            nbt: Option::<Nbt>::read(r)?,
        }))
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            None => (-1i16).write(w),
            Some(s) => {
                s.id.write(w);
                s.count.write(w);
                s.damage.write(w);
                s.nbt.write(w);
            }
        }
    }
}

/// Root compound, or a single `TAG_End` byte for "no data".
impl Wire for Option<Nbt> {
    fn read(r: &mut Reader) -> Result<Self> {
        if r.peek_u8()? == 0 {
            r.take(1)?;
            return Ok(None);
        }
        Nbt::read(r).map(Some)
    }
    fn write(&self, w: &mut Vec<u8>) {
        match self {
            None => w.push(0),
            Some(n) => n.write(w),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Float(f32),
    String(String),
    Slot(Slot),
    /// Three plain `i32`s, not the packed form.
    BlockPos(Position),
    Rotation(f32, f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetaEntry {
    pub index: u8,
    pub value: MetaValue,
}

/// Entity metadata list, terminated on the wire by `0x7F`.
#[derive(Debug, Clone, PartialEq)]
pub struct Metadata(pub Vec<MetaEntry>);

impl Wire for Metadata {
    fn read(r: &mut Reader) -> Result<Self> {
        let mut out = Vec::new();
        loop {
            let b = u8::read(r)?;
            if b == 0x7F {
                return Ok(Metadata(out));
            }
            let value = match b >> 5 {
                0 => MetaValue::Byte(Wire::read(r)?),
                1 => MetaValue::Short(Wire::read(r)?),
                2 => MetaValue::Int(Wire::read(r)?),
                3 => MetaValue::Float(Wire::read(r)?),
                4 => MetaValue::String(Wire::read(r)?),
                5 => MetaValue::Slot(Wire::read(r)?),
                6 => MetaValue::BlockPos(Position {
                    x: Wire::read(r)?,
                    y: Wire::read(r)?,
                    z: Wire::read(r)?,
                }),
                7 => MetaValue::Rotation(Wire::read(r)?, Wire::read(r)?, Wire::read(r)?),
                _ => return Err(Error::InvalidValue("metadata type")),
            };
            out.push(MetaEntry {
                index: b & 0x1F,
                value,
            });
        }
    }
    fn write(&self, w: &mut Vec<u8>) {
        for e in &self.0 {
            w.push(e.value.type_id() << 5 | (e.index & 0x1F));
            e.value.write_body(w);
        }
        w.push(0x7F);
    }
}

impl MetaValue {
    fn type_id(&self) -> u8 {
        match self {
            MetaValue::Byte(_) => 0,
            MetaValue::Short(_) => 1,
            MetaValue::Int(_) => 2,
            MetaValue::Float(_) => 3,
            MetaValue::String(_) => 4,
            MetaValue::Slot(_) => 5,
            MetaValue::BlockPos(_) => 6,
            MetaValue::Rotation(..) => 7,
        }
    }

    fn write_body(&self, w: &mut Vec<u8>) {
        match self {
            MetaValue::Byte(v) => v.write(w),
            MetaValue::Short(v) => v.write(w),
            MetaValue::Int(v) => v.write(w),
            MetaValue::Float(v) => v.write(w),
            MetaValue::String(v) => v.write(w),
            MetaValue::Slot(v) => v.write(w),
            MetaValue::BlockPos(p) => {
                p.x.write(w);
                p.y.write(w);
                p.z.write(w);
            }
            MetaValue::Rotation(a, b, c) => {
                a.write(w);
                b.write(w);
                c.write(w);
            }
        }
    }
}

/// Plain struct with a field-by-field wire encoding.
macro_rules! wire_struct {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $f:ident : $t:ty),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name { $($(#[$fm])* pub $f: $t),* }

        impl $crate::codec::Wire for $name {
            #[allow(unused_variables)]
            fn read(r: &mut $crate::codec::Reader) -> $crate::codec::Result<Self> {
                Ok($name { $($f: <$t as $crate::codec::Wire>::read(r)?),* })
            }
            #[allow(unused_variables)]
            fn write(&self, w: &mut Vec<u8>) {
                $($crate::codec::Wire::write(&self.$f, w);)*
            }
        }
    };
}

/// A packet struct: fixed packet id plus a body encoding.
pub trait PacketData: Wire {
    const ID: i32;
}

/// Packet with a plain field list.
macro_rules! packet {
    ($(#[$m:meta])* $name:ident = $id:literal { $($(#[$fm:meta])* $f:ident : $t:ty),* $(,)? }) => {
        wire_struct!($(#[$m])* $name { $($(#[$fm])* $f: $t),* });
        impl $crate::codec::PacketData for $name { const ID: i32 = $id; }
    };
}

/// Attach a packet id to a type that has a hand-written `Wire` impl.
macro_rules! packet_id {
    ($name:ident = $id:literal) => {
        impl $crate::codec::PacketData for $name {
            const ID: i32 = $id;
        }
    };
}

/// All packets of one state and direction.
pub trait PacketSet: Sized {
    fn id(&self) -> i32;
    fn write_body(&self, w: &mut Vec<u8>);
    fn read_body(id: i32, r: &mut Reader) -> Result<Self>;

    /// Packet id followed by the body, without the frame header.
    fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        VarInt(self.id()).write(&mut w);
        self.write_body(&mut w);
        w
    }

    fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = Reader::new(buf);
        let id = VarInt::read(&mut r)?.0;
        let p = Self::read_body(id, &mut r)?;
        r.finish()?;
        Ok(p)
    }
}

macro_rules! packet_set {
    ($(#[$m:meta])* $name:ident { $($var:ident),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq)]
        #[allow(clippy::large_enum_variant)]
        pub enum $name { $($var($var)),* }

        impl $crate::codec::PacketSet for $name {
            fn id(&self) -> i32 {
                match self { $(Self::$var(_) => <$var as $crate::codec::PacketData>::ID),* }
            }
            fn write_body(&self, w: &mut Vec<u8>) {
                match self { $(Self::$var(p) => $crate::codec::Wire::write(p, w)),* }
            }
            fn read_body(id: i32, r: &mut $crate::codec::Reader) -> $crate::codec::Result<Self> {
                $(
                    if id == <$var as $crate::codec::PacketData>::ID {
                        return Ok(Self::$var(<$var as $crate::codec::Wire>::read(r)?));
                    }
                )*
                Err($crate::codec::Error::UnknownPacket(id))
            }
        }

        $(impl From<$var> for $name {
            fn from(p: $var) -> Self { Self::$var(p) }
        })*
    };
}
