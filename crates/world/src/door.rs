//! Client side of `DoorBlock`, `TrapdoorBlock` and `FenceGateBlock` `onUse`: the OPEN toggle.

/// The blocks to set and the aux effect (1003 open, 1006 close) the click plays.
#[derive(Debug, PartialEq)]
pub struct Toggle {
    pub set: Vec<([i32; 3], u16)>,
    pub event: i32,
}

const OPEN: u16 = 4;

fn effect(open: bool) -> i32 {
    if open {
        1003
    } else {
        1006
    }
}

/// `onUse` of the block at `pos` for a player at `yaw`; `None` when it is not a wooden door,
/// trapdoor or fence gate. Iron doors and trapdoors do not toggle.
pub fn toggle(at: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], yaw: f32) -> Option<Toggle> {
    let state = at(pos);
    let (id, meta) = (state >> 4, state & 15);
    match id {
        64 | 193..=197 => {
            // The OPEN bit lives in the lower half.
            let low = if meta & 8 != 0 {
                [pos[0], pos[1] - 1, pos[2]]
            } else {
                pos
            };
            let s = at(low);
            if s >> 4 != id {
                return None;
            }
            let s = s ^ OPEN;
            Some(Toggle {
                set: vec![(low, s)],
                event: effect(s & OPEN != 0),
            })
        }
        96 => {
            let s = state ^ OPEN;
            Some(Toggle {
                set: vec![(pos, s)],
                event: effect(s & OPEN != 0),
            })
        }
        107 | 183..=187 => {
            let mut s = state;
            if meta & OPEN != 0 {
                s &= !OPEN;
            } else {
                // `MathHelper.floor(yaw * 4 / 360 + 0.5) & 3`, a gate facing the player turns around.
                let facing = ((yaw * 4.0 / 360.0 + 0.5).floor() as i32 & 3) as u16;
                if meta & 3 == (facing + 2) & 3 {
                    s = (s & !3) | facing;
                }
                s |= OPEN;
            }
            Some(Toggle {
                set: vec![(pos, s)],
                event: effect(s & OPEN != 0),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_door_half_toggles_the_lower() {
        let at = |p: [i32; 3]| if p[1] == 0 { 64 << 4 } else { 64 << 4 | 8 };
        let t = toggle(&at, [0, 1, 0], 0.0).unwrap();
        assert_eq!(
            t,
            Toggle {
                set: vec![([0, 0, 0], 64 << 4 | 4)],
                event: 1003
            }
        );
        let open = |_: [i32; 3]| 64 << 4 | 4;
        assert_eq!(toggle(&open, [0, 0, 0], 0.0).unwrap().event, 1006);
    }

    #[test]
    fn iron_door_and_stone_do_not_toggle() {
        assert_eq!(toggle(&|_| 71 << 4, [0, 0, 0], 0.0), None);
        assert_eq!(toggle(&|_| 1 << 4, [0, 0, 0], 0.0), None);
    }

    #[test]
    fn gate_turns_to_the_player_and_closes() {
        // Gate facing south (0), player yaw 180 faces north (2): opposite, so it turns.
        let t = toggle(&|_| 107 << 4, [0, 0, 0], 180.0).unwrap();
        assert_eq!(t.set[0].1, 107 << 4 | 2 | 4);
        assert_eq!(t.event, 1003);
        let t = toggle(&|_| 107 << 4 | 2 | 4, [0, 0, 0], 180.0).unwrap();
        assert_eq!((t.set[0].1, t.event), (107 << 4 | 2, 1006));
    }
}
