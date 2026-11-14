//! The HUD lists the mods read: the engine's tab list, scoreboard, boss bar, title and action bar as the
//! `'L'` message of `ferrocraft_modhost::hud_lists`.

use super::App;
use ferrocraft_modhost::hud_lists::{Boss, Lists, Objective, Player, Team, Title};
use gui::scoreboard::Visibility;

impl App {
    /// What the vanilla list classes in the mod host must hold now.
    pub(super) fn mod_lists(&self) -> Lists {
        let hud = &self.hud;
        let board = &hud.scoreboard;
        let mut players: Vec<Player> = hud
            .tablist
            .entries
            .iter()
            .map(|e| Player {
                uuid: e.uuid,
                name: e.name.clone(),
                ping: e.ping,
                gamemode: e.gamemode,
                local: e.name == self.args.username,
                display: e.display.clone(),
            })
            .collect();
        // The scoreboard and the tab list come from hash maps and a packet order: sort, so that a frame with the
        // same lists writes the same bytes and the JVM skips it.
        players.sort_by_key(|p| p.uuid);
        let mut objectives: Vec<Objective> = board
            .objectives()
            .map(|(name, display, hearts)| {
                let mut scores: Vec<_> =
                    board.scores(name).map(|(p, v)| (p.to_owned(), v)).collect();
                scores.sort();
                Objective {
                    name: name.to_owned(),
                    display: display.to_owned(),
                    hearts,
                    scores,
                }
            })
            .collect();
        objectives.sort_by(|a, b| a.name.cmp(&b.name));
        let mut slots: Vec<_> = board.slots().map(|(s, o)| (s, o.to_owned())).collect();
        slots.sort();
        let mut teams: Vec<Team> = board
            .teams()
            .map(|(name, t)| {
                let mut members = t.members.clone();
                members.sort();
                Team {
                    name: name.to_owned(),
                    prefix: t.prefix.clone(),
                    suffix: t.suffix.clone(),
                    visibility: match t.visibility {
                        Visibility::Always => "always",
                        Visibility::Never => "never",
                        Visibility::HideForOtherTeams => "hideForOtherTeams",
                        Visibility::HideForOwnTeam => "hideForOwnTeam",
                    },
                    color: t.color,
                    members,
                }
            })
            .collect();
        teams.sort_by(|a, b| a.name.cmp(&b.name));
        Lists {
            players,
            header: hud.tablist.header.clone(),
            footer: hud.tablist.footer.clone(),
            objectives,
            slots,
            teams,
            boss: (hud.boss.time > 0).then(|| Boss {
                name: hud.boss.name.clone(),
                health: hud.boss.health,
                time: hud.boss.time,
            }),
            title: Title {
                title: hud.title.title.clone(),
                subtitle: hud.title.subtitle.clone(),
                timer: hud.title.timer,
                fade_in: hud.title.fade_in,
                stay: hud.title.stay,
                fade_out: hud.title.fade_out,
            },
            action_bar: hud.action_bar().map(|(t, n)| (t.to_owned(), n)),
        }
    }
}
