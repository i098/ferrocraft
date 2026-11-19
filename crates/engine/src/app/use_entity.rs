//! `PlayerControllerMP.interactWithEntity*`: the right click on the entity under the crosshair.

use super::App;
use world::entities::Kind;
use world::inventory::ItemStack;

impl App {
    /// `PlayerControllerMP.interactWithEntity*` on the entity under the crosshair. True when the
    /// click is done: the entity consumed it (`Minecraft.doUse` sends no use-item packet) or is gone.
    pub(super) fn use_entity(
        &mut self,
        id: i32,
        dist: f64,
        eye: [f64; 3],
        dir: [f64; 3],
        sneaking: bool,
        held: &Option<ItemStack>,
    ) -> bool {
        let gamemode = self.game.gamemode;
        let Some(e) = self.game.entities.get(id) else {
            return true;
        };
        let at = [0, 1, 2].map(|a| (eye[a] + dir[a] * dist - e.pos[a]) as f32);
        let handled = gamemode != 3 && e.kind == Kind::Object(78);
        let rider = world::interact::rider_of(&self.game.entities, self.game.vehicle, id);
        let held_id = held.as_ref().map(|s| s.id);
        let consumed = gamemode != 3
            && (world::interact::client_interact(e, held_id, sneaking, rider)
                || world::interact::client_item_interact(e, held.as_ref(), self.game.entity_id));
        let r = self.session_send(|s| s.interact_entity_at(id, at));
        self.send_result(r);
        if handled {
            return true;
        }
        let r = self.session_send(|s| s.interact_entity(id));
        self.send_result(r);
        consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::entities::EntityEvent;

    /// `Minecraft.doUse`: a click on an adult villager is consumed, so a sword starts no block.
    #[test]
    fn a_click_on_a_villager_starts_no_item_use() {
        let click = |kind: u8| {
            let mut app = App::new(crate::args::Args {
                username: "Player".into(),
                auth: None,
                assets_dir: None,
                server: None,
                game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
                ..Default::default()
            });
            app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
            app.game.inv.player[36] = Some(world::inventory::ItemStack {
                id: 267,
                count: 1,
                damage: 0,
                nbt: None,
            });
            // Yaw 0 faces +z: the mob stands two blocks ahead of the eye.
            app.game.entities.apply(EntityEvent::SpawnMob {
                id: 9,
                kind,
                pos: [16, 2048, 96],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size: Default::default(),
            });
            app.use_item();
            app.use_ticks
        };
        assert_eq!(click(120), 0, "villager");
        assert_eq!(click(54), 72_000, "zombie: the sword blocks");
    }
}
