//! Food crumbs of other players: `PlayerEntity.tick` calls `onConsumeItem(stack, 5)` for every
//! remote player whose use flag is set (`RemoteClientPlayerEntity.tick` starts the use from it).

use super::play::FOOD;
use super::App;
use world::entities::Kind;

/// The `ITEM_CRACK` data of a food: `id`, plus the damage for items with subtypes (golden apple, fish).
pub(super) fn crumb_item(id: i16, damage: i16) -> i32 {
    let id = i32::from(id);
    if matches!(id, 322 | 349 | 350) {
        id | i32::from(damage) << 16
    } else {
        id
    }
}

impl App {
    /// The 5 crumbs of every remote player that eats this tick, from where it stood before the tick moved it.
    pub(super) fn remote_eat_crumbs(&mut self) {
        for e in self.game.entities.iter() {
            let (Some(n), Some(held)) = (e.use_ticks, e.equipment[0]) else {
                continue;
            };
            if e.kind == Kind::Player && FOOD.contains(&held.id) && render::remote_crumbs_due(n) {
                let item = crumb_item(held.id, held.damage);
                let look = (e.prev_yaw, e.prev_pitch);
                self.particles
                    .eat_crumbs(5, e.prev_pos, e.eye_height(), look, item);
            }
        }
    }
}
