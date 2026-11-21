//! `LayerCustomHead` (`WornSkullLayer`): the helmet slot of a player, zombie, zombie pigman,
//! skeleton or armor stand, when it holds a block or a skull. A block is drawn by `item::head`, a
//! skull by `blockent::build_worn_heads`.

use super::{Class, EntityDraw};
use crate::blockent::WornHead;

/// `Items.skull`.
pub(crate) const SKULL: i16 = 397;

/// Renderers that add the layer: `RenderPlayer`, `RenderZombie`, `RenderPigZombie`,
/// `RenderSkeleton` (all `RenderBiped`) and `RenderArmorStand`.
fn has_layer(d: &EntityDraw) -> bool {
    matches!(
        d.class,
        Class::Player { .. } | Class::Mob(54 | 57 | 51) | Class::Object(78)
    )
}

/// The helmet as a block item (`ItemBlock`, ids below 256) with its damage. The pumpkin item model
/// has no variants, so it takes the carved front that `LayerSnowmanHead` shows whatever the damage.
pub(super) fn block(d: &EntityDraw) -> Option<(i16, i16)> {
    let helmet = d.armor[3];
    if !has_layer(d) || !(1..256).contains(&helmet) {
        return None;
    }
    Some(if helmet == super::snowman::PUMPKIN.0 {
        super::snowman::PUMPKIN
    } else {
        (helmet, d.look.stand.head_damage)
    })
}

/// The skulls worn this frame. Armor stands keep their own pose and are not drawn here.
pub fn worn_heads(draws: &[EntityDraw]) -> Vec<WornHead> {
    draws
        .iter()
        .filter(|d| d.armor[3] == SKULL && has_layer(d) && d.class != Class::Object(78))
        .map(|d| WornHead {
            pos: d.pos,
            body_yaw: d.yaw,
            head_yaw: (d.head_yaw - d.yaw + 180.0).rem_euclid(360.0) - 180.0,
            pitch: d.pitch,
            sneaking: d.sneaking,
            child: d.child,
            // `RenderPlayer.preRenderCallback` scales the whole entity.
            scale: if matches!(d.class, Class::Player { .. }) {
                0.937_5
            } else {
                1.0
            },
            kind: d.look.stand.head_damage as u8,
            owner: d.look.head_owner,
        })
        .collect()
}
