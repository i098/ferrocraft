//! Placement against the local vanilla 1.8.9 server: for each click the client prediction in
//! `world::place` must equal the block states the server sends back.
//!
//! Needs `harness/testserver/run.sh` with its stdin on a fifo and `FC_CONSOLE` set to that fifo,
//! then `cargo test -p ferrocraft-engine --test place_server -- --ignored --nocapture`.

use ferrocraft_net_client::{NetEvent, Session};
use std::io::Write;
use std::time::{Duration, Instant};
use world::chunk::ChunkStore;
use world::entities::Entities;
use world::inventory::ItemStack;
use world::place::{self, Click, Outcome};

struct Bot {
    s: Session,
    store: ChunkStore,
    entities: Entities,
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
                    NetEvent::Entity(ev) => self.entities.apply(ev),
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

    /// Console lines right before the click; each already drains the server's answer.
    fn run_late(&mut self, lines: impl Iterator<Item = String>) {
        for l in lines {
            self.cmd(&l);
        }
    }

    /// A lit TNT would blow a crater into the cells of later cases.
    fn defuse(&mut self, setup: &[String]) {
        if setup.iter().any(|l| l.contains("PrimedTnt")) {
            self.cmd("kill @e[type=PrimedTnt]");
        }
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
    /// Player feet height (`tp` y).
    feet: f64,
    /// Player x offset inside its column.
    px: f64,
    /// Further server changes the click causes, relative to the column.
    extra: Vec<([i32; 3], u16)>,
    /// Console lines run right before the click, after the setup has settled.
    late: Vec<String>,
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
        feet: 4.0,
        px: 0.5,
        extra: vec![],
        late: vec![],
    }
}

/// A `Used` prediction holds for a sign or door the server placed, and for the items whose box
/// the client leaves to the server (trapdoors and anvils).
fn used_is_right(item: i16, got: &[([i32; 3], u16)]) -> bool {
    matches!(item, 96 | 145 | 167) || got.iter().any(|(_, s)| matches!(s >> 4, 63 | 68))
}

