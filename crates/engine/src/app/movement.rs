//! Which of `C03` to `C06` the client sends on a tick.

/// The packet `EntityPlayerSP.onUpdateWalkingPlayer` picks.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Move {
    /// `C03 Player`: the ground flag only.
    Still,
    /// `C04 PlayerPosition`.
    Pos,
    /// `C05 PlayerLook`.
    Look,
    /// `C06 PlayerPositionAndLook`.
    Both,
}

/// `lastReportedPosX/Y/Z`, `lastReportedYaw/Pitch` and `positionUpdateTicks` of the local player.
/// A new player entity starts with all of them at zero.
#[derive(Debug, Default, PartialEq)]
pub struct SentMovement {
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    ticks: u32,
}

impl SentMovement {
    /// `EntityPlayerSP.onUpdateWalkingPlayer`: a position goes out when the player moved more than 0.03
    /// blocks since the last one, or 20 ticks passed; a look when yaw or pitch changed.
    pub fn next(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) -> Move {
        let d = [
            pos[0] - self.pos[0],
            pos[1] - self.pos[1],
            pos[2] - self.pos[2],
        ];
        let moved = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 9.0E-4 || self.ticks >= 20;
        let turned = yaw - self.yaw != 0.0 || pitch - self.pitch != 0.0;
        self.ticks += 1;
        if moved {
            self.pos = pos;
            self.ticks = 0;
        }
        if turned {
            self.yaw = yaw;
            self.pitch = pitch;
        }
        match (moved, turned) {
            (true, true) => Move::Both,
            (true, false) => Move::Pos,
            (false, true) => Move::Look,
            (false, false) => Move::Still,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled() -> SentMovement {
        let mut s = SentMovement::default();
        s.next([1.0, 64.0, 1.0], 10.0, 5.0);
        s
    }

    #[test]
    fn first_tick_of_a_new_player_reports_its_position() {
        let mut s = SentMovement::default();
        assert_eq!(s.next([0.5, 64.0, 0.5], 0.0, 0.0), Move::Pos);
        let mut s = SentMovement::default();
        assert_eq!(s.next([0.5, 64.0, 0.5], 90.0, 0.0), Move::Both);
    }

    #[test]
    fn standing_still_sends_a_position_every_21st_tick() {
        let mut s = settled();
        let still = |s: &mut SentMovement| s.next([1.0, 64.0, 1.0], 10.0, 5.0);
        for _ in 0..20 {
            assert_eq!(still(&mut s), Move::Still);
        }
        assert_eq!(still(&mut s), Move::Pos);
        assert_eq!(still(&mut s), Move::Still);
    }

    #[test]
    fn turning_on_the_spot_sends_a_look() {
        let mut s = settled();
        assert_eq!(s.next([1.0, 64.0, 1.0], 11.0, 5.0), Move::Look);
        assert_eq!(s.next([1.0, 64.0, 1.0], 11.0, 6.0), Move::Look);
    }

    #[test]
    fn walking_sends_a_position_and_walking_while_turning_sends_both() {
        let mut s = settled();
        assert_eq!(s.next([1.2, 64.0, 1.0], 10.0, 5.0), Move::Pos);
        assert_eq!(s.next([1.4, 64.0, 1.0], 12.0, 5.0), Move::Both);
    }

    /// A step shorter than 0.03 blocks is not a move, and the next tick compares to the last sent position.
    #[test]
    fn a_small_step_sends_nothing_new_until_the_sum_passes_the_limit() {
        let mut s = settled();
        assert_eq!(s.next([1.029, 64.0, 1.0], 10.0, 5.0), Move::Still);
        assert_eq!(s.next([1.031, 64.0, 1.0], 10.0, 5.0), Move::Pos);
    }
}
