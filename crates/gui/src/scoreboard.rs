//! Client `Scoreboard` (objectives, scores, display slots, teams) and `GuiIngame.renderScoreboard`.

use crate::toolkit::{Gui, FONT_HEIGHT};
use std::collections::HashMap;

/// Display slot 0: the tab list. 1: the sidebar. 2: below the name. 3 to 18: the sidebar of one team color.
pub const SLOT_LIST: u8 = 0;
pub const SLOT_SIDEBAR: u8 = 1;
pub const SLOT_BELOW_NAME: u8 = 2;
const SLOT_TEAM_COLOR: u8 = 3;

/// `Team.Visibility`: who sees the name tags of a team's members.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Visibility {
    #[default]
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl Visibility {
    /// `Visibility.byKey`; vanilla crashes on an unknown key, this shows the tag.
    fn by_key(key: &str) -> Self {
        match key {
            "never" => Self::Never,
            "hideForOtherTeams" => Self::HideForOtherTeams,
            "hideForOwnTeam" => Self::HideForOwnTeam,
            _ => Self::Always,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Team {
    pub prefix: String,
    pub suffix: String,
    pub visibility: Visibility,
    /// `Formatting.getId` of the team color; `None` is reset (-1).
    pub color: Option<u8>,
    pub members: Vec<String>,
}

/// The fields of an S3E in modes 0 and 2.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TeamInfo {
    pub prefix: String,
    pub suffix: String,
    /// `nameTagVisibility` key.
    pub visibility: String,
    pub color: i8,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Objective {
    display: String,
    /// The `hearts` render type.
    hearts: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scoreboard {
    objectives: HashMap<String, Objective>,
    /// Objective name to player name to points.
    scores: HashMap<String, HashMap<String, i32>>,
    /// Display slot to objective name.
    display: HashMap<u8, String>,
    teams: HashMap<String, Team>,
}

impl Scoreboard {
    /// S3B: mode 0 creates, 1 removes, 2 renames the objective.
    pub fn set_objective(&mut self, name: &str, display: &str, mode: u8, hearts: bool) {
        if mode == 1 {
            self.objectives.remove(name);
            self.scores.remove(name);
            self.display.retain(|_, o| o != name);
        } else {
            let display = display.into();
            self.objectives
                .insert(name.into(), Objective { display, hearts });
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
    pub fn set_team(&mut self, name: &str, mode: u8, info: &TeamInfo, members: &[String]) {
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
            t.prefix = info.prefix.clone();
            t.suffix = info.suffix.clone();
            t.visibility = Visibility::by_key(&info.visibility);
            t.color = u8::try_from(info.color).ok();
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

    /// `RendererLivingEntity.canRenderName` for a player on a team: `Some` when the team decides whether
    /// `player`'s tag shows to `me`, `None` when the player has no team.
    pub fn name_tag_shown(&self, player: &str, me: &str) -> Option<bool> {
        let (team, t) = self.team_of(player)?;
        let mine = self.team_of(me).map(|(n, _)| n);
        let same = |m: &str| m == team;
        Some(match t.visibility {
            Visibility::Always => true,
            Visibility::Never => false,
            Visibility::HideForOtherTeams => mine.is_none_or(same),
            Visibility::HideForOwnTeam => mine.is_none_or(|m| !same(m)),
        })
    }

    /// `RenderPlayer.renderNameTag`: the line below a player's tag, `"<score> <objective>"`.
    pub fn below_name(&self, player: &str) -> Option<String> {
        let o = self.objective_in(SLOT_BELOW_NAME)?;
        let score = self.score(o, player).unwrap_or(0);
        Some(format!("{score} {}", self.objectives[o].display))
    }

    /// Whether an objective uses the `hearts` render type.
    pub fn is_hearts(&self, objective: &str) -> bool {
        self.objectives.get(objective).is_some_and(|o| o.hearts)
    }

    /// Every objective as name, display name and `hearts`, in no order.
    pub fn objectives(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.objectives
            .iter()
            .map(|(n, o)| (n.as_str(), o.display.as_str(), o.hearts))
    }

    /// The points of every player in an objective, in no order.
    pub fn scores(&self, objective: &str) -> impl Iterator<Item = (&str, i32)> {
        self.scores
            .get(objective)
            .into_iter()
            .flat_map(|m| m.iter().map(|(n, p)| (n.as_str(), *p)))
    }

    /// The display slots that hold an objective, with its name, in no order.
    pub fn slots(&self) -> impl Iterator<Item = (u8, &str)> {
        self.display
            .iter()
            .map(|(s, o)| (*s, o.as_str()))
            .filter(|(_, o)| self.objectives.contains_key(*o))
    }

    /// Every team with its name, in no order.
    pub fn teams(&self) -> impl Iterator<Item = (&str, &Team)> {
        self.teams.iter().map(|(n, t)| (n.as_str(), t))
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

    /// `renderScoreboard`: the objective of the local player's team color (slots 3 to 18), else slot 1.
    pub fn draw_sidebar(&self, gui: &mut Gui, me: &str) {
        let colored = self
            .team_of(me)
            .and_then(|(_, t)| t.color)
            .and_then(|c| self.objective_in(SLOT_TEAM_COLOR + c));
        let Some(obj) = colored.or_else(|| self.objective_in(SLOT_SIDEBAR)) else {
            return;
        };
        let title = &self.objectives[obj].display;
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
        s.set_objective("o", "Obj", 0, false);
        s.set_display(SLOT_SIDEBAR, "o");
        s.set_score("b", "o", 1, false);
        s.set_score("a", "o", 1, false);
        s.set_score("c", "o", 0, false);
        assert_eq!(s.sorted("o"), [("c", 0), ("b", 1), ("a", 1)]);
        let info = TeamInfo {
            prefix: "[".into(),
            suffix: "]".into(),
            ..TeamInfo::default()
        };
        s.set_team("t", 0, &info, &["a".into()]);
        assert_eq!(s.format_name("a"), "[a]");
        s.set_team("t", 4, &TeamInfo::default(), &["a".into()]);
        assert_eq!(s.format_name("a"), "a");
        s.set_objective("o", "", 1, false);
        assert_eq!(s.objective_in(SLOT_SIDEBAR), None);
    }

    /// `canRenderName`: the four `Team.Visibility` modes for a viewer on the same team, on another team and on none.
    #[test]
    fn name_tag_visibility_modes() {
        let mut s = Scoreboard::default();
        let team = |s: &mut Scoreboard, n: &str, vis: &str, who: &str| {
            let info = TeamInfo {
                visibility: vis.into(),
                ..TeamInfo::default()
            };
            s.set_team(n, 0, &info, &[who.into()]);
        };
        team(&mut s, "red", "always", "r1");
        team(&mut s, "blue", "never", "b1");
        team(&mut s, "green", "hideForOtherTeams", "g1");
        team(&mut s, "gold", "hideForOwnTeam", "y1");
        s.set_team("green", 3, &TeamInfo::default(), &["g2".into()]);
        s.set_team("gold", 3, &TeamInfo::default(), &["y2".into()]);
        // Viewers: "g2" is on green, "y2" on gold, "nobody" has no team.
        let shown = |p, me| s.name_tag_shown(p, me);
        assert_eq!(shown("r1", "g2"), Some(true));
        assert_eq!(shown("b1", "b1x"), Some(false));
        assert_eq!(shown("g1", "g2"), Some(true));
        assert_eq!(shown("g1", "y2"), Some(false));
        assert_eq!(shown("g1", "nobody"), Some(true));
        assert_eq!(shown("y1", "y2"), Some(false));
        assert_eq!(shown("y1", "g2"), Some(true));
        assert_eq!(shown("y1", "nobody"), Some(true));
        assert_eq!(shown("loner", "g2"), None);
    }

    #[test]
    fn below_name_line_and_hearts() {
        let mut s = Scoreboard::default();
        assert_eq!(s.below_name("a"), None);
        s.set_objective("hp", "\u{a7}cHP", 0, true);
        s.set_display(SLOT_BELOW_NAME, "hp");
        assert_eq!(s.below_name("a").as_deref(), Some("0 \u{a7}cHP"));
        s.set_score("a", "hp", 17, false);
        assert_eq!(s.below_name("a").as_deref(), Some("17 \u{a7}cHP"));
        assert!(s.is_hearts("hp"));
        s.set_objective("hp", "HP", 2, false);
        assert!(!s.is_hearts("hp"));
    }
}
