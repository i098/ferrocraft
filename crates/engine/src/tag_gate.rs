//! Which parts of a name tag are drawn (`Render.renderLivingLabel`, `LivingEntityRenderer.shouldRenderNameTag`).

/// `None` hides the tag. `Some(solid)` draws the 25% plate and faint text (vanilla: depth test off), plus the
/// solid text when `solid` (vanilla: depth test on).
///
/// With the HUD hidden (F1) only team players keep their tag. A sneaking tag keeps the depth test, so a block on
/// the line of sight hides it whole; a standing tag shows plate and faint text anyway. `blocked` is a raycast,
/// so it runs only for tags that survive the F1 filter.
pub fn parts(
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
