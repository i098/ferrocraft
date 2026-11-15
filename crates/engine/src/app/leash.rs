//! The holder of a leashed mob's rope (`RenderLiving.renderLeash`).

use super::{eye_height, App};
use render::leash::Holder;
use world::entities::{Entity, Kind};

impl App {
    /// `getLeashedToEntity` as the renderer reads it: the holder's interpolated position, eye height
    /// and look. `None` when the mob is not leashed or the holder is not in the world.
    pub(super) fn leash_holder(&self, e: &Entity, frac: f64) -> Option<Holder> {
        let id = e.leash?;
        let p = frac as f32;
        if id == self.game.entity_id {
            let me = self.game.player.as_ref()?;
            let lerp = |a: f64, b: f64| a + (b - a) * frac;
            let pos = std::array::from_fn(|i| lerp(self.game.prev_pos[i], me.pos[i]));
            let look = [me.yaw, self.game.pitch];
            // shortcut: the local player keeps no previous look, so the half-tick blend is skipped.
            let eye = eye_height(self.input.sneaking()) as f32;
            return Some(Holder::new(pos, eye, look, look, p, false));
        }
        let h = self.game.entities.get(id)?;
        let knot = h.kind == Kind::Object(77);
        // `EntityLeashKnot` stands at the centre of its block.
        let pos = if knot {
            h.pos.map(|c| c.floor() + 0.5)
        } else {
            h.lerp_pos(frac)
        };
        let (prev, cur) = ([h.prev_yaw, h.prev_pitch], [h.yaw, h.pitch]);
        Some(Holder::new(pos, h.eye_height(), prev, cur, p, knot))
    }
}
