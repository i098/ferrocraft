//! Block breaking of Minecraft 1.8.9: the block ray, bare-hand break speed and the dig state
//! machine of `PlayerControllerMP` and `Minecraft.clickMouse`/`sendClickBlockToController`.
//!
//! Not ported yet: held tools and enchantments (the client has no inventory, so the hand is
//! always empty), potion effects, water, entity targets, adventure mode and the world border.
//! The break speed of every block is checked against the real 1.8.9 jar through `oracle`.

use crate::{cos, sin};

/// Block ids 0..=197 of 1.8.9: `Block.blockHardness`, -1 for unbreakable.
const HARDNESS: [f32; 198] = [
    0.0, 1.5, 0.6, 0.5, 2.0, 2.0, 0.0, -1.0, 100.0, 100.0, // 0-9
    100.0, 100.0, 0.5, 0.6, 3.0, 3.0, 3.0, 2.0, 0.2, 0.6, // 10-19
    0.3, 3.0, 3.0, 3.5, 0.8, 0.8, 0.2, 0.7, 0.7, 0.5, // 20-29
    4.0, 0.0, 0.0, 0.5, 0.5, 0.8, -1.0, 0.0, 0.0, 0.0, // 30-39
    0.0, 3.0, 5.0, 2.0, 2.0, 2.0, 0.0, 1.5, 2.0, 50.0, // 40-49
    0.0, 0.0, 5.0, 2.0, 2.5, 0.0, 3.0, 5.0, 2.5, 0.0, // 50-59
    0.6, 3.5, 3.5, 1.0, 3.0, 0.4, 0.7, 2.0, 1.0, 0.5, // 60-69
    0.5, 5.0, 0.5, 3.0, 3.0, 0.0, 0.0, 0.5, 0.1, 0.5, // 70-79
    0.2, 0.4, 0.6, 0.0, 2.0, 2.0, 1.0, 0.4, 0.5, 0.3, // 80-89
    -1.0, 1.0, 0.5, 0.0, 0.0, 0.3, 3.0, 0.75, 1.5, 0.2, // 90-99
    0.2, 5.0, 0.3, 1.0, 0.0, 0.0, 0.2, 2.0, 2.0, 1.5, // 100-109
    0.6, 0.0, 2.0, 2.0, 2.0, 0.0, 5.0, 0.5, 2.0, -1.0, // 110-119
    -1.0, 3.0, 3.0, 0.3, 0.3, 2.0, 2.0, 0.2, 0.8, 3.0, // 120-129
    22.5, 0.0, 0.0, 5.0, 2.0, 2.0, 2.0, -1.0, 3.0, 2.0, // 130-139
    0.0, 0.0, 0.0, 0.5, 1.0, 5.0, 2.5, 0.5, 0.5, 0.0, // 140-149
    0.0, 0.2, 5.0, 3.0, 3.0, 0.8, 0.8, 0.7, 3.5, 1.25, // 150-159
    0.3, 0.2, 2.0, 2.0, 2.0, 0.0, -1.0, 5.0, 1.5, 0.3, // 160-169
    0.5, 0.1, 1.25, 5.0, 0.5, 0.0, 1.0, 1.0, 0.2, 0.8, // 170-179
    0.8, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, // 180-189
    2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, // 190-197
];

/// `Block.blockHardness` of a block id; -1 is unbreakable. Ids past 1.8.9's last block count as
/// unbreakable.
pub fn hardness(id: u16) -> f32 {
    HARDNESS.get(usize::from(id)).copied().unwrap_or(-1.0)
}

/// Blocks whose material has `requiresTool` (rock, iron, anvil, snow, web): the bare hand
/// digs them at 1/100 speed and drops nothing.
pub fn requires_tool(id: u16) -> bool {
    matches!(
        id,
        1 | 4 | 7 | 14..=16 | 21..=24 | 30 | 41..=45 | 48 | 49 | 52 | 56 | 57 | 61 | 62 | 67
            | 70 | 71 | 73 | 74 | 78 | 80 | 87 | 98 | 101 | 108 | 109 | 112..=114 | 116 | 117
            | 118 | 120 | 121 | 128..=130 | 133 | 137 | 139 | 145 | 147 | 148 | 152..=156
            | 158 | 159 | 166..=168 | 172 | 173 | 179..=182
    )
}

