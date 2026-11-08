use ferrocraft_protocol::codec::*;
use ferrocraft_protocol::frame::{Cfb8, FrameDecoder, FrameEncoder, MAX_PACKET};
use ferrocraft_protocol::nbt::{Nbt, Tag};
use ferrocraft_protocol::play::{clientbound as cb, serverbound as sb, *};
use ferrocraft_protocol::{handshake, login, status, PacketSet, PROTOCOL_VERSION};
use std::fmt::Debug;

fn bytes_of<T: Wire>(v: &T) -> Vec<u8> {
    let mut w = Vec::new();
    v.write(&mut w);
    w
}

fn read_all<T: Wire>(b: &[u8]) -> T {
    let mut r = Reader::new(b);
    let v = T::read(&mut r).unwrap();
    r.finish().unwrap();
    v
}

/// Decode then encode must give the original bytes; encode then decode must give the value.
fn rt<P: PacketSet + PartialEq + Debug>(p: P) -> Vec<u8> {
    let enc = p.encode();
    let dec = P::decode(&enc).unwrap_or_else(|e| panic!("decode {p:?}: {e}"));
    assert_eq!(dec, p);
    assert_eq!(dec.encode(), enc);
    enc
}

#[test]
fn varint_known_vectors() {
    let cases: &[(i32, &[u8])] = &[
        (0, &[0]),
        (1, &[1]),
        (127, &[0x7F]),
        (128, &[0x80, 0x01]),
        (255, &[0xFF, 0x01]),
        (25565, &[0xDD, 0xC7, 0x01]),
        (2097151, &[0xFF, 0xFF, 0x7F]),
        (i32::MAX, &[0xFF, 0xFF, 0xFF, 0xFF, 0x07]),
        (-1, &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]),
        (i32::MIN, &[0x80, 0x80, 0x80, 0x80, 0x08]),
    ];
    for (v, b) in cases {
        assert_eq!(bytes_of(&VarInt(*v)), *b, "encode {v}");
        assert_eq!(read_all::<VarInt>(b).0, *v, "decode {v}");
    }
    assert!(matches!(
        VarInt::read(&mut Reader::new(&[0x80; 6])),
        Err(Error::VarIntTooLong)
    ));
    assert!(matches!(
        VarInt::read(&mut Reader::new(&[0x80])),
        Err(Error::Eof)
    ));
    assert_eq!(
        bytes_of(&VarLong(-1)),
        [0xFF; 9].iter().chain(&[1]).copied().collect::<Vec<_>>()
    );
    for v in [0, 1, -1, i64::MAX, i64::MIN, 1 << 40] {
        assert_eq!(read_all::<VarLong>(&bytes_of(&VarLong(v))).0, v);
    }
}

#[test]
fn position_packing() {
    let p = Position {
        x: 18357644,
        y: 831,
        z: -20882616,
    };
    assert_eq!(bytes_of(&p), 0x4607630C_FEC15B48u64.to_be_bytes());
    assert_eq!(read_all::<Position>(&bytes_of(&p)), p);
    for p in [
        Position {
            x: -33554432,
            y: -2048,
            z: -33554432,
        },
        Position {
            x: 33554431,
            y: 2047,
            z: 33554431,
        },
        Position {
            x: -1,
            y: -1,
            z: -1,
        },
        Position { x: 0, y: 0, z: 0 },
    ] {
        assert_eq!(read_all::<Position>(&bytes_of(&p)), p);
    }
}