/// R9-6: every entity with `preventEntitySpawning` refuses the block, living or not, also while
/// dying. R9-2: the fence-like boxes, the hopper, and the trapdoor and anvil that defer to the
/// server.
fn blocker_and_box_cases() -> Vec<Case> {
    let ground = |c: [u8; 3]| ([0, 3, 0], 1, c);
    let mut v = Vec::new();
    for (name, summon) in [
        ("cube into a boat", "summon Boat {x} 4 {z}"),
        ("cube into a minecart", "summon MinecartRideable {x} 4 {z}"),
        (
            "cube into primed tnt",
            // `Fuse` is a byte: 32000 would wrap to 0 and blow up at once.
            "summon PrimedTnt {x} 4 {z} {Fuse:127b}",
        ),
        ("cube into an end crystal", "summon EnderCrystal {x} 4 {z}"),
    ] {
        let mut c = case(name, 1, 0, 0.0, ground([8, 16, 8]));
        c.setup = vec![summon.into()];
        v.push(c);
    }
    // `isDead` turns true only when the 20-tick death animation ends.
    let mut c = case("cube into a dying mob", 1, 0, 0.0, ground([8, 16, 8]));
    c.setup = vec!["summon Pig {x} 4 {z} {NoAI:1}".into()];
    c.late = vec!["kill @e[type=Pig,x={x},y=4,z={z},r=3]".into()];
    v.push(c);
    // A fence post is 0.375..0.625 wide, a wall post 0.25..0.75, both 1.5 high; a body edge on
    // x 0.8 touches neither, a hopper fills the cell.
    for (name, item) in [
        ("fence", 85),
        ("nether fence", 113),
        ("wall", 139),
        ("hopper", 154),
        ("trapdoor", 96),
        ("anvil", 145),
    ] {
        let mut c = case(
            &format!("{name} beside a body sliver"),
            item,
            0,
            0.0,
            ground([8, 16, 8]),
        );
        c.inside = true;
        c.px = 1.1;
        v.push(c);
    }
    for (name, item) in [("fence", 85), ("wall", 139), ("gate", 107), ("cube", 1)] {
        let mut c = case(
            &format!("{name} over a raised body"),
            item,
            0,
            0.0,
            ground([8, 16, 8]),
        );
        c.inside = true;
        c.feet = 5.2;
        v.push(c);
    }
    // Placement reads the default state, so the gate's panel is the same at any yaw.
    let mut c = case(
        "gate yaw 90 in the player's cell",
        107,
        0,
        90.0,
        ground([8, 16, 8]),
    );
    c.inside = true;
    c.px = 1.1;
    v.push(c);
    // The armor stand status byte sets the box: a marker has none, a small stand is half high.
    for (name, nbt, y, item) in [
        ("cube into a marker armor stand", "Marker:1b", "4", 1),
        ("cube over a small armor stand", "Small:1b", "3.0", 1),
        ("cube into a large armor stand", "Small:0b", "3.0", 1),
    ] {
        let mut c = case(name, item, 0, 0.0, ground([8, 16, 8]));
        c.setup = vec![format!("summon ArmorStand {{x}}.5 {y} {{z}}.5 {{{nbt}}}")];
        v.push(c);
    }
    // `BlockLilyPad` fills the cell floor 0..1; one snow layer is a zero-height box at the floor.
    let mut c = case(
        "lily pad beside a body sliver",
        111,
        0,
        0.0,
        ([0, 4, -1], 3, [8, 8, 16]),
    );
    c.setup = vec![
        "setblock {x} 3 {z} water".into(),
        "setblock {x} 4 {zn} stone".into(),
    ];
    c.inside = true;
    c.px = 1.1;
    v.push(c);
    let mut c = case(
        "snow layer into a straddling body",
        78,
        0,
        0.0,
        ground([8, 16, 8]),
    );
    c.inside = true;
    c.feet = 3.5;
    v.push(c);
    // Baby and slime sizes come from metadata; a mob centred 0.3 west of the cell reaches into
    // it only with its adult half width (pig 0.45, slime 0.255 at size 1 and 0.765 at size 3).
    for (name, summon) in [
        ("cube beside a baby pig", "Pig {xw}.7 4 {z}.5 {NoAI:1b,Age:-24000}"),
        ("cube into an adult pig", "Pig {xw}.7 4 {z}.5 {NoAI:1b}"),
        ("cube beside a small slime", "Slime {xw}.7 4 {z}.5 {NoAI:1b,Size:0}"),
        ("cube into a big slime", "Slime {xw}.7 4 {z}.5 {NoAI:1b,Size:2}"),
    ] {
        let mut c = case(name, 1, 0, 0.0, ground([8, 16, 8]));
        c.setup = vec![format!("summon {summon}")];
        v.push(c);
    }
    v
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
    // R9-1: `Material.snow` is replaceable, so the block lands in the snow cell beside the stone.
    let mut c = case(
        "cube beside a 3-layer snow block",
        1,
        0,
        0.0,
        ([0, 4, 0], 2, [8, 8, 0]),
    );
    c.setup = vec![
        "setblock {x} 4 {z} stone".into(),
        "setblock {x} 4 {zn} snow_layer 2".into(),
    ];
    v.push(c);
    // R9-1: the sunflower top is not replaceable (its variant comes from the lower half).
    let mut c = case(
        "cube on a sunflower top",
        1,
        0,
        0.0,
        ([0, 5, 0], 1, [8, 16, 8]),
    );
    c.setup = vec![
        "setblock {x} 4 {z} double_plant 0".into(),
        "setblock {x} 5 {z} double_plant 10".into(),
    ];
    v.push(c);
    // R9-2: a web has no collision box, so the player's own cell does not refuse it.
    let mut c = case(
        "web in the player's own cell",
        30,
        0,
        0.0,
        ground([8, 16, 8]),
    );
    c.inside = true;
    v.push(c);
    // R9-2: the dragon egg box ends at x + 0.9375; a body on that outer sliver keeps the cell free.
    let mut c = case(
        "dragon egg beside a body sliver",
        122,
        0,
        0.0,
        ground([8, 16, 8]),
    );
    c.inside = true;
    c.px = 1.25;
    v.push(c);
    // R9-3: a pressure plate stands on a fence post.
    let mut c = case("plate on fence", 70, 0, 0.0, ([0, 4, 0], 1, [8, 16, 8]));
    c.setup = vec!["setblock {x} 4 {z} fence".into()];
    v.push(c);
    // R9-5: 1.8.9 has no y=255 refusal, so both land on top of the y=254 block.
    for (name, item) in [("cube on y=254", 1), ("torch on y=254", 50)] {
        let mut c = case(name, item, 0, 0.0, ([0, 254, 0], 1, [8, 16, 8]));
        c.feet = 255.0;
        c.setup = vec![
            "setblock {x} 254 {z} stone".into(),
            "setblock {x} 254 {zs} stone".into(),
        ];
        v.push(c);
    }
    let mut c = case(
        "button on redstone block",
        143,
        0,
        0.0,
        ([0, 5, 0], 5, [16, 8, 8]),
    );
    c.setup = vec!["setblock {x} 5 {z} redstone_block".into()];
    v.push(c);
    // R9-6: a living entity in the target cell refuses the block (armor stands are living).
    let mut c = case(
        "cube into a standing armor stand",
        1,
        0,
        0.0,
        ground([8, 16, 8]),
    );
    c.setup = vec!["summon ArmorStand {x} 4 {z}".into()];
    v.push(c);
    v.extend(blocker_and_box_cases());
    // R9-7: redstone ore is a normal cube (`BlockRedstoneOre` cannot provide power), so a
    // ladder and a button hold on it. These stay last: the lit ore's revert tick must not reach
    // another case.
    for (name, item) in [
        ("ladder on redstone ore", 65),
        ("button on redstone ore", 143),
    ] {
        let mut c = case(name, item, 0, 0.0, ([0, 5, 0], 5, [16, 8, 8]));
        c.setup = vec!["setblock {x} 5 {z} redstone_ore".into()];
        // `BlockRedstoneOre.onBlockActivated` also lights the ore.
        c.extra = vec![([0, 5, 0], 74 << 4)];
        v.push(c);
    }
    v
}

