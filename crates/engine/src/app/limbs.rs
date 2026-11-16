//! Walk and body-yaw state of the local player.

/// `EntityPlayer.limbSwing` for the local player, so the third-person model walks.
#[derive(Default)]
pub(super) struct Limbs {
    pub(super) swing: f32,
    pub(super) amount: f32,
    pub(super) prev_amount: f32,
    /// `renderYawOffset` and its previous value.
    pub(super) body_yaw: f32,
    pub(super) prev_body_yaw: f32,
}

impl Limbs {
    /// `renderYawOffset` between the previous and current tick, the short way round.
    pub(super) fn body_yaw(&self, partial: f32) -> f32 {
        world::body::lerp_yaw(self.prev_body_yaw, self.body_yaw, partial)
    }
}