#[test]
fn string_limits_and_utf8() {
    let s = "héllo ✓ 𝄞".to_string();
    assert_eq!(read_all::<String>(&bytes_of(&s)), s);
    // 32767 UTF-16 units is accepted, 32768 is not.
    assert!(String::read(&mut Reader::new(&bytes_of(&"a".repeat(32767)))).is_ok());
    assert!(matches!(
        String::read(&mut Reader::new(&bytes_of(&"a".repeat(32768)))),
        Err(Error::StringTooLong(_))
    ));
    let mut bad = bytes_of(&VarInt(2));
    bad.extend_from_slice(&[0xC3, 0x28]);
    assert!(matches!(
        String::read(&mut Reader::new(&bad)),
        Err(Error::InvalidUtf8)
    ));
    assert!(matches!(
        String::read(&mut Reader::new(&bytes_of(&VarInt(-1)))),
        Err(Error::NegativeLength(_))
    ));
}

#[test]
fn nbt_known_bytes_and_roundtrip() {
    // The "hello world" example from the NBT specification.
    let hello: Vec<u8> = [
        &[0x0A, 0x00, 0x0B][..],
        b"hello world",
        &[0x08, 0x00, 0x04],
        b"name",
        &[0x00, 0x09],
        b"Bananrama",
        &[0x00],
    ]
    .concat();
    let n = read_all::<Nbt>(&hello);
    assert_eq!(n.name, "hello world");
    assert_eq!(
        n.root,
        vec![("name".into(), Tag::String("Bananrama".into()))]
    );
    assert_eq!(bytes_of(&n), hello);

    let big = Nbt {
        name: String::new(),
        root: vec![
            ("b".into(), Tag::Byte(-5)),
            ("s".into(), Tag::Short(-300)),
            ("i".into(), Tag::Int(i32::MIN)),
            ("l".into(), Tag::Long(i64::MAX)),
            ("f".into(), Tag::Float(1.5)),
            ("d".into(), Tag::Double(-2.25)),
            ("ba".into(), Tag::ByteArray(vec![-1, 0, 1])),
            ("ia".into(), Tag::IntArray(vec![1, -2, 3])),
            ("str".into(), Tag::String("nul\0 é ✓ 𝄞".into())),
            ("empty".into(), Tag::List(vec![])),
            ("list".into(), Tag::List(vec![Tag::Int(1), Tag::Int(2)])),
            (
                "nest".into(),
                Tag::Compound(vec![("x".into(), Tag::List(vec![Tag::Compound(vec![])]))]),
            ),
        ],
    };
    assert_eq!(read_all::<Nbt>(&bytes_of(&big)), big);
}

#[test]
fn nbt_rejects_malformed_input() {
    // Root must be a compound.
    assert!(Nbt::read(&mut Reader::new(&[0x08, 0, 0, 0, 0])).is_err());
    // Array length larger than the data left must fail without allocating.
    let huge = [0x0A, 0, 0, 0x07, 0, 1, b'a', 0x7F, 0xFF, 0xFF, 0xFF, 0];
    assert!(Nbt::read(&mut Reader::new(&huge)).is_err());
    // Excess nesting.
    let mut deep = vec![0x0A, 0, 0];
    for _ in 0..600 {
        deep.extend_from_slice(&[0x0A, 0, 1, b'a']);
    }
    deep.extend(std::iter::repeat_n(0, 601));
    assert!(matches!(
        Nbt::read(&mut Reader::new(&deep)),
        Err(Error::Nbt(_))
    ));
    // Overlong or truncated modified UTF-8.
    assert!(Nbt::read(&mut Reader::new(&[0x0A, 0, 1, 0xC3])).is_err());
}

