//! Placement against the local vanilla 1.8.9 server: for each click the client prediction in
//! `world::place` must equal the block states the server sends back.
//!
//! Needs `harness/testserver/run.sh` with its stdin on a fifo and `FC_CONSOLE` set to that fifo,
//! then `cargo test -p ferrocraft-engine --test place_server -- --ignored --nocapture`.

use ferrocraft_net_client::{NetEvent, Session};
use std::io::Write;
use std::time::{Duration, Instant};
use world::chunk::ChunkStore;
use world::inventory::ItemStack;
use world::place::{self, Click, Outcome};

struct Bot {
    s: Session,
    store: ChunkStore,
    changes: Vec<([i32; 3], u16)>,
    pos: [f64; 3],
}

impl Bot {
    fn drain(&mut self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            for e in self.s.poll() {
                match e {
                    NetEvent::Chunk(c) => self.store.insert(c),
                    NetEvent::BlockChange { x, y, z, state } => {
                        self.store.set_block(x, y, z, state);
                        self.changes.push(([x, y, z], state));
                    }
                    NetEvent::PlayerPosLook { x, y, z, .. } => self.pos = [x, y, z],
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Poll until `done` holds (or 8 s pass), then a short quiet time for the rest of the burst.
    fn until(&mut self, done: impl Fn(&Bot) -> bool) {
        let end = Instant::now() + Duration::from_secs(8);
        while !done(self) && Instant::now() < end {
            self.drain(20);
        }
        self.drain(300);
    }

    /// A server console line (chat commands trip the server's spam kick).
    fn cmd(&mut self, c: &str) {
        let path = std::env::var("FC_CONSOLE").expect("FC_CONSOLE: the fifo of the server stdin");
        let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        writeln!(f, "{c}").unwrap();
        self.drain(400);
    }
}

struct Case {
    name: String,
    item: i16,
    damage: i16,
    yaw: f32,
    /// `/setblock` lines relative to the column, before the click.
    setup: Vec<String>,
    /// Block under the click, relative to the column (`bx`, `bz`).
    click: ([i32; 3], u8, [u8; 3]),
    /// Stand in the target column instead of two blocks south of it.
    inside: bool,
}

fn case(name: &str, item: i16, damage: i16, yaw: f32, click: ([i32; 3], u8, [u8; 3])) -> Case {
    Case {
        name: name.into(),
        item,
        damage,
        yaw,
        setup: vec![],
        click,
        inside: false,
    }
}

fn cases() -> Vec<Case> {
    let ground = |c: [u8; 3]| ([0, 3, 0], 1, c);
    let mut v = Vec::new();
    for yaw in [0.0, 90.0, 180.0, 270.0] {
        v.push(case(
            &format!("stairs bottom yaw {yaw}"),
            53,
            0,
            yaw,
            ground([8, 16, 8]),
        ));
        let mut top = case(
            &format!("stairs top yaw {yaw}"),
            67,
            0,
            yaw,
            ([0, 4, 0], 5, [16, 12, 8]),
        );
        top.setup = vec!["setblock {x} 4 {z} stone".into()];
        v.push(top);
        v.push(case(
            &format!("door yaw {yaw}"),
            324,
            0,
            yaw,
            ground([8, 16, 8]),
        ));
    }
    v.push(case("slab bottom", 44, 3, 0.0, ground([8, 16, 8])));
    let mut c = case("slab top by side", 126, 2, 0.0, ([0, 4, 0], 3, [8, 12, 16]));
    c.setup = vec!["setblock {x} 4 {z} stone".into()];
    v.push(c);
    let mut c = case("slab merge up", 44, 3, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} stone_slab 3".into()];
    v.push(c);
    let mut c = case("slab other variant", 44, 1, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} stone_slab 3".into()];
    v.push(c);
    let mut c = case(
        "slab merge below top slab",
        44,
        0,
        0.0,
        ([0, 4, 0], 0, [8, 0, 8]),
    );
    c.setup = vec!["setblock {x} 4 {z} stone_slab 8".into()];
    v.push(c);
    for (name, face) in [
        ("up", 1u8),
        ("north", 2),
        ("south", 3),
        ("west", 4),
        ("east", 5),
    ] {
        let mut c = case(
            &format!("log face {name}"),
            17,
            1,
            0.0,
            ([0, 4, 0], face, [8, 8, 8]),
        );
        c.setup = vec!["setblock {x} 4 {z} stone".into()];
        v.push(c);
        let mut c = case(
            &format!("log2 face {name}"),
            162,
            1,
            0.0,
            ([0, 4, 0], face, [8, 8, 8]),
        );
        c.setup = vec!["setblock {x} 4 {z} stone".into()];
        v.push(c);
    }
    v.push(case("torch on ground", 50, 0, 0.0, ground([8, 16, 8])));
    for (name, face) in [("north", 2u8), ("south", 3), ("west", 4), ("east", 5)] {
        let mut c = case(
            &format!("torch side {name}"),
            50,
            0,
            0.0,
            ([0, 4, 0], face, [8, 8, 8]),
        );
        c.setup = vec!["setblock {x} 4 {z} stone".into()];
        v.push(c);
        let mut c = case(
            &format!("ladder side {name}"),
            65,
            0,
            0.0,
            ([0, 4, 0], face, [8, 8, 8]),
        );
        c.setup = vec!["setblock {x} 4 {z} stone".into()];
        v.push(c);
    }
    let mut c = case(
        "torch on ceiling falls back",
        50,
        0,
        0.0,
        ([0, 4, 0], 0, [8, 0, 8]),
    );
    c.setup = vec![
        "setblock {x} 4 {z} stone".into(),
        "setblock {x1} 4 {z} stone".into(),
    ];
    v.push(c);
    let mut c = case(
        "ladder on ground beside a wall",
        65,
        0,
        0.0,
        ([0, 3, 0], 1, [8, 16, 8]),
    );
    c.setup = vec!["setblock {x1} 4 {z} stone".into()];
    v.push(c);
    v.push(case("ladder with no wall", 65, 0, 0.0, ground([8, 16, 8])));
    v.push(case("torch on air", 50, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8])));
    v.push(case("sign standing", 323, 0, 90.0, ground([8, 16, 8])));
    let mut c = case("sign wall", 323, 0, 0.0, ([0, 4, 0], 3, [8, 8, 16]));
    c.setup = vec!["setblock {x} 4 {z} stone".into()];
    v.push(c);
    v.push(case(
        "sign on the floor face down",
        323,
        0,
        0.0,
        ([0, 3, 0], 0, [8, 0, 8]),
    ));
    v.push(case("leaves", 18, 1, 0.0, ground([8, 16, 8])));
    v.push(case("wool", 35, 14, 0.0, ground([8, 16, 8])));
    v.push(case("planks", 5, 4, 0.0, ground([8, 16, 8])));
    v.push(case("stone", 1, 0, 0.0, ground([8, 16, 8])));
    let mut c = case("cube inside the player", 1, 0, 0.0, ground([8, 16, 8]));
    c.inside = true;
    v.push(c);
    let mut c = case("torch inside the player", 50, 0, 0.0, ground([8, 16, 8]));
    c.inside = true;
    v.push(c);
    let mut c = case("replace tall grass", 4, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} tallgrass 1".into()];
    v.push(c);
    let mut c = case("sapling on grass", 6, 0, 0.0, ground([8, 16, 8]));
    c.setup = vec![];
    v.push(c);
    let mut c = case("sapling on stone", 6, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} stone".into()];
    v.push(c);
    let mut c = case("rail on stone", 66, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} stone".into()];
    v.push(c);
    let mut c = case("door on slab top", 324, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} stone_slab 8".into()];
    v.push(c);
    v
}

#[test]
#[ignore = "needs the local server from harness/testserver with Ferro as operator"]
fn client_prediction_equals_the_server() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut b = Bot {
        s: Session::connect(&addr, "Ferro").expect("login"),
        store: ChunkStore::default(),
        changes: vec![],
        pos: [0.0; 3],
    };
    b.drain(4000);
    b.cmd("gamemode 1 Ferro");
    let [sx, _, sz] = b.pos.map(|v| v.floor() as i32);
    let mut bad = Vec::new();
    let only = std::env::var("FC_ONLY").unwrap_or_default();
    for (i, c) in cases().into_iter().enumerate() {
        if !c.name.contains(&only) {
            continue;
        }
        let (bx, bz) = (sx + 2 + (i as i32 % 8) * 3, sz + 4 + (i as i32 / 8) * 3);
        let fill = |l: &str| {
            l.replace("{x1}", &(bx + 1).to_string())
                .replace("{x}", &bx.to_string())
                .replace("{z}", &bz.to_string())
        };
        for l in &c.setup {
            b.cmd(&fill(l));
        }
        let stand = if c.inside { bz } else { bz - 2 };
        let (px, pz) = (f64::from(bx) + 0.5, f64::from(stand) + 0.5);
        b.pos = [0.0; 3];
        b.cmd(&format!("tp Ferro {px} 4 {pz} {} 0", c.yaw));
        b.until(|b| b.pos[1] != 0.0);
        b.s.send_position_look(px, 4.0, pz, c.yaw, 0.0, true)
            .unwrap();
        b.drain(300);
        let stack = Some(ItemStack {
            id: c.item,
            count: 1,
            damage: c.damage,
            nbt: None,
        });
        b.s.creative_slot(36, &stack).unwrap();
        b.s.held_item_change(0).unwrap();
        b.drain(500);

        let (rel, face, cur) = c.click;
        let pos = [bx + rel[0], rel[1], bz + rel[2]];
        let click = Click {
            pos,
            face,
            cursor: cur.map(|v| f32::from(v) / 16.0),
        };
        let body = [px - 0.3, 4.0, pz - 0.3, px + 0.3, 5.8, pz + 0.3];
        let at = |p: [i32; 3]| b.store.get_block(p[0], p[1], p[2]);
        let gate = place::can_place_on_side(&at, c.item, c.damage, click, body)
            || !(1..=255).contains(&c.item);
        let outcome = place::use_item(&at, c.item, c.damage, click, c.yaw, body);
        b.changes.clear();
        let before: Vec<([i32; 3], u16)> = (-1..=1)
            .flat_map(|dx| (-1..=3).flat_map(move |dy| (-1..=1).map(move |dz| [dx, dy, dz])))
            .map(|d| [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]])
            .map(|p| (p, at(p)))
            .collect();
        b.s.place_block(pos, face, &stack, cur).unwrap();
        b.until(|b| b.changes.iter().any(|(p, _)| *p == pos));
        // The server also resends the clicked block and its neighbour; keep real changes only.
        let mut got: Vec<([i32; 3], u16)> = Vec::new();
        for &(p, st) in &b.changes {
            got.retain(|(q, _)| *q != p);
            got.push((p, st));
        }
        got.retain(|(p, st)| before.iter().any(|(q, old)| q == p && old != st));
        got.sort_unstable();
        let verdict = match &outcome {
            Outcome::Placed(w) => {
                let mut w = w.clone();
                w.sort_unstable();
                got == w
            }
            Outcome::Refused => got.is_empty(),
            Outcome::Used => got.iter().any(|(_, s)| matches!(s >> 4, 63 | 68)),
        };
        eprintln!(
            "{} {:<34} gate={gate} predicted={outcome:?} server={got:?}",
            if verdict { "ok  " } else { "FAIL" },
            c.name
        );
        if !verdict {
            bad.push(c.name);
        }
    }
    assert!(
        bad.is_empty(),
        "prediction differs from the server: {bad:?}"
    );
}
