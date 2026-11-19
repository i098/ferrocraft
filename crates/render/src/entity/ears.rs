//! `LayerDeadmau5Head`: the ears and the raised name tag of the player named `deadmau5`.

use super::{emit_part, EntityDraw, Model, Part, Skins, Xf};
use crate::mesh::MeshData;

const NAME: &str = "deadmau5";

/// `Render.renderLivingLabel`: the text of "deadmau5" starts 10 font pixels higher.
pub(super) fn tag_lift(text: &str) -> f32 {
    if text == NAME {
        10.0
    } else {
        0.0
    }
}

/// Two 6x6x1 ears (skin texture 24, 0) scaled 4/3, at (+-6, -6) px from the head pivot in the head's
/// own rotation; the layer undoes the head rotation before the ear model applies it again. They draw
/// only for a visible player with a skin.
pub(super) fn emit(out: &mut MeshData, xf: &Xf, m: &Model, d: &EntityDraw, skins: &Skins) {
    let Some(own) = d.skin.and_then(|k| skins.player(k)) else {
        return;
    };
    if d.name.as_deref() != Some(NAME) || d.invisible {
        return;
    }
    let head = &m.parts[0];
    for side in [-1.0, 1.0] {
        let mut ear = Part::new(
            [4.5 * side, -4.5, 0.0],
            [-3.0, -6.0, -1.0],
            [6.0, 6.0, 1.0],
            (24.0, 0.0),
        );
        ear.scale = 4.0 / 3.0;
        ear.parents = vec![([0.0; 3], head.rot)];
        emit_part(out, xf, &ear, 0.0, [1.0; 3], skins.v_offset(own));
    }
}