/// `Block.getPlayerRelativeBlockHardness` for an empty hand: the share of the block that one
/// tick of digging removes. At 1.0 or more the block breaks at once.
pub fn relative_hardness(id: u16, on_ground: bool) -> f32 {
    let h = hardness(id);
    if h < 0.0 {
        return 0.0;
    }
    let mut speed = 1.0_f32;
    if !on_ground {
        speed /= 5.0;
    }
    speed / h / if requires_tool(id) { 100.0 } else { 30.0 }
}

/// Selectable box of a block state relative to its cell as `[x0, y0, z0, x1, y1, z1]`, or `None`
/// when the block ray passes through (air, water, lava). Boxes that depend on neighbours
/// (stairs, fences, doors, panes, crops and so on) are full cubes here.
pub fn selection_box(state: u16) -> Option<[f64; 6]> {
    let meta = state & 15;
    Some(match state >> 4 {
        0 | 8..=11 => return None,
        6 | 31 | 32 => [0.1, 0.0, 0.1, 0.9, 0.8, 0.9],
        37 | 38 => [0.3, 0.0, 0.3, 0.7, 0.6, 0.7],
        39 | 40 => [0.3, 0.0, 0.3, 0.7, 0.4, 0.7],
        44 | 126 | 182 if meta & 8 != 0 => [0.0, 0.5, 0.0, 1.0, 1.0, 1.0],
        44 | 126 | 182 => [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
        60 => [0.0, 0.0, 0.0, 1.0, 0.9375, 1.0],
        78 => [0.0, 0.0, 0.0, 1.0, f64::from((meta & 7) + 1) / 8.0, 1.0],
        81 => [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375],
        83 => [0.125, 0.0, 0.125, 0.875, 1.0, 0.875],
        111 => [0.0625, 0.0, 0.0625, 0.9375, 0.015625, 0.9375],
        171 => [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0],
        _ => [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    })
}

/// A block under the crosshair: its cell and the face the ray enters (0 down, 1 up, 2 north,
/// 3 south, 4 west, 5 east: the `EnumFacing` index that PlayerDigging carries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub pos: [i32; 3],
    pub face: u8,
}

/// `Entity.getVectorForRotation`, with the vanilla sine table.
pub fn look_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let k = 0.017453292_f32;
    let f = cos(-yaw * k - std::f32::consts::PI);
    let f1 = sin(-yaw * k - std::f32::consts::PI);
    let f2 = -cos(-pitch * k);
    let f3 = sin(-pitch * k);
    [f64::from(f1 * f2), f64::from(f3), f64::from(f * f2)]
}

/// `PlayerControllerMP.getBlockReachDistance`.
pub fn reach(creative: bool) -> f64 {
    if creative {
        5.0
    } else {
        4.5
    }
}

/// Entry distance and face of the ray `eye + t * dir` into a box, if it enters from outside.
fn enter_box(eye: [f64; 3], dir: [f64; 3], cell: [i32; 3], b: [f64; 6]) -> Option<(f64, u8)> {
    let (mut near, mut far, mut face) = (f64::NEG_INFINITY, f64::INFINITY, 0);
    for a in 0..3 {
        let (lo, hi) = (f64::from(cell[a]) + b[a], f64::from(cell[a]) + b[a + 3]);
        if dir[a] == 0.0 {
            if eye[a] < lo || eye[a] > hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo - eye[a]) / dir[a], (hi - eye[a]) / dir[a]);
        let (t_in, t_out) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if t_in > near {
            near = t_in;
            // EnumFacing index of the face the ray enters: x west/east, y down/up, z north/south.
            face = [4, 0, 2][a] + u8::from(dir[a] < 0.0);
        }
        far = far.min(t_out);
    }
    (near >= 0.0 && near <= far).then_some((near, face))
}

/// First selectable block along the look ray within `reach`, walking the cells in ray order.
pub fn raycast(
    state: &dyn Fn([i32; 3]) -> u16,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    reach: f64,
) -> Option<Hit> {
    let dir = look_vector(yaw, pitch);
    let mut cell = eye.map(|v| v.floor() as i32);
    let step = dir.map(|d| if d < 0.0 { -1 } else { 1 });
    let delta = dir.map(|d| {
        if d == 0.0 {
            f64::INFINITY
        } else {
            (1.0 / d).abs()
        }
    });
    let mut t_max = [0.0; 3];
    for a in 0..3 {
        t_max[a] = if dir[a] == 0.0 {
            f64::INFINITY
        } else {
            let edge = f64::from(cell[a] + i32::from(dir[a] > 0.0));
            (edge - eye[a]) / dir[a]
        };
    }
    loop {
        if let Some(b) = selection_box(state(cell)) {
            if let Some((t, face)) = enter_box(eye, dir, cell, b) {
                if t <= reach {
                    return Some(Hit { pos: cell, face });
                }
            }
        }
        let a = (0..3)
            .min_by(|&i, &j| t_max[i].total_cmp(&t_max[j]))
            .unwrap_or(0);
        if t_max[a] > reach {
            return None;
        }
        cell[a] += step[a];
        t_max[a] += delta[a];
    }
}

