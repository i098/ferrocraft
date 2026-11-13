//! Block entity packets (S24 BlockAction, S33 UpdateSign, S35 UpdateBlockEntity) as [`TileEvent`] values.

use crate::{chat, NetEvent};
use ferrocraft_protocol::nbt::{Compound, Nbt, Tag};
use ferrocraft_protocol::play::clientbound as cb;

/// What the renderer needs of a block entity.
#[derive(Debug, Clone, PartialEq)]
pub enum TileEvent {
    /// Chest lid: `event` 1 carries the number of viewers in `value`.
    Action {
        pos: [i32; 3],
        block: u16,
        event: u8,
        value: u8,
    },
    /// The four lines of a sign as formatted text (§ codes).
    Sign { pos: [i32; 3], lines: [String; 4] },
    /// `SkullType`, `Rot` and the owner's UUID.
    Skull {
        pos: [i32; 3],
        kind: u8,
        rotation: u8,
        owner: Option<u128>,
    },
    /// `Base` and `Patterns` (pattern code and dye damage of each layer).
    Banner {
        pos: [i32; 3],
        base: u8,
        layers: Vec<(String, u8)>,
    },
}

/// The event of a block entity packet; `None` for every other packet.
pub(crate) fn event(packet: &cb::Packet) -> Option<NetEvent> {
    let tile = match packet {
        cb::Packet::BlockAction(b) => TileEvent::Action {
            pos: [b.location.x, b.location.y, b.location.z],
            block: u16::try_from(b.block_type.0).unwrap_or(0),
            event: b.byte1,
            value: b.byte2,
        },
        cb::Packet::UpdateSign(s) => TileEvent::Sign {
            pos: [s.location.x, s.location.y, s.location.z],
            lines: std::array::from_fn(|i| formatted(&s.lines[i])),
        },
        cb::Packet::UpdateBlockEntity(u) => {
            let pos = [u.location.x, u.location.y, u.location.z];
            update(pos, u.action, u.nbt.as_ref()?)?
        }
        _ => return None,
    };
    Some(NetEvent::Tile(tile))
}

/// `TileEntity.onDataPacket` of the sign (9), skull (4) and banner (6) block entities.
fn update(pos: [i32; 3], action: u8, nbt: &Nbt) -> Option<TileEvent> {
    let root = &nbt.root;
    match action {
        9 => Some(TileEvent::Sign {
            pos,
            lines: ["Text1", "Text2", "Text3", "Text4"]
                .map(|k| string(root, k).map(formatted).unwrap_or_default()),
        }),
        4 => Some(TileEvent::Skull {
            pos,
            kind: byte(root, "SkullType").unwrap_or(0),
            rotation: byte(root, "Rot").unwrap_or(0),
            owner: owner(root),
        }),
        6 => Some(TileEvent::Banner {
            pos,
            base: int(root, "Base").map_or(0, |v| v as u8),
            layers: patterns(root),
        }),
        _ => None,
    }
}

fn formatted(json: &str) -> String {
    chat::formatted_text(&chat::styled(json))
}

fn tag<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.iter().find(|(k, _)| k == key).map(|(_, t)| t)
}

fn string<'a>(c: &'a Compound, key: &str) -> Option<&'a str> {
    match tag(c, key)? {
        Tag::String(s) => Some(s),
        _ => None,
    }
}

fn int(c: &Compound, key: &str) -> Option<i32> {
    match tag(c, key)? {
        Tag::Int(v) => Some(*v),
        _ => None,
    }
}

fn byte(c: &Compound, key: &str) -> Option<u8> {
    match tag(c, key)? {
        Tag::Byte(v) => Some(*v as u8),
        _ => None,
    }
}

/// `Owner.Id`, the dashed UUID of the skull's player.
fn owner(root: &Compound) -> Option<u128> {
    let Tag::Compound(owner) = tag(root, "Owner")? else {
        return None;
    };
    let hex: String = string(owner, "Id")?.chars().filter(|&c| c != '-').collect();
    u128::from_str_radix(&hex, 16).ok()
}

fn patterns(root: &Compound) -> Vec<(String, u8)> {
    let Some(Tag::List(list)) = tag(root, "Patterns") else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|t| match t {
            Tag::Compound(c) => Some((string(c, "Pattern")?.to_owned(), int(c, "Color")? as u8)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_protocol::codec::Position;
    use ferrocraft_protocol::codec::VarInt;
    use ferrocraft_protocol::PacketSet;

    fn wire(p: impl Into<cb::Packet>) -> Option<TileEvent> {
        match event(&cb::Packet::decode(&p.into().encode()).unwrap())? {
            NetEvent::Tile(t) => Some(t),
            other => panic!("not a tile event: {other:?}"),
        }
    }

    fn pos() -> Position {
        Position {
            x: -3,
            y: 64,
            z: 12,
        }
    }

    fn tags(entries: &[(&str, Tag)]) -> Nbt {
        Nbt {
            name: String::new(),
            root: entries
                .iter()
                .map(|(k, t)| (k.to_string(), t.clone()))
                .collect(),
        }
    }

    #[test]
    fn block_action_opens_a_chest() {
        let got = wire(cb::BlockAction {
            location: pos(),
            byte1: 1,
            byte2: 2,
            block_type: VarInt(54),
        });
        assert_eq!(
            got,
            Some(TileEvent::Action {
                pos: [-3, 64, 12],
                block: 54,
                event: 1,
                value: 2
            })
        );
    }

    #[test]
    fn update_sign_carries_four_formatted_lines() {
        let line = |s: &str| format!("{{\"text\":\"{s}\",\"color\":\"blue\"}}");
        let got = wire(cb::UpdateSign {
            location: pos(),
            lines: [line("a"), line("b"), "\"\"".into(), line("d")],
        });
        let Some(TileEvent::Sign { pos, lines }) = got else {
            panic!("no sign")
        };
        assert_eq!(pos, [-3, 64, 12]);
        assert_eq!(lines[0], "\u{a7}9a\u{a7}r");
        assert_eq!(lines[3], "\u{a7}9d\u{a7}r");
        assert_eq!(lines[2], "");
    }

    #[test]
    fn update_block_entity_sets_sign_skull_and_banner() {
        let send = |action, nbt| {
            wire(cb::UpdateBlockEntity {
                location: pos(),
                action,
                nbt: Some(nbt),
            })
        };
        let sign = send(9, tags(&[("Text1", Tag::String("\"hi\"".into()))]));
        assert!(matches!(sign, Some(TileEvent::Sign { lines, .. }) if lines[0].starts_with("hi")));

        let owner = Tag::Compound(vec![(
            "Id".into(),
            Tag::String("00000000-0000-0000-0000-00000000002a".into()),
        )]);
        let skull = send(
            4,
            tags(&[
                ("SkullType", Tag::Byte(3)),
                ("Rot", Tag::Byte(5)),
                ("Owner", owner),
            ]),
        );
        assert_eq!(
            skull,
            Some(TileEvent::Skull {
                pos: [-3, 64, 12],
                kind: 3,
                rotation: 5,
                owner: Some(42)
            })
        );

        let layer = Tag::Compound(vec![
            ("Pattern".into(), Tag::String("bs".into())),
            ("Color".into(), Tag::Int(11)),
        ]);
        let banner = send(
            6,
            tags(&[("Base", Tag::Int(1)), ("Patterns", Tag::List(vec![layer]))]),
        );
        assert_eq!(
            banner,
            Some(TileEvent::Banner {
                pos: [-3, 64, 12],
                base: 1,
                layers: vec![("bs".into(), 11)]
            })
        );
    }
}
