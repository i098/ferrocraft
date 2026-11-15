//! Wither skull (object 66) inputs of the renderer.

use world::entities::{Entity, Kind};

/// `WitherSkullRenderer.render` interpolates the yaw and pitch itself; every other entity keeps
/// the raw values.
pub fn look(e: &Entity, partial: f32) -> (f32, f32) {
    if e.kind == Kind::Object(66) {
        e.lerp_look(partial)
    } else {
        (e.yaw, e.pitch)
    }
}

/// `WitherSkullEntity.isCharged`: metadata byte 10, the blue skull.
pub fn charged(e: &Entity) -> bool {
    e.kind == Kind::Object(66) && e.meta.stand.unwrap_or(0) != 0
}
