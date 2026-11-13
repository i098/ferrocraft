//! Full-screen overlays of `GuiIngame` (pumpkin blur, nether portal swirl) and the `timeInPortal`
//! timer of `EntityPlayerSP.onLivingUpdate` that also drives the nausea camera wobble.

use crate::toolkit::Gui;

pub const PUMPKIN: &str = "textures/misc/pumpkinblur.png";
/// The animated strip of the portal block: 32 square frames, one tick each (`"animation": {}`).
pub const PORTAL: &str = "textures/blocks/portal.png";
const PORTAL_FRAMES: i32 = 32;

/// `timeInPortal`, `prevTimeInPortal` and `inPortal` of the local player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Portal {
    time: f32,
    prev: f32,
    in_portal: bool,
}

impl Portal {
    /// `Entity.onPortalCollision` on the client, called for each nether portal block the box overlaps.
    pub fn touch(&mut self) {
        self.in_portal = true;
    }

    /// The timer part of `EntityPlayerSP.onLivingUpdate`; `nausea_ticks` is the duration left of the
    /// Nausea effect. Call before the movement of the tick, which may `touch` again. Returns true
    /// when the player has just entered the portal (`timeInPortal == 0`): the tick plays the trigger sound.
    pub fn tick(&mut self, nausea_ticks: Option<i32>) -> bool {
        self.prev = self.time;
        let mut entered = false;
        if self.in_portal {
            entered = self.time == 0.0;
            self.time = (self.time + 0.0125).min(1.0);
            self.in_portal = false;
        } else if nausea_ticks.is_some_and(|d| d > 60) {
            self.time = (self.time + 0.006_666_667).min(1.0);
        } else {
            if self.time > 0.0 {
                self.time -= 0.05;
            }
            self.time = self.time.max(0.0);
        }
        entered
    }

    /// `prevTimeInPortal + (timeInPortal - prevTimeInPortal) * partialTicks`.
    pub fn lerp(&self, partial: f32) -> f32 {
        self.prev + (self.time - self.prev) * partial
    }
}

/// The camera wobble of `EntityRenderer.setupCameraTransform` for the lerped portal time `t` (> 0):
/// the angle in degrees about the axis (0, 1, 1) and the x scale of the squash between the two rotations.
/// `ticks` is `EntityRenderer.ticks`.
pub fn wobble(t: f32, nausea: bool, ticks: i32, partial: f32) -> [f32; 2] {
    let speed = if nausea { 7 } else { 20 };
    let f = 5.0 / (t * t + 5.0) - t * 0.04;
    let f = f * f;
    [(ticks as f32 + partial) * speed as f32, 1.0 / f]
}

/// `renderPumpkinOverlay`.
pub fn draw_pumpkin(gui: &mut Gui) {
    let (w, h) = (gui.scale.width as f32, gui.scale.height as f32);
    gui.blit_tinted(PUMPKIN, 0.0, 0.0, w, h, [0.0, 0.0, 1.0, 1.0], 0xFFFF_FFFF);
}

/// `renderNauseaOverlay` (the portal swirl): `time` is the lerped `timeInPortal`, `tick` the HUD tick count.
pub fn draw_portal(gui: &mut Gui, time: f32, tick: i32) {
    let mut a = time;
    if a < 1.0 {
        a *= a;
        a *= a;
        a = a * 0.8 + 0.2;
    }
    let (w, h) = (gui.scale.width as f32, gui.scale.height as f32);
    let frame = tick.rem_euclid(PORTAL_FRAMES) as f32;
    let (v0, v1) = (
        frame / PORTAL_FRAMES as f32,
        (frame + 1.0) / PORTAL_FRAMES as f32,
    );
    let alpha = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
    gui.blit_tinted(
        PORTAL,
        0.0,
        0.0,
        w,
        h,
        [0.0, v0, 1.0, v1],
        alpha << 24 | 0xFF_FFFF,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_in_a_portal_fills_the_timer_in_81_ticks_and_it_drains_in_21() {
        let mut p = Portal::default();
        for _ in 0..81 {
            p.touch();
            p.tick(None);
        }
        assert_eq!(p.lerp(1.0), 1.0);
        for _ in 0..21 {
            p.tick(None);
        }
        assert_eq!(p.lerp(1.0), 0.0);
    }

    #[test]
    fn nausea_fills_the_timer_slower_and_stops_above_60_ticks_left() {
        let mut p = Portal::default();
        p.tick(Some(61));
        assert_eq!(p.lerp(1.0), 0.006_666_667);
        p.tick(Some(60));
        assert_eq!(p.lerp(1.0), 0.0);
    }

    #[test]
    fn entering_a_portal_triggers_once_and_staying_in_it_only_advances() {
        let mut p = Portal::default();
        p.touch();
        assert!(p.tick(None));
        p.touch();
        assert!(!p.tick(None));
        assert_eq!(p.lerp(1.0), 0.025);
    }

    /// `(ticks + partial) * 20` and `1 / (5 / (t * t + 5) - t * 0.04)^2`, read from `GameRenderer.setupCamera`.
    #[test]
    fn wobble_angle_follows_the_tick_counter_and_the_effect_speed() {
        let [a0, _] = wobble(0.5, false, 10, 0.0);
        let [a1, _] = wobble(0.5, false, 11, 0.5);
        assert_eq!((a0, a1), (200.0, 230.0));
        let [n, s] = wobble(1.0, true, 10, 0.0);
        assert_eq!(n, 70.0);
        let f = 5.0f32 / 6.0 - 0.04;
        assert_eq!(s, 1.0 / (f * f));
    }
}
