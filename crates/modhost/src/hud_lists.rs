//! The HUD lists for the mods: the tab list, the scoreboard, the boss bar, the title and the action bar that
//! the engine holds and the vanilla classes in the JVM must hold too.
//!
//! Rust to Java, once per frame right after the `'M'` message, tag `'L'`; Java does not answer. All numbers are
//! big-endian, a string is a u32 length and UTF-8 bytes (`§` codes stay).
//!
//! `'L'`, u32 length of the list part, the list part, then the timed part. The list part is what changes rarely, so
//! Java skips it when the bytes equal the last frame's:
//! - u16 player count, per player: u128 uuid, name string, i32 ping, u8 game mode, u8 flags (bit 0 the local
//!   player, bit 1 a display name follows), the display name string when bit 1 is set; header string, footer string.
//! - u16 objective count, per objective: name string, display name string, u8 hearts, u16 score count and per score
//!   the player string and an i32 value. u16 slot count, per slot: u8 display slot, objective name string.
//! - u16 team count, per team: name string, prefix string, suffix string, name tag visibility key string, i8 color
//!   (-1 for none), u16 member count and the member strings.
//!
//! The timed part, which counts down every tick: u8 boss bar shown, then when shown a name string, f32 health
//! ratio and i32 ticks left; title string, subtitle string, four i32 (ticks left, fade in, stay, fade out); u8
//! action bar shown, then when shown a string and the i32 ticks left.

use std::io::{self, Write};

