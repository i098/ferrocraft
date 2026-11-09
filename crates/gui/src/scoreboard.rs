//! Client `Scoreboard` (objectives, scores, display slots, teams) and `GuiIngame.renderScoreboard`.

use crate::toolkit::{Gui, FONT_HEIGHT};
use std::collections::HashMap;

/// Display slot 0: the tab list. 1: the sidebar.
pub const SLOT_LIST: u8 = 0;
pub const SLOT_SIDEBAR: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Team {
    pub prefix: String,
    pub suffix: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scoreboard {
    /// Objective name to display name.
    objectives: HashMap<String, String>,
    /// Objective name to player name to points.
    scores: HashMap<String, HashMap<String, i32>>,
    /// Display slot to objective name.
    display: HashMap<u8, String>,
    teams: HashMap<String, Team>,
}

impl Scoreboard {
    /// S3B: mode 0 creates, 1 removes, 2 renames the objective.
    pub fn set_objective(&mut self, name: &str, display: &str, mode: u8) {
        if mode == 1 {
            self.objectives.remove(name);
            self.scores.remove(name);
            self.display.retain(|_, o| o != name);
        } else {
            self.objectives.insert(name.into(), display.into());
        }
    }

    /// S3C: `remove` with an empty `objective` drops the player from every objective.
    pub fn set_score(&mut self, player: &str, objective: &str, value: i32, remove: bool) {
        if !remove {
            if self.objectives.contains_key(objective) {
                let s = self.scores.entry(objective.into()).or_default();
                s.insert(player.into(), value);
            }
        } else if objective.is_empty() {
            self.scores.values_mut().for_each(|s| {
                s.remove(player);
            });
        } else if let Some(s) = self.scores.get_mut(objective) {
            s.remove(player);
        }
    }

    /// S3D: an empty name clears the slot.
    pub fn set_display(&mut self, position: u8, name: &str) {
        if name.is_empty() {
            self.display.remove(&position);
        } else {
            self.display.insert(position, name.into());
        }
    }

    /// S3E: mode 0 create, 1 remove, 2 update prefix/suffix, 3 add members, 4 remove members.
    pub fn set_team(
        &mut self,
        name: &str,
        mode: u8,
        prefix: &str,
        suffix: &str,
        members: &[String],
    ) {
        if mode == 1 {
            self.teams.remove(name);
            return;
        }
        if mode == 0 || mode == 3 {
            // A player is on at most one team.
            for t in self.teams.values_mut() {
                t.members.retain(|m| !members.contains(m));
            }
        }
        let t = self.teams.entry(name.into()).or_default();
        if mode == 0 || mode == 2 {
            t.prefix = prefix.into();
            t.suffix = suffix.into();
        }
        match mode {
            0 | 3 => t.members.extend(members.iter().cloned()),
            4 => t.members.retain(|m| !members.contains(m)),
            _ => {}
        }
    }

    /// The team a player is on, by name.
    pub fn team_of(&self, player: &str) -> Option<(&str, &Team)> {
        self.teams
            .iter()
            .find(|(_, t)| t.members.iter().any(|m| m == player))
            .map(|(n, t)| (n.as_str(), t))
    }

    /// `ScorePlayerTeam.formatPlayerName`.
    pub fn format_name(&self, player: &str) -> String {
        match self.team_of(player) {
            Some((_, t)) => format!("{}{player}{}", t.prefix, t.suffix),
            None => player.into(),
        }
    }

    /// The objective in a display slot.
    pub fn objective_in(&self, slot: u8) -> Option<&str> {
        let name = self.display.get(&slot)?;
        self.objectives.contains_key(name).then_some(name.as_str())
    }

    /// A player's points in an objective.
    pub fn score(&self, objective: &str, player: &str) -> Option<i32> {
        self.scores.get(objective)?.get(player).copied()
    }

    /// `getSortedScores`: ascending points, ties by name descending (case-insensitive).
    fn sorted(&self, objective: &str) -> Vec<(&str, i32)> {
        let mut v: Vec<(&str, i32)> = self
            .scores
            .get(objective)
            .map(|m| m.iter().map(|(n, p)| (n.as_str(), *p)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.0.to_lowercase().cmp(&a.0.to_lowercase()))
        });
        v
    }

    /// `renderScoreboard` for the sidebar objective (slot 1).
    pub fn draw_sidebar(&self, gui: &mut Gui) {
        // shortcut: the team-color sidebar slots 3..=18 are ignored; add when the engine tracks team colors.
        let Some(obj) = self.objective_in(SLOT_SIDEBAR) else {
            return;
        };
        let title = &self.objectives[obj];
        let all = self.sorted(obj);
        let list: Vec<_> = all.iter().filter(|(n, _)| !n.starts_with('#')).collect();
        // Vanilla skips by the unfiltered count.
        let rows: Vec<_> = if list.len() > 15 {
            list.iter().skip(all.len() - 15).collect()
        } else {
            list.iter().collect()
        };
        let mut i = gui.width(title);
        for (n, p) in &rows {
            i = i.max(gui.width(&format!("{}: \u{a7}c{p}", self.format_name(n))));
        }
        let (w, h) = (gui.scale.width, gui.scale.height);
        let j1 = h / 2 + rows.len() as i32 * FONT_HEIGHT / 3;
        let k1 = 3;
        let l1 = w - i - k1;
        let l = w - k1 + 2;
        // `drawString` with blending off: the 0x20 alpha of 553648127 does not show.
        let white = 0x00FF_FFFF;
        for (j, (n, p)) in rows.iter().enumerate() {
            let j = j as i32 + 1;
            let name = self.format_name(n);
            let points = format!("\u{a7}c{p}");
            let k = j1 - j * FONT_HEIGHT;
            gui.fill(l1 - 2, k, l, k + FONT_HEIGHT, 1_342_177_280);
            gui.text_plain(&name, l1 as f32, k as f32, white);
            gui.text_plain(&points, (l - gui.width(&points)) as f32, k as f32, white);
            if j == rows.len() as i32 {
                gui.fill(l1 - 2, k - FONT_HEIGHT - 1, l, k - 1, 1_610_612_736);
                gui.fill(l1 - 2, k - 1, l, k, 1_342_177_280);
                let x = l1 + i / 2 - gui.width(title) / 2;
                gui.text_plain(title, x as f32, (k - FONT_HEIGHT) as f32, white);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_sort_and_teams_format() {
        let mut s = Scoreboard::default();
        s.set_objective("o", "Obj", 0);
        s.set_display(SLOT_SIDEBAR, "o");
        s.set_score("b", "o", 1, false);
        s.set_score("a", "o", 1, false);
        s.set_score("c", "o", 0, false);
        assert_eq!(s.sorted("o"), [("c", 0), ("b", 1), ("a", 1)]);
        s.set_team("t", 0, "[", "]", &["a".into()]);
        assert_eq!(s.format_name("a"), "[a]");
        s.set_team("t", 4, "", "", &["a".into()]);
        assert_eq!(s.format_name("a"), "a");
        s.set_objective("o", "", 1);
        assert_eq!(s.objective_in(SLOT_SIDEBAR), None);
    }
}
