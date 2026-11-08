//! Window-free game state: chunks, the local player and the 20 TPS step.

use ferrocraft_net_client::NetEvent;
use world::chunk::ChunkStore;
use world::{Block, Input, Player, World};

pub const EYE_HEIGHT: f64 = 1.62;
const CHAT_LINES: usize = 10;

/// Chunk changes that the renderer must mirror.
#[derive(Debug, PartialEq)]
pub enum Effect {
    Upload(i32, i32),
    Remove(i32, i32),
    Clear,
}

/// A 1.8 block state id as a physics block: what the renderer draws as a cube is solid, so no
/// collision is invisible. Water and lava are not solid; slabs and stairs are not modelled yet.
fn physics_block(state: u16) -> Block {
    match state >> 4 {
        8..=11 => Block::Air,
        79 | 174 | 212 => Block::Ice,
        _ if render::is_solid(state) => Block::Stone,
        _ => Block::Air,
    }
}

struct Collision<'a>(&'a ChunkStore);

impl World for Collision<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> Block {
        physics_block(self.0.get_block(x, y, z))
    }
}

pub struct Game {
    pub store: ChunkStore,
    pub player: Option<Player>,
    /// Player position before the latest tick, for partial-tick interpolation.
    pub prev_pos: [f64; 3],
    /// Set by S08 until the engine has echoed the new pose to the server.
    pub teleported: bool,
    pub pitch: f32,
    pub chat: Vec<String>,
    pub disconnected: Option<String>,
}

impl Game {
    pub fn new() -> Game {
        Game {
            store: ChunkStore::default(),
            player: None,
            prev_pos: [0.0; 3],
            teleported: false,
            pitch: 0.0,
            chat: Vec::new(),
            disconnected: None,
        }
    }

    pub fn apply(&mut self, ev: NetEvent) -> Option<Effect> {
        match ev {
            NetEvent::JoinGame { .. } => None,
            NetEvent::Respawn => {
                self.store.clear();
                Some(Effect::Clear)
            }
            NetEvent::Chunk(c) => {
                let at = (c.x, c.z);
                self.store.insert(c);
                Some(Effect::Upload(at.0, at.1))
            }
            NetEvent::UnloadChunk(x, z) => {
                self.store.remove(x, z);
                Some(Effect::Remove(x, z))
            }
            NetEvent::BlockChange { x, y, z, state } => {
                self.store.get(x >> 4, z >> 4)?;
                self.store.set_block(x, y, z, state);
                Some(Effect::Upload(x >> 4, z >> 4))
            }
            NetEvent::PlayerPosLook {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                self.teleport([x, y, z], yaw, pitch, flags);
                None
            }
            NetEvent::Chat(m) => {
                self.chat.push(m);
                let extra = self.chat.len().saturating_sub(CHAT_LINES);
                self.chat.drain(..extra);
                None
            }
            NetEvent::Disconnect(r) => {
                self.disconnected = Some(r);
                None
            }
        }
    }

    /// S08: bits 0..=2 make x/y/z relative, bit 3 yaw, bit 4 pitch.
    fn teleport(&mut self, pos: [f64; 3], yaw: f32, pitch: f32, flags: u8) {
        let rel = |bit: u8| flags & (1 << bit) != 0;
        let (old_pos, old_yaw) = self
            .player
            .as_ref()
            .map_or(([0.0; 3], 0.0), |p| (p.pos, p.yaw));
        let mut at = pos;
        for (i, v) in at.iter_mut().enumerate() {
            if rel(i as u8) {
                *v += old_pos[i];
            }
        }
        let yaw = if rel(3) { old_yaw + yaw } else { yaw };
        self.pitch = if rel(4) { self.pitch + pitch } else { pitch };
        // `Player::new` rebuilds the private bounding box, which a bare `pos` write would not.
        self.player = Some(Player::new(at, yaw));
        self.prev_pos = at;
        self.teleported = true;
    }

    /// Apply a look delta in degrees; pitch clamps to +-90 like `Entity.setAngles`.
    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        if let Some(p) = &mut self.player {
            p.yaw += dyaw;
        }
        self.pitch = (self.pitch + dpitch).clamp(-90.0, 90.0);
    }

    /// One 20 TPS step. The player stays frozen until the chunk under it has arrived, so
    /// it does not fall through the void while the world loads.
    pub fn tick(&mut self, input: Input) {
        let Some(p) = &mut self.player else { return };
        self.prev_pos = p.pos;
        let (cx, cz) = (
            (p.pos[0].floor() as i32) >> 4,
            (p.pos[2].floor() as i32) >> 4,
        );
        if self.store.get(cx, cz).is_some() {
            p.tick(&Collision(&self.store), input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::chunk::Chunk;

    fn floor_chunk() -> Chunk {
        let mut c = Chunk::new(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                c.set_block(x, 63, z, 1 << 4);
            }
        }
        c
    }

    const IDLE: Input = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    };

    #[test]
    fn relative_teleport_adds_and_absolute_replaces() {
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 1.0,
            y: 70.0,
            z: 2.0,
            yaw: 10.0,
            pitch: 5.0,
            flags: 0,
        });
        g.apply(NetEvent::PlayerPosLook {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            yaw: 5.0,
            pitch: 5.0,
            flags: 0b11111,
        });
        let p = g.player.as_ref().unwrap();
        assert!(g.teleported);
        assert_eq!(g.prev_pos, [2.0, 71.0, 3.0]);
        assert_eq!(p.pos, [2.0, 71.0, 3.0]);
        assert_eq!(p.yaw, 15.0);
        assert_eq!(g.pitch, 10.0);
    }

    #[test]
    fn player_waits_for_chunk_then_lands_on_floor() {
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 8.5,
            y: 66.0,
            z: 8.5,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.tick(IDLE);
        assert_eq!(g.player.as_ref().unwrap().pos[1], 66.0);
        assert_eq!(
            g.apply(NetEvent::Chunk(floor_chunk())),
            Some(Effect::Upload(0, 0))
        );
        for _ in 0..40 {
            g.tick(IDLE);
        }
        let p = g.player.as_ref().unwrap();
        assert_eq!(p.pos[1], 64.0);
        assert!(p.on_ground);
    }

    #[test]
    fn block_change_in_unloaded_chunk_is_dropped_and_chat_is_capped() {
        let mut g = Game::new();
        assert_eq!(
            g.apply(NetEvent::BlockChange {
                x: 0,
                y: 1,
                z: 0,
                state: 16
            }),
            None
        );
        for i in 0..15 {
            g.apply(NetEvent::Chat(i.to_string()));
        }
        assert_eq!(g.chat.len(), CHAT_LINES);
        assert_eq!(g.chat[0], "5");
    }

    #[test]
    fn only_drawn_cubes_collide_and_respawn_drops_chunks() {
        // stone, oak door, glass pane, carpet, snow layer, water, lava, ice
        let solid = [1, 64, 102, 171, 78, 8, 10, 79].map(|id| physics_block(id << 4));
        let want = [
            Block::Stone,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Ice,
        ];
        assert!(solid.iter().zip(&want).all(|(a, b)| a == b));
        let mut g = Game::new();
        g.apply(NetEvent::Chunk(floor_chunk()));
        assert_eq!(g.apply(NetEvent::Respawn), Some(Effect::Clear));
        assert!(g.store.is_empty());
    }
}
