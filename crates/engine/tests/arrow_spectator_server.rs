//! A fake 1.8.9 server spawns a remote player at x=3 and an arrow flying +x through it, with the
//! player's tab list gamemode 0 (survival) or 3 (spectator). `World.getEntities` skips spectators:
//! the arrow must end with `random.bowhit` only in the survival run.

use ferrocraft_net_client::PlayerListEvent;
use ferrocraft_net_client::{NetEvent, Session};
use ferrocraft_protocol::codec::{Metadata, Opt, Uuid, VarInt};
use ferrocraft_protocol::frame::FrameEncoder;
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::play::{PlayerListAction, PlayerListAdd};
use ferrocraft_protocol::{login, PacketSet};
use gui::tablist::{TabEntry, TabList};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use world::entities::{Entities, Fx, Kind};

const PLAYER: u128 = 0x2222;

/// `object` 60: an arrow flies +x through the player. 90: a bobber falls down onto the player, who
/// stands on a stone floor (the thrower is entity 1).
fn fake_server(gamemode: i32, object: i8) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 256];
        let _ = s.read(&mut buf);
        let mut enc = FrameEncoder::default();
        let mut send = |p: Vec<u8>| {
            let mut out = Vec::new();
            enc.encode(&p, &mut out).unwrap();
            s.write_all(&out).unwrap();
        };
        send(
            login::clientbound::Packet::from(login::clientbound::LoginSuccess {
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                username: "Ferro".into(),
            })
            .encode(),
        );
        send(
            cb::Packet::from(cb::JoinGame {
                entity_id: 1,
                gamemode: 0,
                dimension: 0,
                difficulty: 0,
                max_players: 4,
                level_type: "flat".into(),
                reduced_debug_info: false,
            })
            .encode(),
        );
        send(
            cb::Packet::from(cb::PlayerListItem {
                action: PlayerListAction::Add(vec![(
                    Uuid(PLAYER),
                    PlayerListAdd {
                        name: "Target".into(),
                        properties: vec![],
                        gamemode: VarInt(gamemode),
                        ping: VarInt(0),
                        display_name: Opt(None),
                    },
                )]),
            })
            .encode(),
        );
        send(
            cb::Packet::from(cb::SpawnPlayer {
                entity_id: VarInt(4),
                uuid: Uuid(PLAYER),
                x: if object == 60 { 3 * 32 } else { 16 },
                y: if object == 60 { 99 * 32 + 16 } else { 69 * 32 },
                z: if object == 60 { 0 } else { 16 },
                yaw: 0,
                pitch: 0,
                current_item: 0,
                metadata: Metadata(vec![]),
            })
            .encode(),
        );
        send(
            cb::Packet::from(cb::SpawnObject {
                entity_id: VarInt(5),
                kind: object,
                x: if object == 60 { 0 } else { 16 },
                y: if object == 60 { 100 * 32 } else { 2291 },
                z: if object == 60 { 0 } else { 16 },
                pitch: 0,
                yaw: 0,
                data: 2,
                velocity: Some(if object == 60 { [8000, 0, 0] } else { [0, -12800, 0] }),
            })
            .encode(),
        );
        while s.read(&mut buf).is_ok_and(|n| n > 0) {}
    });
    port
}

fn on_event(e: NetEvent, es: &mut Entities, tab: &mut TabList) {
    match e {
        NetEvent::Entity(ev) => es.apply(ev),
        NetEvent::PlayerList {
            action: PlayerListEvent::Add(v),
        } => {
            for p in v {
                tab.add(TabEntry {
                    uuid: p.uuid,
                    name: p.name,
                    gamemode: p.gamemode,
                    ..TabEntry::default()
                });
            }
        }
        _ => {}
    }
}

fn join(port: u16) -> (Entities, TabList) {
    let mut s = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).expect("login");
    let (mut es, mut tab) = (Entities::default(), TabList::default());
    let end = Instant::now() + Duration::from_secs(10);
    while es.get(5).is_none() {
        assert!(Instant::now() < end, "arrow did not spawn");
        for e in s.poll() {
            on_event(e, &mut es, &mut tab);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (es, tab)
}

/// Returns (bowhit count, arrow x or None when it ended).
fn run(gamemode: i32) -> (usize, Option<f64>) {
    let (mut es, tab) = join(fake_server(gamemode, 60));
    assert!(es.iter().any(|e| e.kind == Kind::Player));
    // Same derivation as `App::sync_spectators`.
    let spectators: Vec<u128> = es
        .iter()
        .filter_map(|e| e.uuid.filter(|&u| tab.is_spectator(u)))
        .collect();
    es.set_spectators(spectators);
    let mut hits = 0;
    for _ in 0..6 {
        es.tick_in(&|_| 0);
        hits += es
            .take_fx()
            .iter()
            .filter(|f| matches!(f, Fx::BowHit { .. }))
            .count();
    }
    (hits, es.get(5).map(|a| a.pos[0]))
}

#[test]
fn arrow_stops_at_a_survival_player_and_passes_a_spectator() {
    let (hits, x) = run(0);
    eprintln!("survival: bowhit={hits} arrow_x={x:?}");
    assert_eq!((hits, x), (1, None));
    let (hits, x) = run(3);
    eprintln!("spectator: bowhit={hits} arrow_x={x:?}");
    assert_eq!(hits, 0);
    assert!(x.is_some_and(|x| x > 5.0));
}

/// Bobber (SpawnObject 90) falling from above through a remote player onto a stone floor (y < 70).
/// Returns (y after the first tick, velocity y after it): a bobber that settles in the block stays put
/// with its velocity unchanged; one that meets an entity first keeps moving.
fn bobber(player: Option<i32>) -> (f64, f64) {
    let (mut es, tab) = join(fake_server(player.unwrap_or(0), 90));
    if player.is_none() {
        es.apply(world::entities::EntityEvent::Destroy(vec![4]));
    }
    let spectators: Vec<u128> = es
        .iter()
        .filter_map(|e| e.uuid.filter(|&u| tab.is_spectator(u)))
        .collect();
    es.set_spectators(spectators);
    es.tick_in(&|p| if p[1] < 70 { 1 << 4 } else { 0 });
    let b = es.get(5).expect("bobber");
    (b.pos[1], b.vel[1])
}

#[test]
fn bobber_stops_at_a_survival_player_not_in_the_block_behind_it() {
    let settled = bobber(None);
    eprintln!("no player: y={:.4} vy={:.4}", settled.0, settled.1);
    assert_eq!(settled.1, -1.6, "settles in the floor");
    let hit = bobber(Some(0));
    eprintln!("survival player: y={:.4} vy={:.4}", hit.0, hit.1);
    assert_ne!(hit, settled, "ends at the player instead of settling");
    let spec = bobber(Some(3));
    eprintln!("spectator: y={:.4} vy={:.4}", spec.0, spec.1);
    assert_eq!(spec, settled, "flies through a spectator and settles");
}