#[test]
fn slot_and_metadata() {
    assert_eq!(bytes_of(&Slot::None), [0xFF, 0xFF]);
    let item = Some(ItemStack {
        id: 276,
        count: 1,
        damage: 5,
        nbt: Some(Nbt {
            name: String::new(),
            root: vec![(
                "ench".into(),
                Tag::List(vec![Tag::Compound(vec![("id".into(), Tag::Short(16))])]),
            )],
        }),
    });
    assert_eq!(read_all::<Slot>(&bytes_of(&item)), item);
    let plain = Some(ItemStack {
        id: 1,
        count: 64,
        damage: 0,
        nbt: None,
    });
    assert_eq!(bytes_of(&plain), [0, 1, 64, 0, 0, 0]);

    let md = Metadata(vec![
        MetaEntry {
            index: 0,
            value: MetaValue::Byte(1),
        },
        MetaEntry {
            index: 1,
            value: MetaValue::Short(300),
        },
        MetaEntry {
            index: 2,
            value: MetaValue::Int(-7),
        },
        MetaEntry {
            index: 6,
            value: MetaValue::Float(20.0),
        },
        MetaEntry {
            index: 10,
            value: MetaValue::String("Steve".into()),
        },
        MetaEntry {
            index: 8,
            value: MetaValue::Slot(plain),
        },
        MetaEntry {
            index: 9,
            value: MetaValue::BlockPos(Position { x: 1, y: 2, z: 3 }),
        },
        MetaEntry {
            index: 31,
            value: MetaValue::Rotation(1.0, 2.0, 3.0),
        },
    ]);
    assert_eq!(read_all::<Metadata>(&bytes_of(&md)), md);
    assert_eq!(bytes_of(&Metadata(vec![])), [0x7F]);
}

#[test]
fn handshake_status_login_bytes() {
    let hs = handshake::Serverbound::Handshake(handshake::Handshake {
        protocol_version: VarInt(PROTOCOL_VERSION),
        server_address: "localhost".into(),
        server_port: 25565,
        next_state: VarInt(1),
    });
    let mut expected = vec![0x00, 0x2F, 0x09];
    expected.extend_from_slice(b"localhost");
    expected.extend_from_slice(&[0x63, 0xDD, 0x01]);
    assert_eq!(rt(hs), expected);

    rt(status::serverbound::Packet::from(
        status::serverbound::Request {},
    ));
    rt(status::serverbound::Packet::from(
        status::serverbound::Ping { time: -42 },
    ));
    rt(status::clientbound::Packet::from(
        status::clientbound::Response { json: "{}".into() },
    ));
    rt(status::clientbound::Packet::from(
        status::clientbound::Pong { time: 7 },
    ));
    rt(login::serverbound::Packet::from(
        login::serverbound::LoginStart {
            name: "Steve".into(),
        },
    ));
    rt(login::serverbound::Packet::from(
        login::serverbound::EncryptionResponse {
            shared_secret: Bytes(vec![1; 128]),
            verify_token: Bytes(vec![2; 128]),
        },
    ));
    rt(login::clientbound::Packet::from(
        login::clientbound::EncryptionRequest {
            server_id: String::new(),
            public_key: Bytes(vec![3; 162]),
            verify_token: Bytes(vec![4; 4]),
        },
    ));
    rt(login::clientbound::Packet::from(
        login::clientbound::LoginSuccess {
            uuid: "00000000-0000-0000-0000-000000000000".into(),
            username: "Steve".into(),
        },
    ));
    rt(login::clientbound::Packet::from(
        login::clientbound::SetCompression {
            threshold: VarInt(256),
        },
    ));
    rt(login::clientbound::Packet::from(
        login::clientbound::Disconnect {
            reason: "{\"text\":\"bye\"}".into(),
        },
    ));
}

