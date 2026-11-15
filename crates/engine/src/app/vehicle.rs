//! Boats and minecarts in the frame: the metadata the renderers read, and the rail snap of
//! `MinecartRenderer.render`.

use render::{vehicle::VehicleLook, EntityDraw};
use world::chunk::ChunkStore;
use world::entities::{Entity, Kind};
use world::vehicle::{hit_wobble, on_rail, CartPose};
use world::World;

struct Blocks<'a>(&'a ChunkStore);

impl World for Blocks<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        self.0.get_block(x, y, z)
    }
}

/// `getDamagedTimer`, `getDamagedSwingDirection`, `getDamage` and the display block of `e`; a boat
/// sends no display block, and its direction starts at 1 as `BoatEntity` sets it.
pub fn look(e: &Entity, partial: f32) -> VehicleLook {
    let m = &e.meta.vehicle;
    let (block, block_offset) = m.display_block();
    VehicleLook {
        wobble: hit_wobble(
            m.hit_timer.unwrap_or(0),
            m.damage.unwrap_or(0.0),
            m.hit_direction.unwrap_or(1),
            partial,
        ),
        block,
        block_offset,
    }
}

/// Sits a cart on its rail with the slope pitch and the heading of the track.
pub fn snap(store: &ChunkStore, e: &Entity, d: &mut EntityDraw) {
    if !matches!(e.kind, Kind::Object(10..=12)) {
        return;
    }
    let pose = on_rail(
        &Blocks(store),
        CartPose {
            pos: d.pos,
            yaw: d.yaw,
            pitch: d.pitch,
        },
    );
    (d.pos, d.yaw, d.pitch) = (pose.pos, pose.yaw, pose.pitch);
}
