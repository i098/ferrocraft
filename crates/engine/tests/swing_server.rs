//! Arm swing length against the local vanilla 1.8.9 server: Haste II, Mining Fatigue I and no effect
//! must give 4, 8 and 6 ticks (`getArmSwingAnimationEnd`), seen by the swinger and by an observer fed
//! only by the server's `S1D`/`S1E`/`S0B` packets.
//!
//! Needs `harness/testserver/run.sh` with its stdin on a fifo and `FC_CONSOLE` set to that fifo,
//! then `cargo test -p ferrocraft-engine --test swing_server -- --ignored --nocapture`.

use ferrocraft_net_client::{NetEvent, Session};
use std::io::Write;
use std::time::{Duration, Instant};
use world::effects::Effects;
use world::entities::{Entities, EntityEvent};

struct Bot {
    s: Session,
    id: i32,
    entities: Entities,
    effects: Effects,
}

impl Bot {
    fn join(name: &str) -> Bot {
        let addr = std::env::var("FC_SERVER").unwrap_or_else(|_| "127.0.0.1:25565".into());
        let mut b = Bot {
            s: Session::connect(&addr, name, None).expect("login"),
            id: -1,
            entities: Entities::default(),
            effects: Effects::default(),
        };
        b.drain(3000);
        assert!(b.id >= 0, "{name}: no JoinGame");
        b
    }

    fn drain(&mut self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            for e in self.s.poll() {
                self.on_event(e);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The own entity's effects go to `effects`; everything else to the entity table.
    fn on_event(&mut self, e: NetEvent) {
        let ev = match e {
            NetEvent::JoinGame { entity_id, .. } => {
                self.id = entity_id;
                return;
            }
            NetEvent::EntityEffect { entity_id, effect } => (entity_id, Some(effect), effect.id),
            NetEvent::RemoveEntityEffect {
                entity_id,
                effect_id,
            } => (entity_id, None, effect_id),
            NetEvent::Entity(ev) => {
                self.entities.apply(ev);
                return;
            }
            _ => return,
        };
        let (id, add, effect_id) = ev;
        if id == self.id {
            match add {
                Some(effect) => self.effects.add(effect),
                None => self.effects.remove(effect_id),
            }
            return;
        }
        self.entities.apply(match add {
            Some(effect) => EntityEvent::Effect { id, effect },
            None => EntityEvent::RemoveEffect { id, effect_id },
        });
    }

    /// Ticks the observer's view of `swinger` until its swing ends.
    fn swing_ticks(&mut self, swinger: i32) -> i32 {
        let mut ticks = 0;
        while self.entities.get(swinger).unwrap().swinging && ticks < 40 {
            self.entities.tick();
            ticks += 1;
        }
        ticks
    }
}

fn cmd(c: &str) {
    let path = std::env::var("FC_CONSOLE").expect("FC_CONSOLE: the fifo of the server stdin");
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(f, "{c}").unwrap();
}

#[test]
#[ignore = "needs the local server from harness/testserver"]
fn swing_length_follows_the_servers_effect_packets() {
    let mut a = Bot::join("SwingA");
    a.drain(500);
    // (console effect line, vanilla swing length)
    for (line, want) in [
        ("effect SwingA 3 60 1", 4),
        ("effect SwingA 4 60 0", 8),
        ("effect SwingA clear", 6),
    ] {
        cmd("effect SwingA clear");
        cmd(line);
        a.drain(600);
        // 1.8.9 sends `S1D` to the player alone and to observers only when they start tracking.
        let mut b = Bot::join(&format!("SwingB{want}"));
        assert_eq!(a.effects.arm_swing_duration(), want, "{line}: swinger");
        let ent = b.entities.get(a.id).expect("observer sees the swinger");
        assert_eq!(ent.effects.arm_swing_duration(), want, "{line}: observer");

        a.s.send_swing().unwrap();
        a.drain(300);
        b.drain(300);
        assert!(
            b.entities.get(a.id).unwrap().swinging,
            "{line}: no swing seen"
        );
        let ticks = b.swing_ticks(a.id);
        println!("{line}: observer swing = {ticks} ticks (vanilla duration {want})");
        assert_eq!(ticks, want + 1, "{line}: observer swing length");
    }
}
