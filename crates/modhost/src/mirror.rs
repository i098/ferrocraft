//! The HUD state mirror: the engine values that HUD mods read from `Minecraft` and `EntityPlayerSP`.
//!
//! Rust to Java, once per frame before the `'F'` message, tag `'M'`: i32 FPS, i32 ping in ms, three f64 position,
//! f32 yaw, f32 pitch, u8 biome id at the feet, f32 health, u8 selected hotbar slot, u16 effect count and per
//! effect i8 id, i8 amplifier, i32 ticks left; then [`SLOTS`] stacks (36 `mainInventory` slots, then the 4
//! `armorInventory` slots from the boots up) as i16 item id (-1 for an empty slot), i8 count, i16 damage;
//! last u16 length and UTF-8 bytes of the server address (empty outside a server). All numbers are big-endian.

use std::io::{self, Write};

/// 36 `mainInventory` slots and 4 armor slots.
pub const SLOTS: usize = 40;

/// An item stack without its NBT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: i16,
    pub count: i8,
    pub damage: i16,
}

/// A potion effect on the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: i8,
    pub amplifier: i8,
    pub duration: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mirror {
    pub fps: i32,
    /// Latency of the local player from the player list, in ms.
    pub ping: i32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub biome: u8,
    pub health: f32,
    /// Selected hotbar slot, 0 to 8.
    pub held: u8,
    pub effects: Vec<Effect>,
    pub slots: [Option<Stack>; SLOTS],
    /// `host:port` of the server the player joined.
    pub server: String,
}

pub fn write(w: &mut impl Write, m: &Mirror) -> io::Result<()> {
    let mut b = vec![b'M'];
    b.extend(m.fps.to_be_bytes());
    b.extend(m.ping.to_be_bytes());
    for v in m.pos {
        b.extend(v.to_be_bytes());
    }
    b.extend(m.yaw.to_be_bytes());
    b.extend(m.pitch.to_be_bytes());
    b.push(m.biome);
    b.extend(m.health.to_be_bytes());
    b.push(m.held);
    let n = u16::try_from(m.effects.len()).map_err(|_| io::Error::other("too many effects"))?;
    b.extend(n.to_be_bytes());
    for e in &m.effects {
        b.extend(e.id.to_be_bytes());
        b.extend(e.amplifier.to_be_bytes());
        b.extend(e.duration.to_be_bytes());
    }
    for s in &m.slots {
        let s = s.unwrap_or(Stack {
            id: -1,
            count: 0,
            damage: 0,
        });
        b.extend(s.id.to_be_bytes());
        b.extend(s.count.to_be_bytes());
        b.extend(s.damage.to_be_bytes());
    }
    let len =
        u16::try_from(m.server.len()).map_err(|_| io::Error::other("server name too long"))?;
    b.extend(len.to_be_bytes());
    b.extend(m.server.as_bytes());
    w.write_all(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_message_java_reads() {
        let mut slots = [None; SLOTS];
        slots[36] = Some(Stack {
            id: 267,
            count: 1,
            damage: 5,
        });
        let m = Mirror {
            fps: 144,
            ping: 37,
            pos: [1.5, 64.0, -2.5],
            yaw: 90.0,
            pitch: -10.0,
            biome: 4,
            health: 18.5,
            held: 2,
            effects: vec![Effect {
                id: 1,
                amplifier: 0,
                duration: 600,
            }],
            slots,
            server: "mc.example:25565".into(),
        };
        let mut out = Vec::new();
        write(&mut out, &m).unwrap();
        let mut want = vec![b'M'];
        want.extend(144i32.to_be_bytes());
        want.extend(37i32.to_be_bytes());
        for v in [1.5f64, 64.0, -2.5] {
            want.extend(v.to_be_bytes());
        }
        want.extend(90.0f32.to_be_bytes());
        want.extend((-10.0f32).to_be_bytes());
        want.push(4);
        want.extend(18.5f32.to_be_bytes());
        want.push(2);
        want.extend(1u16.to_be_bytes());
        want.extend([1, 0]);
        want.extend(600i32.to_be_bytes());
        for i in 0..SLOTS {
            if i == 36 {
                want.extend(267i16.to_be_bytes());
                want.push(1);
                want.extend(5i16.to_be_bytes());
            } else {
                want.extend((-1i16).to_be_bytes());
                want.extend([0, 0, 0]);
            }
        }
        want.extend(16u16.to_be_bytes());
        want.extend(b"mc.example:25565");
        assert_eq!(out, want);
    }
}