#[test]
fn conditional_fields_follow_their_selector() {
    // SpawnObject: velocity only when data > 0 (vanilla S0EPacketSpawnObject).
    let mut o = cb::SpawnObject {
        entity_id: VarInt(1),
        kind: 50,
        x: 1,
        y: 2,
        z: 3,
        pitch: 4,
        yaw: 5,
        data: 0,
        velocity: None,
    };
    let without = rt(cb::Packet::from(o.clone()));
    o.data = 9;
    o.velocity = Some([1, -2, 3]);
    assert_eq!(rt(cb::Packet::from(o.clone())).len(), without.len() + 6);
    o.data = -1;
    o.velocity = None;
    assert_eq!(rt(cb::Packet::from(o)).len(), without.len());

    // Literal wire bytes, independent of this crate's own encoder.
    let props = cb::Packet::from(cb::EntityProperties {
        entity_id: VarInt(5),
        attributes: Vec32(vec![Attribute {
            key: "ab".into(),
            value: 1.0,
            modifiers: vec![],
        }]),
    });
    assert_eq!(
        props.encode(),
        [0x20, 5, 0, 0, 0, 1, 2, b'a', b'b', 0x3F, 0xF0, 0, 0, 0, 0, 0, 0, 0]
    );
    let health = cb::Packet::from(cb::UpdateHealth {
        health: 20.0,
        food: VarInt(300),
        saturation: 5.0,
    });
    assert_eq!(
        health.encode(),
        [0x06, 0x41, 0xA0, 0, 0, 0xAC, 0x02, 0x40, 0xA0, 0, 0]
    );

    for (id, n) in [(0, 0), (36, 2), (37, 1), (38, 1), (3, 0)] {
        rt(cb::Packet::from(cb::Particle {
            id,
            long_distance: true,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            offset_x: 0.0,
            offset_y: 0.0,
            offset_z: 0.0,
            speed: 0.1,
            count: 4,
            data: (0..n).map(VarInt).collect(),
        }));
    }

    for (wt, horse) in [("minecraft:chest", None), ("EntityHorse", Some(77))] {
        rt(cb::Packet::from(cb::OpenWindow {
            window_id: 1,
            window_type: wt.into(),
            title: "{\"text\":\"x\"}".into(),
            slots: 9,
            horse_entity_id: horse,
        }));
    }

    for mode in 0..=4i8 {
        let info = matches!(mode, 0 | 2).then(|| TeamInfo {
            display_name: "t".into(),
            prefix: "[".into(),
            suffix: "]".into(),
            friendly_fire: 3,
            name_tag_visibility: "always".into(),
            color: -1,
        });
        let players = matches!(mode, 0 | 3 | 4).then(|| vec!["a".to_string(), "b".to_string()]);
        rt(cb::Packet::from(cb::Teams {
            name: "red".into(),
            mode,
            info,
            players,
        }));
    }
    rt(cb::Packet::from(cb::Teams {
        name: "red".into(),
        mode: 1,
        info: None,
        players: None,
    }));

    rt(cb::Packet::from(cb::ScoreboardObjective {
        name: "o".into(),
        mode: 0,
        data: Some(ObjectiveData {
            value: "v".into(),
            kind: "integer".into(),
        }),
    }));
    rt(cb::Packet::from(cb::ScoreboardObjective {
        name: "o".into(),
        mode: 1,
        data: None,
    }));
    rt(cb::Packet::from(cb::UpdateScore {
        name: "p".into(),
        action: 0,
        objective: "o".into(),
        value: Some(VarInt(5)),
    }));
    rt(cb::Packet::from(cb::UpdateScore {
        name: "p".into(),
        action: 1,
        objective: "o".into(),
        value: None,
    }));

    rt(cb::Packet::from(cb::Maps {
        item_damage: VarInt(0),
        scale: 1,
        icons: vec![MapIcon {
            dir_type: 0x31,
            x: 1,
            z: 2,
        }],
        columns: 0,
        update: None,
    }));
    rt(cb::Packet::from(cb::Maps {
        item_damage: VarInt(0),
        scale: 1,
        icons: vec![],
        columns: 2,
        update: Some(MapUpdate {
            rows: 2,
            x: 0,
            z: 0,
            data: Bytes(vec![1, 2, 3, 4]),
        }),
    }));
}

