//! Which parts of a name tag are drawn (`Render.renderLivingLabel`, `LivingEntityRenderer.shouldRenderNameTag`).

use crate::game::Game;
use render::{Camera, EntityDraw};

/// `Render.renderLivingLabel`: a 25% black plate and the name at vanilla's world scale. Vanilla draws the text twice,
/// faint without the depth test and solid with it; the overlay has no depth buffer, so a block on the line of
/// sight to the tag stands in for the depth test.
pub fn draw(
    g: &mut gui::Gui,
    game: &Game,
    cam: &Camera,
    draws: &[EntityDraw],
    (w, h): (u32, u32),
    factor: f32,
    [in_world, labels_owned, show_hud]: [bool; 3],
) {
    let Some(player) = game.player.as_ref().filter(|_| in_world && !labels_owned) else {
        return;
    };
    let vp = render::view_proj(cam, w as f32 / h as f32);
    for tag in render::nametags(draws, player.pos, &vp, cam.fov_deg, w as f32, h as f32) {
        let blocked = || game.line_blocked(cam.pos, tag.anchor);
        let Some(solid) = parts(tag.sneaking, blocked, show_hud, tag.team) else {
            continue;
        };
        let half = g.width(&tag.text) / 2;
        g.push();
        g.translate(tag.screen[0] / factor, tag.screen[1] / factor);
        g.scale(tag.scale / factor, tag.scale / factor);
        g.fill(-half - 1, -1, half + 1, 8, 0x4000_0000);
        g.text_plain(&tag.text, -half as f32, 0.0, 0x20FF_FFFF);
        if solid {
            g.text_plain(&tag.text, -half as f32, 0.0, 0xFFFF_FFFF);
        }
        g.pop();
    }
}

/// `None` hides the tag. `Some(solid)` draws the 25% plate and faint text (vanilla: depth test off), plus the
/// solid text when `solid` (vanilla: depth test on).
///
/// With the HUD hidden (F1) only team players keep their tag. A sneaking tag keeps the depth test, so a block on
/// the line of sight hides it whole; a standing tag shows plate and faint text anyway. `blocked` is a raycast,
/// so it runs only for tags that survive the F1 filter.
fn parts(
    sneaking: bool,
    blocked: impl FnOnce() -> bool,
    show_hud: bool,
    team_shown: bool,
) -> Option<bool> {
    if !show_hud && !team_shown {
        return None;
    }
    let blocked = blocked();
    if sneaking && blocked {
        return None;
    }
    Some(!sneaking && !blocked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sneaking_behind_a_wall_hides_everything() {
        assert_eq!(parts(true, || true, true, false), None);
    }

    #[test]
    fn sneaking_in_view_has_no_solid_text() {
        assert_eq!(parts(true, || false, true, false), Some(false));
    }

    #[test]
    fn standing_behind_a_wall_keeps_plate_and_faint_text() {
        assert_eq!(parts(false, || true, true, false), Some(false));
    }

    #[test]
    fn standing_in_view_draws_solid_text() {
        assert_eq!(parts(false, || false, true, false), Some(true));
    }

    #[test]
    fn f1_keeps_team_tags_and_hides_others() {
        assert_eq!(parts(false, || false, false, true), Some(true));
        assert_eq!(parts(false, || false, false, false), None);
    }

    #[test]
    fn f1_team_tag_still_obeys_sneak_occlusion() {
        assert_eq!(parts(true, || true, false, true), None);
    }
}
