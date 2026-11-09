//! Vanilla 1.8.9 `Block.SoundType`: which step/dig sound a block id makes, plus local-event math.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockSound {
    Stone,
    Wood,
    Gravel,
    Grass,
    Glass,
    Metal,
    Cloth,
    Sand,
    Snow,
    Ladder,
    Anvil,
    Slime,
}

impl BlockSound {
    /// Block id (state >> 4) -> sound type; anything unlisted is stone like vanilla's default.
    pub fn of(id: u16) -> Self {
        match id {
            2 | 18 | 31 | 32 | 37 | 38 | 39 | 40 | 59 | 6 | 106 | 111 | 161 | 175 | 141 | 142
            | 83 | 110 | 170 | 19 | 46 => Self::Grass,
            3 | 13 | 60 | 82 => Self::Gravel,
            12 | 88 => Self::Sand,
            5
            | 17
            | 47
            | 53
            | 54
            | 58
            | 63
            | 64
            | 68
            | 69
            | 72
            | 85
            | 86
            | 91
            | 96
            | 99
            | 100
            | 103
            | 104
            | 105
            | 107
            | 126
            | 127
            | 134
            | 135
            | 136
            | 143
            | 146
            | 151
            | 162
            | 163
            | 164
            | 178
            | 183..=197
            | 26
            | 50
            | 75
            | 76
            | 93
            | 94
            | 125
            | 147
            | 148
            | 149
            | 150
            | 176
            | 177 => Self::Wood,
            65 => Self::Ladder,
            20 | 95 | 102 | 160 | 89 | 169 | 138 | 123 | 124 | 79 | 174 | 90 | 120 => Self::Glass,
            35 | 171 | 81 | 92 | 51 => Self::Cloth,
            165 => Self::Slime,
            78 | 80 => Self::Snow,
            145 => Self::Anvil,
            41 | 42 | 57 | 101 | 66 | 27 | 28 | 157 | 71 | 133 | 152 | 154 | 167 | 52 => {
                Self::Metal
            }
            _ => Self::Stone,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Wood => "wood",
            Self::Gravel => "gravel",
            Self::Grass => "grass",
            Self::Cloth => "cloth",
            Self::Sand => "sand",
            Self::Snow => "snow",
            Self::Ladder => "ladder",
            Self::Glass => "glass",
            Self::Stone | Self::Metal | Self::Anvil => "stone",
            Self::Slime => unreachable!(),
        }
    }

    fn volume(self) -> f32 {
        if self == Self::Anvil {
            0.3
        } else {
            1.0
        }
    }

    fn pitch(self) -> f32 {
        match self {
            Self::Metal => 1.5,
            _ => 1.0,
        }
    }

    /// Footstep: `(event, volume, pitch)`. Vanilla plays it at 0.15x block volume.
    pub fn step(self) -> (String, f32, f32) {
        (self.step_event(), self.volume() * 0.15, self.pitch())
    }

    fn step_event(self) -> String {
        match self {
            Self::Slime => "mob.slime.small".into(),
            Self::Glass => "step.stone".into(),
            _ => format!("step.{}", self.suffix()),
        }
    }

    /// `PlayerControllerMP` dig hit, every 4 ticks while breaking: step sound at `(v+1)/8`, `p*0.5`.
    pub fn dig_hit(self) -> (String, f32, f32) {
        (
            self.step_event(),
            (self.volume() + 1.0) / 8.0,
            self.pitch() * 0.5,
        )
    }

    /// Break (aux effect 2001) and place share one sound: `(event, volume, pitch)`.
    /// Ladder breaks with `dig.wood` like vanilla `soundTypeLadder`.
    pub fn dig(self) -> (String, f32, f32) {
        let e = match self {
            Self::Slime => "mob.slime.big".into(),
            Self::Ladder => "dig.wood".into(),
            _ => format!("dig.{}", self.suffix()),
        };
        (e, (self.volume() + 1.0) / 2.0, self.pitch() * 0.8)
    }
}

