//! What surrounds the camera: the liquid it sits in and the block it sits inside, ported from
//! `Camera.getBlockInside`, `Entity.isSubmergedIn` and `ItemInHandRenderer.renderScreenEffects`.

use super::App;
use crate::game::eye_height;
use render::{Blend, Fluid};

const WATER: [u16; 2] = [8, 9];
const LAVA: [u16; 2] = [10, 11];

/// `Entity.width` of the player.
const WIDTH: f64 = 0.6;

fn fluid_of(state: u16) -> Option<Fluid> {
    match state >> 4 {
        id if WATER.contains(&id) => Some(Fluid::Water),
        id if LAVA.contains(&id) => Some(Fluid::Lava),
        _ => None,
    }
}

/// `LiquidBlock.getHeightLoss(level) - 1/9`: how far the surface lies below the top of the block.
fn surface_drop(state: u16) -> f64 {
    let level = i32::from(state & 15);
    let level = if level >= 8 { 0 } else { level };
    f64::from((level + 1) as f32 / 9.0 - 0.111_111_11)
}

fn cell(v: f64) -> i32 {
    v.floor() as i32
}

/// The state at `at` with the surface of a liquid cell cut off: above the surface the block
/// over it counts, so a camera in the air layer of a half-full cell is not underwater.
fn state_at_surface(get: &impl Fn(i32, i32, i32) -> u16, at: [f64; 3]) -> u16 {
    let [x, y, z] = [cell(at[0]), cell(at[1]), cell(at[2])];
    let state = get(x, y, z);
    if fluid_of(state).is_some() && at[1] >= f64::from(y + 1) - surface_drop(state) {
        return get(x, y + 1, z);
    }
    state
}

/// `Camera.getBlockInside` reduced to its liquid: what the camera at `at` is in.
pub fn fluid_inside(get: &impl Fn(i32, i32, i32) -> u16, at: [f64; 3]) -> Option<Fluid> {
    fluid_of(state_at_surface(get, at))
}

/// `Entity.isSubmergedIn(WATER)` for the player: the eye below the water surface.
pub fn submerged_in_water(
    get: &impl Fn(i32, i32, i32) -> u16,
    feet: [f64; 3],
    eye_height: f64,
) -> bool {
    let eye = feet[1] + eye_height;
    let (x, y, z) = (cell(feet[0]), cell(eye), cell(feet[2]));
    let state = get(x, y, z);
    fluid_of(state) == Some(Fluid::Water) && eye < f64::from(y + 1) - surface_drop(state)
}

/// The view-blocking block the eye is inside: of the eight points around it, the last one
/// that holds an opaque cube (`isViewBlocking`), as `renderScreenEffects` picks it.
pub fn wall_block(
    get: &impl Fn(i32, i32, i32) -> u16,
    opaque: impl Fn(u16) -> bool,
    feet: [f64; 3],
    eye_height: f64,
) -> Option<u16> {
    let off = |bit: usize, size: f64| (((bit % 2) as f64) - 0.5) * size;
    (0..8usize)
        .map(|i| {
            get(
                cell(feet[0] + off(i, WIDTH * 0.8)),
                cell(feet[1] + off(i >> 1, 0.1) + eye_height),
                cell(feet[2] + off(i >> 2, WIDTH * 0.8)),
            )
        })
        .filter(|s| opaque(*s))
        .last()
}

impl App {
    /// The liquid at the camera position, with a half-full cell cut at its surface.
    pub(super) fn fluid_at(&self, at: [f64; 3]) -> Option<Fluid> {
        fluid_inside(&|x, y, z| self.game.store.get_block(x, y, z), at)
    }

    /// `ItemInHandRenderer.renderScreenEffects`: the block the eye is inside, then the water
    /// overlay. Drawn under the HUD, in first person only.
    pub(super) fn draw_screen_effects(&self, g: &mut gui::Gui, size: (u32, u32), frac: f32) {
        let Some(p) = &self.game.player else { return };
        if self.perspective != 0 || self.game.gamemode == 3 || !self.in_world() {
            return;
        }
        let get = |x, y, z| self.game.store.get_block(x, y, z);
        let eye = eye_height(self.input.sneaking());
        // The overlay quad sits 0.5 ahead of the eye, so the screen shows its central part only.
        let reach = 0.5 * (self.camera(f64::from(frac)).hand_fov_deg.to_radians() / 2.0).tan();
        let (hy, hx) = (reach, reach * size.0 as f32 / size.1 as f32);
        let (w, h) = (g.scale.width as f32, g.scale.height as f32);
        g.set_blend(Blend::Alpha);
        if let Some(state) = wall_block(&get, render::is_view_blocking, p.pos, eye) {
            let uv = [
                0.5 + hx / 2.0,
                0.5 - hy / 2.0,
                0.5 - hx / 2.0,
                0.5 + hy / 2.0,
            ];
            g.blit_tinted(
                render::block_texture(state),
                0.0,
                0.0,
                w,
                h,
                uv,
                0x801A_1A1A,
            );
        }
        if submerged_in_water(&get, p.pos, eye) {
            let (du, dv) = (-p.yaw / 64.0, self.game.pitch / 64.0);
            let uv = [
                2.0 + 2.0 * hx + du,
                2.0 - 2.0 * hy + dv,
                2.0 - 2.0 * hx + du,
                2.0 + 2.0 * hy + dv,
            ];
            // shortcut: brightness 1, as `Frame::brightness`; vanilla scales by the light at the eye.
            g.blit_tinted(
                "textures/misc/underwater.png",
                0.0,
                0.0,
                w,
                h,
                uv,
                0x80FF_FFFF,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Still water (level 0) fills every cell below y = 64.
    fn pool(_: i32, y: i32, _: i32) -> u16 {
        if y < 64 {
            9 << 4
        } else {
            0
        }
    }

    #[test]
    fn camera_below_the_surface_is_underwater_and_above_it_is_not() {
        assert_eq!(fluid_inside(&pool, [0.5, 63.5, 0.5]), Some(Fluid::Water));
        assert_eq!(fluid_inside(&pool, [0.5, 64.5, 0.5]), None);
    }

    #[test]
    fn player_is_submerged_only_while_the_eye_is_below_the_surface() {
        assert!(submerged_in_water(&pool, [0.5, 62.0, 0.5], 1.62));
        assert!(!submerged_in_water(&pool, [0.5, 62.5, 0.5], 1.62));
    }

    #[test]
    fn wall_block_is_none_in_open_air_and_found_in_stone() {
        let air = |_: i32, _: i32, _: i32| 0u16;
        let stone = |_: i32, _: i32, _: i32| 1u16 << 4;
        let opaque = |s: u16| s >> 4 == 1;
        assert_eq!(wall_block(&air, opaque, [0.5, 70.0, 0.5], 1.62), None);
        assert_eq!(
            wall_block(&stone, opaque, [0.5, 70.0, 0.5], 1.62),
            Some(1 << 4)
        );
    }
}
