//! World and entity events for the mods. Rust says what happened; Java posts the Forge event that the same change
//! posts on the real client, and answers nothing. Wire, one message per event: `'E'`, u8 code, then the payload.
//!
//! | code | event | payload |
//! |---|---|---|
//! | `'c'` | `FMLNetworkEvent.ClientConnectedToServerEvent` | none |
//! | `'d'` | `FMLNetworkEvent.ClientDisconnectionFromServerEvent` | none |
//! | `'l'` | `WorldEvent.Load` | none |
//! | `'u'` | `WorldEvent.Unload` | none |
//! | `'j'` | `EntityJoinWorldEvent` | i32 entity id, u8 kind, i32 type, u64 uuid high, u64 uuid low, three f64 position |
//! | `'a'` | `AttackEntityEvent` | i32 id of the attacked entity |
//! | `'r'` | none, the entity left the world | i32 entity id |
//!
//! Kind: 0 the local player, 1 another player, 2 mob (type is the entity type id), 3 object (type is the spawn
//! object id), 4 experience orb, 5 painting. The uuid is set for kind 1 only.

use std::io::{self, Write};

/// What an entity is, as the spawn packet says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spawn {
    /// The local player, from the join packet.
    Own,
    Player(u128),
    Mob(u8),
    Object(i8),
    Orb,
    Painting,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Connect,
    Disconnect,
    WorldLoad,
    WorldUnload,
    EntityJoin {
        id: i32,
        kind: Spawn,
        pos: [f64; 3],
    },
    /// The local player attacks this entity.
    Attack(i32),
    /// The server destroyed this entity.
    Remove(i32),
}

pub fn write(w: &mut impl Write, e: &Event) -> io::Result<()> {
    let mut b = vec![b'E'];
    match *e {
        Event::Connect => b.push(b'c'),
        Event::Disconnect => b.push(b'd'),
        Event::WorldLoad => b.push(b'l'),
        Event::WorldUnload => b.push(b'u'),
        Event::EntityJoin { id, kind, pos } => {
            let (kind, ty, uuid) = match kind {
                Spawn::Own => (0, 0, 0),
                Spawn::Player(u) => (1, 0, u),
                Spawn::Mob(t) => (2, i32::from(t), 0),
                Spawn::Object(t) => (3, i32::from(t), 0),
                Spawn::Orb => (4, 0, 0),
                Spawn::Painting => (5, 0, 0),
            };
            b.push(b'j');
            b.extend(id.to_be_bytes());
            b.push(kind);
            b.extend(ty.to_be_bytes());
            b.extend(uuid.to_be_bytes());
            for v in pos {
                b.extend(v.to_be_bytes());
            }
        }
        Event::Attack(id) => {
            b.push(b'a');
            b.extend(id.to_be_bytes());
        }
        Event::Remove(id) => {
            b.push(b'r');
            b.extend(id.to_be_bytes());
        }
    }
    w.write_all(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_join_java_reads() {
        let mut out = Vec::new();
        let e = Event::EntityJoin {
            id: 7,
            kind: Spawn::Player(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            pos: [1.5, 64.0, -2.0],
        };
        write(&mut out, &e).unwrap();
        let mut want = vec![b'E', b'j', 0, 0, 0, 7, 1, 0, 0, 0, 0];
        want.extend(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10u128.to_be_bytes());
        for v in [1.5f64, 64.0, -2.0] {
            want.extend(v.to_be_bytes());
        }
        assert_eq!(out, want);
    }

    #[test]
    fn writes_the_short_events() {
        let mut out = Vec::new();
        write(&mut out, &Event::WorldUnload).unwrap();
        write(&mut out, &Event::Attack(-3)).unwrap();
        write(&mut out, &Event::Remove(-3)).unwrap();
        assert_eq!(out, [b'E', b'u', b'E', b'a', 0xff, 0xff, 0xff, 0xfd, b'E', b'r', 0xff, 0xff, 0xff, 0xfd]);
    }
}