#[test]
#[ignore = "needs the local server from harness/testserver with Ferro as operator"]
fn client_prediction_equals_the_server() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut b = Bot {
        s: Session::connect(&addr, "Ferro").expect("login"),
        store: ChunkStore::default(),
        entities: Entities::default(),
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
        let stand = if c.inside { bz } else { bz - 2 };
        let fill = |l: &str| {
            l.replace("{x1}", &(bx + 1).to_string())
                .replace("{x}", &bx.to_string())
                .replace("{xw}", &(bx - 1).to_string())
                .replace("{zn}", &(bz - 1).to_string())
                .replace("{zs}", &stand.to_string())
                .replace("{z}", &bz.to_string())
        };
        for l in &c.setup {
            b.cmd(&fill(l));
        }
        let (px, pz) = (f64::from(bx) + c.px, f64::from(stand) + 0.5);
        b.pos = [0.0; 3];
        b.cmd(&format!("tp Ferro {px} {} {pz} {} 0", c.feet, c.yaw));
        b.until(|b| b.pos[1] != 0.0);
        b.s.send_position_look(px, c.feet, pz, c.yaw, 0.0, true)
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
        b.run_late(c.late.iter().map(|l| fill(l)));

        let (rel, face, cur) = c.click;
        let pos = [bx + rel[0], rel[1], bz + rel[2]];
        let click = Click {
            pos,
            face,
            cursor: cur.map(|v| f32::from(v) / 16.0),
        };
        let mut bodies = vec![[px - 0.3, c.feet, pz - 0.3, px + 0.3, c.feet + 1.8, pz + 0.3]];
        bodies.extend(b.entities.iter().filter(|e| e.blocks_placement()).map(|e| {
            let (lo, hi) = e.bounds();
            [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]]
        }));
        let at = |p: [i32; 3]| b.store.get_block(p[0], p[1], p[2]);
        let gate = place::can_place_on_side(&at, c.item, c.damage, click, &bodies)
            || !(1..=255).contains(&c.item);
        let outcome = place::use_item(&at, c.item, c.damage, click, c.yaw, &bodies);
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
        let placed = match &outcome {
            Outcome::Placed(w) => {
                let mut w = w.clone();
                w.extend(
                    c.extra
                        .iter()
                        .map(|(rel, st)| ([bx + rel[0], rel[1], bz + rel[2]], *st)),
                );
                w.sort_unstable();
                got == w
            }
            Outcome::Refused => got.is_empty(),
            // The client leaves the box of these to the server: it answers `Used` whatever
            // the server does.
            Outcome::Used => used_is_right(c.item, &got),
        };
        // The client gate and the server must agree: a gate that refuses a click the server
        // honours would drop the placement in the real client.
        let verdict = placed && (gate || got.is_empty());
        eprintln!(
            "{} {:<34} gate={gate} predicted={outcome:?} server={got:?}",
            if verdict { "ok  " } else { "FAIL" },
            c.name
        );
        if !verdict {
            bad.push(c.name);
        }
        b.defuse(&c.setup);
    }
    assert!(
        bad.is_empty(),
        "prediction differs from the server: {bad:?}"
    );
}
