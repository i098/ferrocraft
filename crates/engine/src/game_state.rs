//! `NetHandlerPlayClient.handleChangeGameState` (`GameEventS2CPacket`): what each reason makes the client do.
//! Reasons 1, 2, 7 and 8 are weather (`weather.rs`), 3 is the game mode (net-client) and 10 is the guardian curse.

/// Constants checked against the 1.8.9 jar with `javap`: reason 6 is `random.successful_hit`, volume 0.18,
/// pitch 0.45, at the player's feet plus `getEyeHeight`; `EVENT_MESSAGES` has one entry, reason 0.
#[derive(Debug, PartialEq)]
pub enum Reaction {
    /// `World.playSound(x, y, z, name, volume, pitch, false)`.
    Sound {
        name: &'static str,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    },
    /// A chat line from a lang key; `keys` are the `KeyBinding` descriptions passed as format arguments.
    Message {
        key: &'static str,
        keys: &'static [&'static str],
    },
}

/// Reason 5 with value 101, 102 or 103: the demo help lines.
const DEMO_MOVEMENT: [&str; 4] = ["key.forward", "key.left", "key.back", "key.right"];

/// `eye` is the player's feet position plus `getEyeHeight`. Reason 4 (`CreditsScreen`) and reason 5 value 0
/// (`DemoScreen`) open screens this client does not have, so they do nothing and send nothing.
pub fn react(reason: u8, value: f32, eye: [f64; 3]) -> Option<Reaction> {
    let message = |key, keys| Some(Reaction::Message { key, keys });
    match reason {
        0 => message("tile.bed.notValid", &[]),
        5 if value == 101.0 => message("demo.help.movement", &DEMO_MOVEMENT),
        5 if value == 102.0 => message("demo.help.jump", &["key.jump"]),
        5 if value == 103.0 => message("demo.help.inventory", &["key.inventory"]),
        6 => Some(Reaction::Sound {
            name: "random.successful_hit",
            pos: eye,
            volume: 0.18,
            pitch: 0.45,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_hit_is_one_quiet_high_ding_at_the_eyes() {
        assert_eq!(
            react(6, 0.0, [1.0, 65.62, 2.0]),
            Some(Reaction::Sound {
                name: "random.successful_hit",
                pos: [1.0, 65.62, 2.0],
                volume: 0.18,
                pitch: 0.45,
            })
        );
    }

    #[test]
    fn only_listed_reasons_react() {
        assert!(matches!(
            react(0, 0.0, [0.0; 3]),
            Some(Reaction::Message {
                key: "tile.bed.notValid",
                ..
            })
        ));
        assert!(react(5, 102.0, [0.0; 3]).is_some());
        assert!(react(5, 0.0, [0.0; 3]).is_none());
        for r in [1, 2, 3, 4, 7, 8, 10] {
            assert!(react(r, 1.0, [0.0; 3]).is_none());
        }
    }
}
