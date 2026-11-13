//! Needs a local offline 1.8.9 server: run `harness/testserver/run.sh`, then
//! `cargo test -p ferrocraft-net-client -- --ignored --nocapture`. `FC_SERVER` overrides the address.

use ferrocraft_net_client::{NetEvent, Session};
use std::time::{Duration, Instant};

fn wait_for(
    s: &mut Session,
    log: &mut Vec<NetEvent>,
    what: &str,
    done: impl Fn(&[NetEvent]) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(30);
    while !done(log) {
        assert!(
            Instant::now() < end,
            "timeout waiting for {what}; got {} events",
            log.len()
        );
        log.extend(s.poll());
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "needs the local server from harness/testserver"]
fn join_receive_chunks_and_chat_echo() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut s = Session::connect(&addr, "Ferro", None).expect("login");
    let mut log = Vec::new();

    wait_for(&mut s, &mut log, "join + position + chunks", |l| {
        l.iter().any(|e| matches!(e, NetEvent::JoinGame { .. }))
            && l.iter()
                .any(|e| matches!(e, NetEvent::PlayerPosLook { .. }))
            && l.iter().filter(|e| matches!(e, NetEvent::Chunk(_))).count() >= 9
    });
    for e in &log {
        match e {
            NetEvent::Chunk(c) => eprintln!(
                "chunk ({}, {}) floor block state {:#x}",
                c.x,
                c.z,
                c.block(8, 0, 8)
            ),
            other => eprintln!("{other:?}"),
        }
    }
    let flat_floor = log.iter().find_map(|e| match e {
        NetEvent::Chunk(c) => Some(c.block(8, 0, 8)),
        _ => None,
    });
    assert_eq!(flat_floor, Some(7 << 4), "flat world has bedrock at y=0");

    s.send_chat("hello from ferrocraft").unwrap();
    wait_for(&mut s, &mut log, "chat echo", |l| {
        l.iter().any(|e| {
            matches!(e, NetEvent::Chat(runs)
                if runs.iter().map(|r| r.text.as_str()).collect::<String>() == "<Ferro> hello from ferrocraft")
        })
    });
    eprintln!("chat echo received");
}

#[test]
#[ignore = "needs the local server from harness/testserver"]
fn survival_start_then_finish_digging_removes_the_grass_below_spawn() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut s = Session::connect(&addr, "Ferro", None).expect("login");
    let mut log = Vec::new();
    wait_for(&mut s, &mut log, "join, position and chunks", |l| {
        l.iter()
            .any(|e| matches!(e, NetEvent::PlayerPosLook { .. }))
            && l.iter().filter(|e| matches!(e, NetEvent::Chunk(_))).count() >= 9
    });
    let (x, y, z) = log
        .iter()
        .find_map(|e| match e {
            NetEvent::PlayerPosLook { x, y, z, .. } => Some((*x, *y, *z)),
            _ => None,
        })
        .unwrap();
    let pos = [x.floor() as i32, y.floor() as i32 - 1, z.floor() as i32];
    let below = log.iter().find_map(|e| match e {
        NetEvent::Chunk(c) if (c.x, c.z) == (pos[0] >> 4, pos[2] >> 4) => Some(c.block(
            (pos[0] & 15) as usize,
            pos[1] as usize,
            (pos[2] & 15) as usize,
        )),
        _ => None,
    });
    assert_eq!(
        below,
        Some(2 << 4),
        "the flat world has grass under the spawn"
    );

    // Hand digging grass takes 18 ticks; the server accepts the finish after 14.
    s.send_swing().unwrap();
    s.send_dig(0, pos, 1).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    s.send_dig(2, pos, 1).unwrap();
    wait_for(&mut s, &mut log, "the server's air BlockChange", |l| {
        l.iter().any(
            |e| matches!(e, NetEvent::BlockChange { x, y, z, state: 0 } if [*x, *y, *z] == pos),
        )
    });
    eprintln!("server confirmed: block {pos:?} is air");
}

/// Online-mode proof: start the server with `FC_ONLINE=1 harness/testserver/run.sh` and give the
/// account in the environment (`FC_ACCESS_TOKEN`, `FC_UUID`, `FC_USERNAME`). Only for a local server.
#[test]
#[ignore = "needs an online-mode local server and a real account"]
fn online_mode_login_joins_the_local_server() {
    use ferrocraft_net_client::Auth;
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let auth = Auth::new(var("FC_ACCESS_TOKEN"), &var("FC_UUID"));
    let mut s = Session::connect(&addr, &var("FC_USERNAME"), Some(&auth)).expect("online login");
    let mut log = Vec::new();
    wait_for(&mut s, &mut log, "join and chunks over encryption", |l| {
        l.iter().any(|e| matches!(e, NetEvent::JoinGame { .. }))
            && l.iter().filter(|e| matches!(e, NetEvent::Chunk(_))).count() >= 9
    });
    eprintln!("joined online-mode server, {} events", log.len());
}

#[test]
#[ignore = "needs the local server from harness/testserver"]
fn tab_complete_slash_he_returns_help() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut s = Session::connect(&addr, "Ferro", None).expect("login");
    let mut log = Vec::new();
    wait_for(&mut s, &mut log, "join", |l| {
        l.iter().any(|e| matches!(e, NetEvent::JoinGame { .. }))
    });
    s.send_tab_complete("/he").unwrap();
    wait_for(&mut s, &mut log, "tab complete reply", |l| {
        l.iter()
            .any(|e| matches!(e, NetEvent::TabComplete(m) if m.iter().any(|x| x == "/help")))
    });
}
