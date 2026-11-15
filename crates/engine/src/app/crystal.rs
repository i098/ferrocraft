//! `EnderCrystalEntity.renderTicks` for the frame.

use world::entities::Entity;

/// `renderTicks` starts at `random.nextInt(100000)` and counts one per client tick, so crystals
/// do not spin in step. The start is a hash of the entity id (a client-side look, not game state).
pub fn ticks(e: &Entity) -> i32 {
    let start = (e.id as u32).wrapping_mul(2_654_435_761) % 100_000;
    start as i32 + e.age as i32
}