/// A string above this many bytes means a corrupt stream, not a long text.
pub const MAX_TEXT: usize = 1 << 16;
/// The list part above this many bytes means a corrupt stream.
pub const MAX_LISTS: usize = 1 << 22;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    pub uuid: u128,
    pub name: String,
    pub ping: i32,
    pub gamemode: u8,
    /// The engine's own player: the JVM keeps its stand-in player and shows this entry there.
    pub local: bool,
    /// Display name with `§` codes, replacing the team-formatted name.
    pub display: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Objective {
    pub name: String,
    pub display: String,
    pub hearts: bool,
    pub scores: Vec<(String, i32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Team {
    pub name: String,
    pub prefix: String,
    pub suffix: String,
    /// `nameTagVisibility` key: `always`, `never`, `hideForOtherTeams` or `hideForOwnTeam`.
    pub visibility: &'static str,
    /// `EnumChatFormatting` color index; `None` is reset.
    pub color: Option<u8>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Boss {
    pub name: String,
    pub health: f32,
    pub time: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Title {
    pub title: String,
    pub subtitle: String,
    pub timer: i32,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lists {
    pub players: Vec<Player>,
    pub header: String,
    pub footer: String,
    pub objectives: Vec<Objective>,
    /// Display slot and the name of the objective in it.
    pub slots: Vec<(u8, String)>,
    pub teams: Vec<Team>,
    pub boss: Option<Boss>,
    pub title: Title,
    /// The action bar text and the ticks it stays.
    pub action_bar: Option<(String, i32)>,
}

fn string(b: &mut Vec<u8>, s: &str) -> io::Result<()> {
    if s.len() > MAX_TEXT {
        return Err(io::Error::other(format!("{} byte string", s.len())));
    }
    b.extend((s.len() as u32).to_be_bytes());
    b.extend(s.as_bytes());
    Ok(())
}

fn count(b: &mut Vec<u8>, n: usize, what: &str) -> io::Result<()> {
    let n = u16::try_from(n).map_err(|_| io::Error::other(format!("too many {what}")))?;
    b.extend(n.to_be_bytes());
    Ok(())
}

fn players_part(b: &mut Vec<u8>, l: &Lists) -> io::Result<()> {
    count(b, l.players.len(), "players")?;
    for p in &l.players {
        b.extend(p.uuid.to_be_bytes());
        string(b, &p.name)?;
        b.extend(p.ping.to_be_bytes());
        b.push(p.gamemode);
        b.push(u8::from(p.local) | u8::from(p.display.is_some()) << 1);
        if let Some(d) = &p.display {
            string(b, d)?;
        }
    }
    string(b, &l.header)?;
    string(b, &l.footer)
}

fn board_part(b: &mut Vec<u8>, l: &Lists) -> io::Result<()> {
    count(b, l.objectives.len(), "objectives")?;
    for o in &l.objectives {
        string(b, &o.name)?;
        string(b, &o.display)?;
        b.push(u8::from(o.hearts));
        count(b, o.scores.len(), "scores")?;
        for (player, value) in &o.scores {
            string(b, player)?;
            b.extend(value.to_be_bytes());
        }
    }
    count(b, l.slots.len(), "display slots")?;
    for (slot, objective) in &l.slots {
        b.push(*slot);
        string(b, objective)?;
    }
    Ok(())
}

fn teams_part(b: &mut Vec<u8>, l: &Lists) -> io::Result<()> {
    count(b, l.teams.len(), "teams")?;
    for t in &l.teams {
        string(b, &t.name)?;
        string(b, &t.prefix)?;
        string(b, &t.suffix)?;
        string(b, t.visibility)?;
        b.push(t.color.map_or(-1i8, |c| c as i8) as u8);
        count(b, t.members.len(), "team members")?;
        for m in &t.members {
            string(b, m)?;
        }
    }
    Ok(())
}

fn lists_part(l: &Lists) -> io::Result<Vec<u8>> {
    let mut b = Vec::new();
    players_part(&mut b, l)?;
    board_part(&mut b, l)?;
    teams_part(&mut b, l)?;
    if b.len() > MAX_LISTS {
        return Err(io::Error::other(format!("{} bytes of lists", b.len())));
    }
    Ok(b)
}

pub fn write(w: &mut impl Write, l: &Lists) -> io::Result<()> {
    let part = lists_part(l)?;
    let mut b = vec![b'L'];
    b.extend((part.len() as u32).to_be_bytes());
    b.extend(part);
    match &l.boss {
        Some(boss) => {
            b.push(1);
            string(&mut b, &boss.name)?;
            b.extend(boss.health.to_be_bytes());
            b.extend(boss.time.to_be_bytes());
        }
        None => b.push(0),
    }
    string(&mut b, &l.title.title)?;
    string(&mut b, &l.title.subtitle)?;
    for v in [
        l.title.timer,
        l.title.fade_in,
        l.title.stay,
        l.title.fade_out,
    ] {
        b.extend(v.to_be_bytes());
    }
    match &l.action_bar {
        Some((text, ticks)) => {
            b.push(1);
            string(&mut b, text)?;
            b.extend(ticks.to_be_bytes());
        }
        None => b.push(0),
    }
    w.write_all(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(b: &mut Vec<u8>, t: &str) {
        b.extend((t.len() as u32).to_be_bytes());
        b.extend(t.as_bytes());
    }

    #[test]
    fn writes_the_message_java_reads() {
        let l = Lists {
            players: vec![
                Player {
                    uuid: 7,
                    name: "Alex".into(),
                    ping: 12,
                    gamemode: 1,
                    local: true,
                    display: Some("§aAlex".into()),
                },
                Player {
                    uuid: 8,
                    name: "Bo".into(),
                    ..Player::default()
                },
            ],
            header: "top".into(),
            footer: String::new(),
            objectives: vec![Objective {
                name: "o".into(),
                display: "Obj".into(),
                hearts: true,
                scores: vec![("Bo".into(), -3)],
            }],
            slots: vec![(1, "o".into())],
            teams: vec![Team {
                name: "red".into(),
                prefix: "[R] ".into(),
                suffix: String::new(),
                visibility: "never",
                color: Some(12),
                members: vec!["Bo".into()],
            }],
            boss: Some(Boss {
                name: "Wither".into(),
                health: 0.5,
                time: 90,
            }),
            title: Title {
                title: "T".into(),
                subtitle: "S".into(),
                timer: 50,
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            },
            action_bar: Some(("bar".into(), 40)),
        };
        let mut want = Vec::new();
        want.extend(2u16.to_be_bytes());
        want.extend(7u128.to_be_bytes());
        s(&mut want, "Alex");
        want.extend(12i32.to_be_bytes());
        want.extend([1, 0b11]);
        s(&mut want, "§aAlex");
        want.extend(8u128.to_be_bytes());
        s(&mut want, "Bo");
        want.extend(0i32.to_be_bytes());
        want.extend([0, 0]);
        s(&mut want, "top");
        s(&mut want, "");
        want.extend(1u16.to_be_bytes());
        s(&mut want, "o");
        s(&mut want, "Obj");
        want.push(1);
        want.extend(1u16.to_be_bytes());
        s(&mut want, "Bo");
        want.extend((-3i32).to_be_bytes());
        want.extend(1u16.to_be_bytes());
        want.push(1);
        s(&mut want, "o");
        want.extend(1u16.to_be_bytes());
        s(&mut want, "red");
        s(&mut want, "[R] ");
        s(&mut want, "");
        s(&mut want, "never");
        want.push(12);
        want.extend(1u16.to_be_bytes());
        s(&mut want, "Bo");
        let mut full = vec![b'L'];
        full.extend((want.len() as u32).to_be_bytes());
        full.extend(want);
        full.push(1);
        s(&mut full, "Wither");
        full.extend(0.5f32.to_be_bytes());
        full.extend(90i32.to_be_bytes());
        s(&mut full, "T");
        s(&mut full, "S");
        for v in [50i32, 10, 70, 20] {
            full.extend(v.to_be_bytes());
        }
        full.push(1);
        s(&mut full, "bar");
        full.extend(40i32.to_be_bytes());
        let mut out = Vec::new();
        write(&mut out, &l).unwrap();
        assert_eq!(out, full);
    }

    #[test]
    fn empty_lists_and_a_long_text() {
        let mut out = Vec::new();
        write(&mut out, &Lists::default()).unwrap();
        // 'L', the list part (zero players, header, footer, zero objectives, slots, teams), no boss, empty
        // title and subtitle, four times, no action bar.
        assert_eq!(
            out.len(),
            1 + 4 + 2 + 4 + 4 + 2 + 2 + 2 + 1 + 4 + 4 + 16 + 1
        );
        let long = Lists {
            header: "x".repeat(MAX_TEXT + 1),
            ..Lists::default()
        };
        assert!(write(&mut out, &long).is_err());
    }
}