/// Wire `pitch` byte of `SoundEffect` (0x29) to a playback rate: `byte / 63`, floor at vanilla's 0.5 clamp.
pub fn wire_pitch(byte: u8) -> f32 {
    (byte as f32 / 63.0).max(0.5)
}

/// Aux effect (0x28) ids that play a sound: `(event, volume, pitch)`; `random`, `random2` are in `[0,1)`.
pub fn effect_sound(id: i32, random: f32, random2: f32) -> Option<(&'static str, f32, f32)> {
    let mob = (random - random2) * 0.2 + 1.0;
    let anvil = random * 0.1 + 0.9;
    if id == 1003 {
        let door = if random < 0.5 {
            "random.door_open"
        } else {
            "random.door_close"
        };
        return Some((door, 1.0, anvil));
    }
    const FIXED: &[(i32, &str, f32, f32)] = &[
        (1000, "random.click", 1.0, 1.0),
        (1001, "random.click", 1.0, 1.2),
        (1002, "random.bow", 1.0, 1.2),
        (1004, "random.fizz", 0.5, 2.6),
    ];
    const MOB: &[(i32, &str, f32)] = &[
        (1007, "mob.ghast.charge", 10.0),
        (1008, "mob.ghast.fireball", 10.0),
        (1009, "mob.ghast.fireball", 2.0),
        (1010, "mob.zombie.wood", 2.0),
        (1011, "mob.zombie.metal", 2.0),
        (1012, "mob.zombie.woodbreak", 2.0),
        (1014, "mob.wither.shoot", 2.0),
        (1015, "mob.bat.takeoff", 0.05),
        (1016, "mob.zombie.infect", 2.0),
        (1017, "mob.zombie.unfect", 2.0),
    ];
    const ANVIL: &[(i32, &str, f32)] = &[
        (1020, "random.anvil_break", 1.0),
        (1021, "random.anvil_use", 1.0),
        (1022, "random.anvil_land", 0.3),
        (2002, "game.potion.smash", 1.0),
    ];
    let scaled = |t: &[(i32, &'static str, f32)], pitch: f32| {
        t.iter().find(|e| e.0 == id).map(|&(_, n, v)| (n, v, pitch))
    };
    FIXED
        .iter()
        .find(|e| e.0 == id)
        .map(|&(_, n, v, p)| (n, v, p))
        .or_else(|| scaled(MOB, mob))
        .or_else(|| scaled(ANVIL, anvil))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_and_dig_follow_vanilla_scaling() {
        assert_eq!(BlockSound::of(2).step(), ("step.grass".into(), 0.15, 1.0));
        assert_eq!(BlockSound::of(1).dig(), ("dig.stone".into(), 1.0, 0.8));
        assert_eq!(BlockSound::of(20).step().0, "step.stone");
        assert_eq!(BlockSound::of(20).dig().0, "dig.glass");
        assert_eq!(wire_pitch(63), 1.0);
        assert_eq!(wire_pitch(10), 0.5);
    }

    #[test]
    fn special_blocks_follow_vanilla_sound_types() {
        use BlockSound::*;
        let table = [
            (82, Gravel),
            (172, Stone),
            (159, Stone),
            (19, Grass),
            (46, Grass),
            (79, Glass),
            (174, Glass),
            (50, Wood),
            (75, Wood),
            (76, Wood),
            (26, Wood),
            (52, Metal),
            (92, Cloth),
            (51, Cloth),
            (65, Ladder),
            (25, Stone),
            (30, Stone),
            (84, Stone),
            (147, Wood),
            (90, Glass),
            (120, Glass),
            (165, Slime),
        ];
        for (id, ty) in table {
            assert_eq!(BlockSound::of(id), ty, "block {id}");
        }
        assert_eq!(Ladder.dig().0, "dig.wood");
        assert_eq!(Ladder.step().0, "step.ladder");
        assert_eq!(Slime.dig().0, "mob.slime.big");
        assert_eq!(Anvil.dig_hit(), ("step.stone".into(), 0.1625, 0.5));
    }
}