/// `PlayerDigging` status byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Start = 0,
    Abort = 1,
    Stop = 2,
}

/// What the dig state machine asks the client to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Dig {
        status: Status,
        pos: [i32; 3],
        face: u8,
    },
    /// The `Animation` arm swing packet.
    Swing,
}

/// The loaded world as the dig state machine sees it.
pub trait DigWorld {
    fn state(&self, pos: [i32; 3]) -> u16;
    /// Replace the block with air, locally. The server's `BlockChange` stays the truth.
    fn remove(&mut self, pos: [i32; 3]);
}

/// Inputs of one client tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Attack key is down with the game focused.
    pub held: bool,
    /// Attack key presses since the last tick.
    pub presses: u32,
    /// The mouse was grabbed by a click this tick: `setIngameFocus`.
    pub grabbed: bool,
    pub creative: bool,
    pub on_ground: bool,
    pub target: Option<Hit>,
}

/// `PlayerControllerMP` dig fields plus `Minecraft.leftClickCounter`.
#[derive(Debug, Default)]
pub struct Digger {
    hitting: bool,
    current: [i32; 3],
    progress: f32,
    hit_delay: i32,
    click_delay: i32,
}

/// Ticks to wait after a creative break or a finished dig: `blockHitDelay`.
const HIT_DELAY: i32 = 5;
/// `leftClickCounter` after a swing at nothing in survival.
const MISS_DELAY: i32 = 10;
/// `leftClickCounter` after the click that grabs the mouse: blocks digging until the button is
/// released, which resets it.
const FOCUS_DELAY: i32 = 10000;

impl Digger {
    /// One client tick, in vanilla order: focus, counter, clicks, then the held key.
    pub fn tick(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if f.grabbed {
            self.click_delay = FOCUS_DELAY;
        }
        if self.click_delay > 0 {
            self.click_delay -= 1;
        }
        for _ in 0..f.presses {
            self.click_mouse(f, w, out);
        }
        self.send_click_block(f, w, out);
    }

    fn click_mouse(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if self.click_delay > 0 {
            return;
        }
        out.push(Action::Swing);
        match f.target {
            Some(hit) if w.state(hit.pos) >> 4 != 0 => self.click_block(f, hit, w, out),
            _ if !f.creative => self.click_delay = MISS_DELAY,
            _ => {}
        }
    }

    fn send_click_block(&mut self, f: &Frame, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        if !f.held {
            self.click_delay = 0;
        }
        if self.click_delay > 0 {
            return;
        }
        match f.target {
            Some(hit) if f.held => {
                if w.state(hit.pos) >> 4 != 0 && self.damage_block(f, hit, w, out) {
                    out.push(Action::Swing);
                }
            }
            _ => self.reset_block_removing(out),
        }
    }

    /// `PlayerControllerMP.clickBlock`.
    fn click_block(&mut self, f: &Frame, hit: Hit, w: &mut impl DigWorld, out: &mut Vec<Action>) {
        let start = |status, pos| Action::Dig {
            status,
            pos,
            face: hit.face,
        };
        if f.creative {
            out.push(start(Status::Start, hit.pos));
            self.destroy(hit.pos, w);
            self.hit_delay = HIT_DELAY;
        } else if !self.hitting || self.current != hit.pos {
            if self.hitting {
                out.push(start(Status::Abort, self.current));
            }
            out.push(start(Status::Start, hit.pos));
            let id = w.state(hit.pos) >> 4;
            if id != 0 && relative_hardness(id, f.on_ground) >= 1.0 {
                self.destroy(hit.pos, w);
            } else {
                self.hitting = true;
                self.current = hit.pos;
                self.progress = 0.0;
            }
        }
    }

