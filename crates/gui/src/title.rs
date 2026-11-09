//! `GuiIngame.displayTitle` and the title/subtitle overlay of `renderGameOverlay`.

use crate::toolkit::Gui;

#[derive(Clone, Debug, PartialEq)]
pub struct TitleState {
    pub title: String,
    pub subtitle: String,
    /// `titlesTimer`: ticks left.
    pub timer: i32,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

impl Default for TitleState {
    fn default() -> Self {
        TitleState {
            title: String::new(),
            subtitle: String::new(),
            timer: 0,
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        }
    }
}

impl TitleState {
    /// S45: action 0 title, 1 subtitle, 2 times, 3 hide, 4 reset.
    pub fn set_title(
        &mut self,
        action: u8,
        text: Option<String>,
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    ) {
        match action {
            0 => {
                self.title = text.unwrap_or_default();
                self.timer = self.fade_in + self.stay + self.fade_out;
            }
            1 => self.subtitle = text.unwrap_or_default(),
            2 => {
                if fade_in >= 0 {
                    self.fade_in = fade_in;
                }
                if stay >= 0 {
                    self.stay = stay;
                }
                if fade_out >= 0 {
                    self.fade_out = fade_out;
                }
                if self.timer > 0 {
                    self.timer = self.fade_in + self.stay + self.fade_out;
                }
            }
            3 => {
                self.fade_in = 0;
                self.stay = 0;
                self.fade_out = 0;
                self.timer = 0;
            }
            4 => *self = TitleState::default(),
            _ => {}
        }
    }

    /// `updateTick`.
    pub fn tick(&mut self) {
        if self.timer > 0 {
            self.timer -= 1;
            if self.timer <= 0 {
                self.title.clear();
                self.subtitle.clear();
            }
        }
    }

    pub fn draw(&self, gui: &mut Gui, partial_ticks: f32) {
        if self.timer <= 0 {
            return;
        }
        let f2 = self.timer as f32 - partial_ticks;
        let mut l1 = 255;
        if self.timer > self.fade_out + self.stay {
            let f3 = (self.fade_in + self.stay + self.fade_out) as f32 - f2;
            l1 = (f3 * 255.0 / self.fade_in as f32) as i32;
        }
        if self.timer <= self.fade_out {
            l1 = (f2 * 255.0 / self.fade_out as f32) as i32;
        }
        let l1 = l1.clamp(0, 255) as u32;
        if l1 <= 8 {
            return;
        }
        let color = 0xFF_FFFF | (l1 << 24);
        gui.push();
        gui.translate((gui.scale.width / 2) as f32, (gui.scale.height / 2) as f32);
        gui.push();
        gui.scale(4.0, 4.0);
        let x = -gui.width(&self.title) / 2;
        gui.text(&self.title, x as f32, -10.0, color);
        gui.pop();
        gui.push();
        gui.scale(2.0, 2.0);
        let x = -gui.width(&self.subtitle) / 2;
        gui.text(&self.subtitle, x as f32, 5.0, color);
        gui.pop();
        gui.pop();
    }
}