#[test]
fn enum_payload_packets() {
    let uuid = Uuid(0x0123456789abcdef0123456789abcdef);
    for action in [
        PlayerListAction::Add(vec![(
            uuid,
            PlayerListAdd {
                name: "Steve".into(),
                properties: vec![
                    ProfileProperty {
                        name: "textures".into(),
                        value: "v".into(),
                        signature: Opt(Some("s".into())),
                    },
                    ProfileProperty {
                        name: "x".into(),
                        value: "y".into(),
                        signature: Opt(None),
                    },
                ],
                gamemode: VarInt(1),
                ping: VarInt(50),
                display_name: Opt(Some("{\"text\":\"S\"}".into())),
            },
        )]),
        PlayerListAction::Gamemode(vec![(uuid, VarInt(2))]),
        PlayerListAction::Latency(vec![(uuid, VarInt(99))]),
        PlayerListAction::DisplayName(vec![(uuid, Opt(None))]),
        PlayerListAction::Remove(vec![uuid]),
    ] {
        rt(cb::Packet::from(cb::PlayerListItem { action }));
    }

    for action in [
        WorldBorderAction::SetSize { diameter: 100.0 },
        WorldBorderAction::LerpSize {
            old_diameter: 1.0,
            new_diameter: 2.0,
            speed: VarLong(5000),
        },
        WorldBorderAction::SetCenter { x: 1.0, z: 2.0 },
        WorldBorderAction::Initialize {
            x: 0.0,
            z: 0.0,
            old_diameter: 6e7,
            new_diameter: 6e7,
            speed: VarLong(0),
            portal_teleport_boundary: VarInt(29999984),
            warning_time: VarInt(15),
            warning_blocks: VarInt(5),
        },
        WorldBorderAction::SetWarningTime { seconds: VarInt(3) },
        WorldBorderAction::SetWarningBlocks { blocks: VarInt(4) },
    ] {
        rt(cb::Packet::from(cb::WorldBorder { action }));
    }

    for action in [
        TitleAction::SetTitle("{\"text\":\"a\"}".into()),
        TitleAction::SetSubtitle("{\"text\":\"b\"}".into()),
        TitleAction::SetTimes {
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        },
        TitleAction::Hide,
        TitleAction::Reset,
    ] {
        rt(cb::Packet::from(cb::Title { action }));
    }

    for event in [
        CombatEventKind::Enter,
        CombatEventKind::End {
            duration: VarInt(3),
            entity_id: 4,
        },
        CombatEventKind::EntityDead {
            player_id: VarInt(1),
            entity_id: 2,
            message: "dead".into(),
        },
    ] {
        rt(cb::Packet::from(cb::CombatEvent { event }));
    }

    for action in [
        UseAction::Interact,
        UseAction::Attack,
        UseAction::InteractAt {
            x: 0.5,
            y: 0.25,
            z: 1.0,
        },
    ] {
        rt(sb::Packet::from(sb::UseEntity {
            target: VarInt(12),
            action,
        }));
    }
}