    /// `PlayerControllerMP.onPlayerDamageBlock`: true when the swing counts.
    fn damage_block(
        &mut self,
        f: &Frame,
        hit: Hit,
        w: &mut impl DigWorld,
        out: &mut Vec<Action>,
    ) -> bool {
        if self.hit_delay > 0 {
            self.hit_delay -= 1;
            return true;
        }
        if f.creative {
            self.hit_delay = HIT_DELAY;
            out.push(Action::Dig {
                status: Status::Start,
                pos: hit.pos,
                face: hit.face,
            });
            self.destroy(hit.pos, w);
        } else if self.hitting && self.current == hit.pos {
            let id = w.state(hit.pos) >> 4;
            if id == 0 {
                self.hitting = false;
                return false;
            }
            self.progress += relative_hardness(id, f.on_ground);
            if self.progress >= 1.0 {
                self.hitting = false;
                out.push(Action::Dig {
                    status: Status::Stop,
                    pos: hit.pos,
                    face: hit.face,
                });
                self.destroy(hit.pos, w);
                self.progress = 0.0;
                self.hit_delay = HIT_DELAY;
            }
        } else {
            self.click_block(f, hit, w, out);
        }
        true
    }

    /// `PlayerControllerMP.resetBlockRemoving`.
    fn reset_block_removing(&mut self, out: &mut Vec<Action>) {
        if self.hitting {
            out.push(Action::Dig {
                status: Status::Abort,
                pos: self.current,
                face: 0,
            });
            self.hitting = false;
            self.progress = 0.0;
        }
    }

