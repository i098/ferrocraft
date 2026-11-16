//! Wither skull (object 66) inputs of the renderer.

use world::entities::{Entity, Kind};

/// `WitherSkullEntity.isCharged`: metadata byte 10, the blue skull.
pub fn charged(e: &Entity) -> bool {
    e.kind == Kind::Object(66) && e.meta.stand.unwrap_or(0) != 0
}
