//! Window-free game state: chunks, the local player and the 20 TPS step.

use ferrocraft_net_client::NetEvent;
use input_audio::Attack;
use world::chunk::ChunkStore;
use world::dig::{self, Action, DigWorld, Digger, Frame, Hit};
use world::{Block, Input, Player, World};

pub const EYE_HEIGHT: f64 = 1.62;
const CREATIVE: u8 = 1;
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
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub gamemode: u8,
    digger: Digger,
}

/// Local block removals of the dig state machine, remembering which chunks changed.
struct Local<'a> {
    store: &'a mut ChunkStore,
    touched: &'a mut Vec<(i32, i32)>,
}

impl DigWorld for Local<'_> {
    fn state(&self, [x, y, z]: [i32; 3]) -> u16 {
        self.store.get_block(x, y, z)
    }

    fn remove(&mut self, [x, y, z]: [i32; 3]) {
        self.store.set_block(x, y, z, 0);
        self.touched.push((x >> 4, z >> 4));
    }
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
            gamemode: 0,
            digger: Digger::default(),
        }
    }

    pub fn apply(&mut self, ev: NetEvent) -> Option<Effect> {
        match ev {
            NetEvent::Respawn => {
                self.digger = Digger::default();
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
            NetEvent::JoinGame { gamemode, .. } => {
                self.gamemode = gamemode;
                None
            }
            NetEvent::BlockChange { x, y, z, state } => {
                self.store.get(x >> 4, z >> 4)?;
                self.store.set_block(x, y, z, state);
                Some(Effect::Upload(x >> 4, z >> 4))
            }
            NetEvent::GameMode(mode) => {
                self.gamemode = mode;
                None
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

    /// World-space outline box of the targeted block, grown by 0.002 like vanilla.
    pub fn selection(&self, eye: [f64; 3]) -> Option<[f64; 6]> {
        let hit = self.target(eye)?;
        let [x, y, z] = hit.pos;
        let b = dig::selection_box(self.store.get_block(x, y, z))?;
        let o = hit.pos.map(f64::from);
        let e = 0.002;
        Some([
            o[0] + b[0] - e,
            o[1] + b[1] - e,
            o[2] + b[2] - e,
            o[0] + b[3] + e,
            o[1] + b[4] + e,
            o[2] + b[5] + e,
        ])
    }

    fn creative(&self) -> bool {
        self.gamemode == CREATIVE
    }

    /// The block under the crosshair for a camera at `eye`, within the game mode's reach.
    pub fn target(&self, eye: [f64; 3]) -> Option<Hit> {
        let p = self.player.as_ref()?;
        let at = |pos: [i32; 3]| self.store.get_block(pos[0], pos[1], pos[2]);
        dig::raycast(&at, eye, p.yaw, self.pitch, dig::reach(self.creative()))
    }

    /// One client tick of block breaking. Returns the packets to send and the chunks whose
    /// blocks the player removed locally. Only survival and creative can dig.
    pub fn dig_tick(&mut self, attack: Attack) -> (Vec<Action>, Vec<Effect>) {
        let (mut actions, mut touched) = (Vec::new(), Vec::new());
        let Some(p) = &self.player else {
            return (actions, Vec::new());
        };
        if self.gamemode > CREATIVE {
            return (actions, Vec::new());
        }
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let frame = Frame {
            held: attack.held,
            presses: attack.presses,
            grabbed: attack.grabbed,
            creative: self.creative(),
            on_ground: p.on_ground,
            target: self.target(eye),
        };
        let mut local = Local {
            store: &mut self.store,
            touched: &mut touched,
        };
        self.digger.tick(&frame, &mut local, &mut actions);
        touched.sort_unstable();
        touched.dedup();
        let effects = touched.into_iter().map(|(x, z)| Effect::Upload(x, z));
        (actions, effects.collect())
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
    fn look_clamps_pitch_to_vertical_and_accumulates_yaw() {
        // Vanilla `Entity.setAngles` clamps pitch to [-90, 90].
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.look(30.0, 1000.0);
        assert_eq!(g.pitch, 90.0);
        g.look(0.0, 10.0);
        assert_eq!(g.pitch, 90.0);
        g.look(0.0, -1000.0);
        assert_eq!(g.pitch, -90.0);
        g.look(15.0, 45.0);
        assert_eq!(g.pitch, -45.0);
        assert_eq!(g.player.as_ref().unwrap().yaw, 45.0);
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

#[cfg(test)]
mod dig_tests {
    use super::*;
    use world::chunk::Chunk;
    use world::dig::Status;

    const IDLE: Input = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    };

    fn standing_on_grass(gamemode: u8) -> Game {
        let mut c = Chunk::new(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                c.set_block(x, 63, z, 2 << 4);
            }
        }
        let mut g = Game::new();
        g.apply(NetEvent::GameMode(gamemode));
        g.apply(NetEvent::Chunk(c));
        g.apply(NetEvent::PlayerPosLook {
            x: 8.5,
            y: 64.0,
            z: 8.5,
            yaw: 0.0,
            pitch: 90.0,
            flags: 0,
        });
        g.tick(IDLE);
        g
    }

    #[test]
    fn survival_hold_digs_the_block_below_and_the_server_confirms() {
        let mut g = standing_on_grass(0);
        let below = [8, 63, 8];
        assert_eq!(
            g.target([8.5, 65.62, 8.5]).map(|h| (h.pos, h.face)),
            Some((below, 1))
        );
        let mut sent = vec![];
        let mut uploads = vec![];
        for tick in 0..40 {
            let attack = Attack {
                held: true,
                presses: u32::from(tick == 0),
                grabbed: false,
            };
            let (actions, effects) = g.dig_tick(attack);
            sent.extend(actions);
            uploads.extend(effects);
            g.tick(IDLE);
        }
        let dig = |status| Action::Dig {
            status,
            pos: below,
            face: 1,
        };
        assert_eq!(sent[..2], [Action::Swing, dig(Status::Start)]);
        assert!(sent.contains(&dig(Status::Stop)));
        assert_eq!(g.store.get_block(8, 63, 8), 0);
        assert_eq!(uploads, [Effect::Upload(0, 0)]);
        // The server's BlockChange is the truth: it can put the block back.
        let back = NetEvent::BlockChange {
            x: 8,
            y: 63,
            z: 8,
            state: 2 << 4,
        };
        assert_eq!(g.apply(back), Some(Effect::Upload(0, 0)));
        assert_eq!(g.store.get_block(8, 63, 8), 2 << 4);
    }

    #[test]
    fn adventure_mode_cannot_dig() {
        let mut g = standing_on_grass(2);
        let attack = Attack {
            held: true,
            presses: 1,
            grabbed: false,
        };
        assert_eq!(g.dig_tick(attack), (vec![], vec![]));
    }
}
