//! The silhouettes of `WorldRenderer.renderEntities` when `shouldRenderEntityOutlines` holds:
//! `LivingEntityRenderer.setupSolidState` draws only the model of a player, as flat boxes in the
//! color of the team prefix (white without one). Armor, held items, the cape, the shadow and the name
//! tag draw only in the normal pass.

use super::*;

/// Flat boxes of every player in `draws`, one RGB color per player and no per-face shade.
pub(crate) fn build(draws: &[EntityDraw], skins: &Skins) -> MeshData {
    UNSHADED.set(true);
    let mut out = MeshData::default();
    for d in draws
        .iter()
        .filter(|d| matches!(d.class, Class::Player { .. }))
    {
        silhouette(&mut out, d, skins);
    }
    UNSHADED.set(false);
    out
}

/// `setupSolidState`: the 8-bit channels of the color, as `color4f` takes them.
fn solid_color(d: &EntityDraw) -> V3 {
    let rgb = d.team_tag.outline.unwrap_or(0x00FF_FFFF);
    [16, 8, 0].map(|shift| (rgb >> shift & 255) as f32 / 255.0)
}

/// The same parts as [`draw_body`] boxes, with the texture off.
fn silhouette(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let m = model(d);
    let (skin, _) = base_look(&m, d, skins);
    let xf = Xf::new(d, false);
    let color = solid_color(d);
    for (i, part) in m.parts.iter().enumerate() {
        let shown = player_part_bit(i).is_none_or(|bit| d.look.skin_parts & bit != 0);
        if !d.invisible && shown && (part.inflate == 0.0 || skin != Skin::White) {
            emit_part(out, &xf, part, part.inflate, color, 0.0);
        }
    }
}
