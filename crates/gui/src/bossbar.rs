//! `BossStatus` and `GuiIngame.renderBossHealth`: the wither/dragon bar at the top center.

use crate::toolkit::{Gui, ICONS};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BossStatus {
    pub name: String,
    /// `healthScale`: health over max health.
    pub health: f32,
    /// `statusBarTime`: frames left; `setBossStatus` sets 100 each frame the boss renders.
    pub time: i32,
}

impl BossStatus {
    /// `setBossStatus`.
    pub fn set(&mut self, name: String, ratio: f32) {
        self.name = name;
        self.health = ratio;
        self.time = 100;
    }

    /// `renderBossHealth`; counts `time` down per frame like vanilla.
    pub fn draw(&mut self, gui: &mut Gui) {
        if self.time <= 0 {
            return;
        }
        self.time -= 1;
        let w = gui.scale.width;
        let k = w / 2 - 91;
        let l = (self.health * 183.0) as i32;
        gui.set_color(0xFFFF_FFFF);
        gui.blit(ICONS, k, 12, 0, 74, 182, 5);
        gui.blit(ICONS, k, 12, 0, 74, 182, 5);
        if l > 0 {
            gui.blit(ICONS, k, 12, 0, 79, l, 5);
        }
        gui.text(
            &self.name,
            (w / 2 - gui.width(&self.name) / 2) as f32,
            2.0,
            0xFF_FFFF,
        );
    }
}
