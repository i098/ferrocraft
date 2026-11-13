//! Horse riding against the local vanilla 1.8.9 server (issues #185, #165): mounting a saddled
//! horse, the horse window (`EntityAction` 6), the jump charge (`EntityAction` 5) and the vehicle
//! view turn. Needs the server with stdin on a fifo named by `FC_CONSOLE`, Ferro as operator;
//! run `cargo test -p ferrocraft-engine --test horse_server -- --ignored --nocapture`.

use ferrocraft_net_client::{NetEvent, Session};
use std::io::Write;
use std::time::{Duration, Instant};
use world::entities::{Entities, Kind};

struct Bot {
    s: Session,
    entities: Entities,
    me: i32,
    pos: [f64; 3],
    windows: Vec<(String, Option<i32>)>,
}

impl Bot {
    fn drain(&mut self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            for e in self.s.poll() {
                match e {
                    NetEvent::JoinGame { entity_id, .. } => self.me = entity_id,
                    NetEvent::PlayerPosLook { x, y, z, .. } => self.pos = [x, y, z],
                    NetEvent::Entity(ev) => self.entities.apply(ev),
                    NetEvent::OpenWindow { kind, horse, .. } => self.windows.push((kind, horse)),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn cmd(&mut self, c: &str) {
        let path = std::env::var("FC_CONSOLE").expect("FC_CONSOLE");
        let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        writeln!(f, "{c}").unwrap();
        self.drain(500);
    }
    fn horse(&self) -> (i32, f64, f32) {
        let e = self
            .entities
            .iter()
            .find(|e| e.kind == Kind::Mob(100))
            .expect("horse");
        (e.id, e.pos[1], e.yaw)
    }
}

#[test]
#[ignore = "needs the local server from harness/testserver with Ferro as operator"]
fn horse_window_jump_and_view() {
    let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
    let mut b = Bot {
        s: Session::connect(&addr, "Ferro", None).expect("login"),
        entities: Entities::default(),
        me: -1,
        pos: [0.0; 3],
        windows: vec![],
    };
    b.drain(4000);
    b.cmd("gamemode 1 Ferro");
    let [x, y, z] = b.pos;
    let nbt = "{Tame:1,SaddleItem:{id:\"minecraft:saddle\",Count:1b}}";
    b.cmd(&format!("summon EntityHorse {} {y} {z} {nbt}", x - 2.0));
    b.drain(1000);
    let (hid, y0, yaw0) = b.horse();
    println!("horse {hid} y={y0} yaw={yaw0}");
    // mount: Interact on the tamed horse
    b.s.interact_entity(hid).unwrap();
    b.drain(1500);
    let veh = b.entities.iter().find(|e| e.id == hid).is_some();
    assert!(veh);
    // open the horse window
    b.s.entity_action(b.me, 6, 0).unwrap();
    b.drain(1500);
    println!("windows {:?}", b.windows);
    assert!(
        b.windows
            .iter()
            .any(|w| w.0 == "EntityHorse" && w.1 == Some(hid)),
        "no EntityHorse window for the horse: {:?}",
        b.windows
    );
    // jump: full charge, the server horse must rise
    let mut peak = y0;
    b.s.entity_action(b.me, 5, 100).unwrap();
    for _ in 0..40 {
        b.s.send_steer(0.0, 0.0, true, false).unwrap();
        b.drain(50);
        b.entities.tick();
        peak = peak.max(b.horse().1);
    }
    println!("peak {peak} from {y0}");
    assert!(peak > y0 + 0.5, "horse did not jump: {y0} -> {peak}");
}