#[test]
fn other_packets_roundtrip() {
    let item = Some(ItemStack {
        id: 1,
        count: 2,
        damage: 3,
        nbt: None,
    });
    rt(cb::Packet::from(cb::JoinGame {
        entity_id: 1,
        gamemode: 0,
        dimension: 0,
        difficulty: 2,
        max_players: 20,
        level_type: "flat".into(),
        reduced_debug_info: false,
    }));
    rt(cb::Packet::from(cb::WindowItems {
        window_id: 0,
        slots: Vec16(vec![None, item.clone()]),
    }));
    rt(cb::Packet::from(cb::SetSlot {
        window_id: -1,
        slot: -1,
        item: item.clone(),
    }));
    rt(cb::Packet::from(cb::EntityProperties {
        entity_id: VarInt(5),
        attributes: Vec32(vec![Attribute {
            key: "generic.maxHealth".into(),
            value: 20.0,
            modifiers: vec![Modifier {
                uuid: Uuid(5),
                amount: 0.3,
                operation: 2,
            }],
        }]),
    }));
    rt(cb::Packet::from(cb::Explosion {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        radius: 4.0,
        records: Vec32(vec![Offset { x: 1, y: -1, z: 0 }]),
        velocity_x: 0.0,
        velocity_y: 0.5,
        velocity_z: 0.0,
    }));
    rt(cb::Packet::from(cb::MapChunkBulk {
        sky_light: true,
        chunks: vec![ChunkMeta {
            x: 1,
            z: -1,
            bitmask: 0xFFFF,
        }],
        data: Rest(vec![9; 100]),
    }));
    rt(cb::Packet::from(cb::UpdateSign {
        location: Position { x: 1, y: 64, z: -3 },
        lines: ["a".into(), "b".into(), "c".into(), "d".into()],
    }));
    rt(cb::Packet::from(cb::UpdateBlockEntity {
        location: Position { x: 1, y: 2, z: 3 },
        action: 9,
        nbt: Some(Nbt {
            name: String::new(),
            root: vec![("id".into(), Tag::String("Sign".into()))],
        }),
    }));
    rt(cb::Packet::from(cb::UpdateBlockEntity {
        location: Position { x: 0, y: 0, z: 0 },
        action: 1,
        nbt: None,
    }));
    rt(cb::Packet::from(cb::PluginMessage {
        channel: "MC|Brand".into(),
        data: Rest(b"vanilla".to_vec()),
    }));
    rt(cb::Packet::from(cb::SpawnPlayer {
        entity_id: VarInt(2),
        uuid: Uuid(1),
        x: 32,
        y: 64,
        z: 96,
        yaw: 1,
        pitch: 2,
        current_item: 0,
        metadata: Metadata(vec![MetaEntry {
            index: 0,
            value: MetaValue::Byte(0),
        }]),
    }));
    rt(cb::Packet::from(cb::DestroyEntities {
        entity_ids: vec![VarInt(1), VarInt(300)],
    }));
    rt(sb::Packet::from(sb::ClickWindow {
        window_id: 0,
        slot: 36,
        button: 0,
        action: 1,
        mode: 0,
        clicked_item: item.clone(),
    }));
    rt(sb::Packet::from(sb::PlayerBlockPlacement {
        location: Position { x: 1, y: 2, z: 3 },
        direction: 255,
        held_item: item,
        cursor_x: 8,
        cursor_y: 8,
        cursor_z: 8,
    }));
    rt(sb::Packet::from(sb::TabComplete {
        text: "/g".into(),
        looked_at: Opt(Some(Position { x: 1, y: 2, z: 3 })),
    }));
    rt(sb::Packet::from(sb::TabComplete {
        text: "/g".into(),
        looked_at: Opt(None),
    }));
    rt(sb::Packet::from(sb::ClientSettings {
        locale: "en_US".into(),
        view_distance: 8,
        chat_mode: 0,
        chat_colors: true,
        skin_parts: 0x7F,
    }));
    rt(sb::Packet::from(sb::Animation {}));
    rt(sb::Packet::from(sb::Spectate { target: Uuid(3) }));
}

#[test]
fn decode_rejects_bad_packets() {
    // Unknown id.
    assert!(matches!(
        cb::Packet::decode(&[0x7E]),
        Err(Error::UnknownPacket(0x7E))
    ));
    // Trailing bytes after a complete body.
    let mut keep = sb::Packet::from(sb::KeepAlive { id: VarInt(1) }).encode();
    keep.push(0);
    assert!(matches!(
        sb::Packet::decode(&keep),
        Err(Error::TrailingBytes(1))
    ));
    // Truncated body.
    assert!(matches!(cb::Packet::decode(&[0x03, 0, 0]), Err(Error::Eof)));
}

#[test]
fn random_input_never_panics() {
    let mut s = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..20000 {
        let len = (next() % 64) as usize;
        let mut b: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        if let Some(first) = b.first_mut() {
            *first %= 0x4B;
        }
        let _ = cb::Packet::decode(&b);
        let _ = sb::Packet::decode(&b);
        let _ = login::clientbound::Packet::decode(&b);
        let _ = status::clientbound::Packet::decode(&b);
    }
}