    /// `PlayerControllerMP.onPlayerDestroyBlock`: the block goes at once, the server confirms.
    fn destroy(&mut self, pos: [i32; 3], w: &mut impl DigWorld) {
        if w.state(pos) >> 4 == 0 {
            return;
        }
        w.remove(pos);
        self.current[1] = -1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Blocks(HashMap<[i32; 3], u16>);

    impl DigWorld for Blocks {
        fn state(&self, pos: [i32; 3]) -> u16 {
            self.0.get(&pos).copied().unwrap_or(0)
        }
        fn remove(&mut self, pos: [i32; 3]) {
            self.0.remove(&pos);
        }
    }

    const GRASS: u16 = 2 << 4;
    const STONE: u16 = 1 << 4;
    const AT: Hit = Hit {
        pos: [0, 63, 0],
        face: 1,
    };

    fn held(creative: bool) -> Frame {
        Frame {
            held: true,
            creative,
            on_ground: true,
            target: Some(AT),
            ..Frame::default()
        }
    }

    /// Ticks from the press to the `Stop` packet, counting the press tick as tick 1.
    fn ticks_to_break(state: u16) -> Option<u32> {
        let mut w = Blocks(HashMap::from([(AT.pos, state)]));
        let mut d = Digger::default();
        let mut out = vec![];
        for n in 1..=2000 {
            let f = Frame {
                presses: u32::from(n == 1),
                ..held(false)
            };
            d.tick(&f, &mut w, &mut out);
            if out.iter().any(|a| {
                matches!(
                    a,
                    Action::Dig {
                        status: Status::Stop,
                        ..
                    }
                )
            }) {
                return Some(n);
            }
        }
        None
    }

    #[test]
    fn bare_hand_break_times_follow_hardness() {
        // The press tick already adds one tick of progress, like vanilla (clickMouse, then the held key).
        // Grass 0.6: 1/0.6/30 per tick. Stone 1.5 needs a tool: 1/1.5/100 per tick.
        assert_eq!(ticks_to_break(GRASS), Some(18));
        assert_eq!(ticks_to_break(STONE), Some(151));
        assert_eq!(ticks_to_break(7 << 4), None, "bedrock never breaks");
    }

    #[test]
    fn instant_blocks_break_on_the_press() {
        let mut w = Blocks(HashMap::from([(AT.pos, 31 << 4)]));
        let mut out = vec![];
        let f = Frame {
            presses: 1,
            ..held(false)
        };
        Digger::default().tick(&f, &mut w, &mut out);
        assert_eq!(w.state(AT.pos), 0);
        assert!(out.contains(&Action::Dig {
            status: Status::Start,
            pos: AT.pos,
            face: 1
        }));
    }

    #[test]
    fn creative_breaks_with_a_five_tick_delay() {
        let mut w = Blocks::default();
        let mut d = Digger::default();
        let mut broken = vec![];
        for n in 0..14 {
            // Refill the cell so each break is visible.
            if w.state(AT.pos) == 0 {
                w.0.insert(AT.pos, STONE);
            }
            let f = Frame {
                presses: u32::from(n == 0),
                ..held(true)
            };
            d.tick(&f, &mut w, &mut vec![]);
            if w.state(AT.pos) == 0 {
                broken.push(n);
            }
        }
        assert_eq!(broken, [0, 6, 12]);
    }

    #[test]
    fn releasing_aborts_the_dig_and_the_grab_click_does_not_dig() {
        let mut w = Blocks(HashMap::from([(AT.pos, GRASS)]));
        let mut d = Digger::default();
        let mut out = vec![];
        let press = Frame {
            presses: 1,
            ..held(false)
        };
        d.tick(&press, &mut w, &mut out);
        d.tick(&held(false), &mut w, &mut out);
        out.clear();
        d.tick(&Frame::default(), &mut w, &mut out);
        assert_eq!(
            out,
            [Action::Dig {
                status: Status::Abort,
                pos: AT.pos,
                face: 0
            }]
        );

        // The click that grabbed the mouse blocks digging until the button is released.
        let mut out = vec![];
        let grab = Frame {
            grabbed: true,
            ..held(false)
        };
        d.tick(&grab, &mut w, &mut out);
        for _ in 0..30 {
            d.tick(&held(false), &mut w, &mut out);
        }
        assert!(out.is_empty());
        d.tick(&Frame::default(), &mut w, &mut out);
        d.tick(&held(false), &mut w, &mut out);
        assert!(!out.is_empty());
    }

    /// Every block's hardness, tool requirement and empty-hand speed against the real client.
    #[test]
    fn hardness_and_hand_speed_match_the_real_client() {
        let mut script = String::from("start 0 64 0 0\n");
        for id in 0..HARDNESS.len() {
            script += &format!("mine {id}\n");
        }
        let lines = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(lines.len(), HARDNESS.len());
        let mut wrong = vec![];
        for (id, line) in lines.iter().enumerate() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let bits = |s: &str| u32::from_str_radix(s, 16).unwrap();
            let id = id as u16;
            let got = (
                hardness(id).to_bits(),
                relative_hardness(id, true).to_bits(),
                relative_hardness(id, false).to_bits(),
                u32::from(!requires_tool(id)),
            );
            let want = (bits(f[0]), bits(f[1]), bits(f[2]), bits(f[3]));
            if got != want {
                wrong.push(format!(
                    "id {id}: hardness {} want {}, requires_tool {} want {}",
                    hardness(id),
                    f32::from_bits(want.0),
                    requires_tool(id),
                    want.3 == 0
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    fn cell_state(pos: [i32; 3]) -> u16 {
        if pos[1] <= 63 {
            STONE
        } else if pos == [3, 65, 0] {
            // A flower: its small box is missed by a ray through the cell's corner.
            37 << 4
        } else {
            0
        }
    }

    #[test]
    fn ray_hits_the_first_block_face_within_reach() {
        // Straight down from 1.62 above the floor top: top face of the floor block.
        let hit = raycast(&cell_state, [0.5, 65.62, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(
            hit,
            Some(Hit {
                pos: [0, 63, 0],
                face: 1
            })
        );
        // Yaw 0 looks toward +z: a wall at z = 4 is entered through its north face.
        let wall = |p: [i32; 3]| if p[2] == 4 { STONE } else { 0 };
        let hit = raycast(&wall, [0.5, 65.0, 0.5], 0.0, 0.0, 4.5);
        assert_eq!(
            hit,
            Some(Hit {
                pos: [0, 65, 4],
                face: 2
            })
        );
        // Reach 4.5 stops 3.5 blocks away from the wall face at z = 4 only when it is closer.
        assert_eq!(raycast(&wall, [0.5, 65.0, -1.0], 0.0, 0.0, 4.5), None);
        assert!(raycast(&wall, [0.5, 65.0, -1.0], 0.0, 0.0, 5.5).is_some());
        // Looking straight up at open sky hits nothing.
        assert_eq!(
            raycast(&cell_state, [0.5, 65.62, 0.5], 0.0, -90.0, 4.5),
            None
        );
    }

    #[test]
    fn ray_passes_air_and_liquid_and_hits_a_slab_half() {
        let water_over_stone = |p: [i32; 3]| match p[1] {
            64 => 9 << 4,
            ..=63 => STONE,
            _ => 0,
        };
        let hit = raycast(&water_over_stone, [0.5, 66.0, 0.5], 0.0, 90.0, 4.5);
        assert_eq!(hit.map(|h| h.pos), Some([0, 63, 0]));
        // A bottom slab is only half a block tall: a level ray at y 64.7 misses it.
        let slab = |p: [i32; 3]| if p == [0, 64, 2] { 44 << 4 } else { 0 };
        assert_eq!(raycast(&slab, [0.5, 64.7, 0.5], 0.0, 0.0, 4.5), None);
        assert!(raycast(&slab, [0.5, 64.3, 0.5], 0.0, 0.0, 4.5).is_some());
    }
}
