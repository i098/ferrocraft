//! The sneaking pose of a player: `ModelBiped.render` lowers the body, `RenderPlayer.doRender` the name tag.

use super::{Class, EntityDraw, Part};

/// `ModelBiped.render` and `ModelPlayer.render`: a sneaking biped is drawn `translate(0, 0.2, 0)` lower, in model pixels.
const LOWER: f32 = 0.2 * 16.0;

/// Lowers the parts of an adult sneaking biped; a child's `ModelBiped.render` has no such translate.
pub(super) fn lower(d: &EntityDraw, parts: &mut [Part]) {
    if d.sneaking && !d.child {
        for part in parts {
            part.lower = LOWER;
        }
    }
}

/// How far a sneaking tag hangs lower: `Render.renderLivingLabel` takes 0.25, and `RenderPlayer.doRender`
/// passes `y - 0.125` on for every player that is not the `EntityPlayerSP`.
pub(super) fn tag_drop(d: &EntityDraw) -> f64 {
    match (
        d.sneaking,
        matches!(d.class, Class::Player { .. }) && !d.look.local,
    ) {
        (false, _) => 0.0,
        (true, false) => 0.25,
        (true, true) => 0.375,
    }
}
