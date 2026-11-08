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
    let mut s = Session::connect(&addr, "Ferro").expect("login");
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
        l.iter()
            .any(|e| matches!(e, NetEvent::Chat(t) if t == "<Ferro> hello from ferrocraft"))
    });
    eprintln!("chat echo received");
}