fn drain(dec: &mut FrameDecoder) -> Vec<Vec<u8>> {
    std::iter::from_fn(|| dec.next_frame().unwrap()).collect()
}

#[test]
fn frames_plain_and_compressed() {
    let small = vec![1u8; 10];
    let large = (0..5000u32).map(|i| (i % 7) as u8).collect::<Vec<_>>();

    let mut enc = FrameEncoder::default();
    let mut plain = Vec::new();
    enc.encode(&small, &mut plain).unwrap();
    assert_eq!(plain[0] as usize, small.len());

    enc.set_compression(256);
    let mut wire = Vec::new();
    enc.encode(&small, &mut wire).unwrap();
    assert_eq!(&wire[..2], &[11, 0], "below threshold: data length 0");
    enc.encode(&large, &mut wire).unwrap();
    assert!(wire.len() < large.len(), "large packet is compressed");

    // Feed one byte at a time to prove partial frames wait.
    let mut dec = FrameDecoder::default();
    dec.set_compression(256);
    let mut got = Vec::new();
    for b in &wire {
        dec.feed(std::slice::from_ref(b));
        got.extend(drain(&mut dec));
    }
    assert_eq!(got, vec![small, large]);
}

#[test]
fn bad_compressed_frames_are_rejected() {
    let mut dec = FrameDecoder::default();
    dec.set_compression(256);
    // Declares 10 bytes uncompressed size, below the threshold.
    dec.feed(&[3, 10, 0x78, 0x9C]);
    assert!(matches!(dec.next_frame(), Err(Error::BadCompression(_))));

    let mut dec = FrameDecoder::default();
    dec.set_compression(256);
    let mut b = Vec::new();
    VarInt(MAX_PACKET as i32 + 1).write(&mut b);
    b.push(0);
    let mut frame = Vec::new();
    VarInt(b.len() as i32).write(&mut frame);
    frame.extend(b);
    dec.feed(&frame);
    assert!(matches!(dec.next_frame(), Err(Error::FrameTooLarge(_))));

    // A frame length needing 4 bytes is invalid.
    let mut dec = FrameDecoder::default();
    dec.feed(&[0x80, 0x80, 0x80, 0x01]);
    assert!(matches!(dec.next_frame(), Err(Error::VarIntTooLong)));
}

#[test]
fn cfb8_matches_openssl() {
    let key: [u8; 16] = std::array::from_fn(|i| i as u8);
    let plain = b"The quick brown fox jumps over the lazy dog";
    let expected =
        "5eb44639981b73c74762c48f7d973120307c8f1aa88b0e6db30e4abda49393d9828e35c494ff4a4bc6e4f1";
    let mut buf = plain.to_vec();
    Cfb8::new(&key).encrypt(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, expected);
    Cfb8::new(&key).decrypt(&mut buf);
    assert_eq!(buf, plain);
}

#[test]
fn encrypted_and_compressed_stream_in_both_directions() {
    let secret = [7u8; 16];
    let packets: Vec<Vec<u8>> = vec![vec![0x00; 3], (0..3000u32).map(|i| i as u8).collect()];

    let mut enc = FrameEncoder::default();
    let mut dec = FrameDecoder::default();
    enc.set_compression(64);
    dec.set_compression(64);
    // Bytes before encryption starts stay plaintext.
    let mut wire = Vec::new();
    enc.encode(&packets[0], &mut wire).unwrap();
    dec.feed(&wire);
    assert_eq!(drain(&mut dec), vec![packets[0].clone()]);

    enc.enable_encryption(&secret);
    dec.enable_encryption(&secret);
    let mut wire = Vec::new();
    for p in &packets {
        enc.encode(p, &mut wire).unwrap();
    }
    assert!(
        !wire.windows(3).any(|w| w == [0, 0, 0]),
        "wire bytes are encrypted"
    );
    for chunk in wire.chunks(13) {
        dec.feed(chunk);
    }
    assert_eq!(drain(&mut dec), packets);
}
