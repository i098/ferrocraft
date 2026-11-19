//! `ItemInHandRenderer`: the hand dips when the held stack changes or is used, and rises again.

use super::{App, HandItem};
use render::JavaRandom;
use world::entities::Kind;
use world::inventory::ItemStack;

/// `Enchantment.UNBREAKING.id`.
const UNBREAKING: i32 = 34;

fn hand_item(s: &ItemStack) -> HandItem {
    let dye = s.nbt.as_ref().and_then(|n| n.dye());
    (s.id, s.damage, world::inventory::glint(s), dye)
}

/// `SwordItem.attack` wears 1, `ToolItem.attack` (pickaxe, axe, shovel) wears 2; other items wear 0.
fn hit_wear(id: i16) -> i32 {
    match id {
        267 | 268 | 272 | 276 | 283 => 1,
        256..=258 | 269..=271 | 273..=275 | 277..=279 | 284..=286 => 2,
        _ => 0,
    }
}

/// `ItemStack.takeDamageAndBreak` for a player: Unbreaking skips each point on `nextInt(level + 1) > 0`,
/// the item breaks past its maximum damage. Returns the stack in its new state.
fn worn(
    mut s: ItemStack,
    amount: i32,
    unbreaking: i32,
    roll: &mut impl FnMut(i32) -> i32,
) -> ItemStack {
    let max = i32::from(world::inventory::items::item(s.id).max_damage);
    let unbreakable = s.nbt.as_ref().is_some_and(|n| {
        n.root.iter().any(|(k, v)| {
            k == "Unbreakable" && matches!(v, world::inventory::Tag::Byte(b) if *b != 0)
        })
    });
    if max <= 0 || unbreakable {
        return s;
    }
    let skipped = (0..amount)
        .take_while(|_| unbreaking > 0)
        .filter(|_| roll(unbreaking + 1) > 0)
        .count();
    let amount = amount - skipped as i32;
    if amount <= 0 {
        return s;
    }
    s.damage += amount as i16;
    if i32::from(s.damage) > max {
        s.count = (s.count - 1).max(0);
        s.damage = 0;
    }
    s
}

impl App {
    /// `ItemInHandRenderer.tick`: the hand lowers while the held stack differs from the shown one
    /// (`ItemStack.matches`: size, item, damage and NBT) and rises again; the shown stack swaps near the bottom.
    pub(super) fn equip_tick(&mut self) {
        let held = self.game.inv.held_stack().clone();
        let (cur, _, shown) = self.equip;
        let same = self.hand_stack == held;
        let next = cur + ((if same { 1.0 } else { 0.0 }) - cur).clamp(-0.4, 0.4);
        let item = held.as_ref().map(hand_item);
        let shown = if same || next < 0.1 { item } else { shown };
        if next < 0.1 {
            self.hand_stack = held;
        }
        self.equip = (next, cur, shown);
    }

    /// `ItemInHandRenderer.onBlockUsed` and `onItemUsed`: the hand starts from the bottom.
    pub(super) fn reset_equip(&mut self) {
        self.equip.0 = 0.0;
    }

    /// The shown stack is the same object as the held one in vanilla, so a change in place of the
    /// held stack (`before`) carries over to it; a count of 0 stays shown until the hand has dipped.
    fn follow_in_place(&mut self, before: &ItemStack, after: ItemStack) {
        if self.hand_stack.as_ref() == Some(before) {
            self.hand_stack = Some(after);
        }
    }

    /// `Minecraft.rightClickMouse` after `onPlayerRightClick`: a stack that changed size (or any use in
    /// creative) resets the hand; a stack used up is removed from the slot.
    pub(super) fn after_block_use(&mut self, before: &ItemStack) {
        let now = self.game.inv.held_stack().as_ref().map_or(0, |s| s.count);
        self.follow_in_place(
            before,
            ItemStack {
                count: now,
                ..before.clone()
            },
        );
        if now != 0 && (now != before.count || self.game.creative) {
            self.reset_equip();
        }
    }

    /// `PlayerControllerMP.sendUseItem`: a pearl, snowball, egg, experience bottle or splash potion thrown
    /// outside creative uses one up in place, which resets the hand.
    pub(super) fn after_item_use(&mut self, before: &ItemStack) {
        let thrown = matches!(before.id, 332 | 344 | 368 | 384)
            || (before.id == 373 && before.damage & 16384 != 0);
        if !thrown || self.game.creative {
            return;
        }
        let slot = 36 + usize::from(self.game.inv.held);
        let after = ItemStack {
            count: before.count - 1,
            ..before.clone()
        };
        self.game.inv.player[slot] = (after.count > 0).then(|| after.clone());
        self.follow_in_place(before, after);
        self.reset_equip();
    }

    /// `RemoteClientPlayerEntity.takeDamage` is true, so a hit on another player wears the held weapon on
    /// the client (`PlayerEntity.attack`); a hit on a mob is not predicted.
    pub(super) fn wear_after_hit(&mut self, target: i32) {
        let on_player = self
            .game
            .entities
            .get(target)
            .is_some_and(|e| e.kind == Kind::Player);
        let Some(before) = self.game.inv.held_stack().clone() else {
            return;
        };
        let amount = hit_wear(before.id);
        if !on_player || amount == 0 || self.game.creative || self.game.gamemode == 3 {
            return;
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_nanos() as i64);
        let mut rng = JavaRandom::new(seed);
        let level = world::dig::highest_enchant_level([self.game.inv.held_stack()], UNBREAKING);
        let after = worn(before.clone(), amount, level, &mut |n| rng.next_int(n));
        let slot = 36 + usize::from(self.game.inv.held);
        self.game.inv.player[slot] = (after.count > 0).then(|| after.clone());
        self.follow_in_place(&before, after);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::chunk::Chunk;
    use world::entities::EntityEvent;

    fn app() -> App {
        let mut app = App::new(crate::args::Args {
            username: "Player".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.game.player = Some(world::Player::new([0.5, 64.0, 0.5], 0.0));
        app
    }

    fn hold(app: &mut App, id: i16, count: i8, damage: i16) {
        app.game.inv.player[36] = Some(ItemStack {
            id,
            count,
            damage,
            nbt: None,
        });
        app.equip_tick();
        for _ in 0..10 {
            app.equip_tick();
        }
        assert_eq!(app.equip.0, 1.0);
    }

    /// `Minecraft.rightClickMouse`: a block placed from a stack of 8 sends the hand to the bottom.
    #[test]
    fn placing_a_block_lowers_the_hand() {
        let mut app = app();
        let mut c = Chunk::new(0, 0);
        c.set_block(0, 65, 2, 1 << 4);
        app.game.store.insert(c);
        hold(&mut app, 4, 8, 0);
        app.use_item();
        assert_eq!(app.game.inv.held_stack().as_ref().map(|s| s.count), Some(7));
        assert_eq!(app.equip.0, 0.0);
        app.equip_tick();
        assert!(app.equip.0 > 0.0, "the hand rises again");
    }

    /// `ItemEnderPearl.onItemRightClick` uses one up on the client, so `sendUseItem` returns true.
    #[test]
    fn throwing_a_pearl_lowers_the_hand() {
        let mut app = app();
        hold(&mut app, 368, 8, 0);
        app.use_item();
        assert_eq!(app.game.inv.held_stack().as_ref().map(|s| s.count), Some(7));
        assert_eq!(app.equip.0, 0.0);
        app.equip_tick();
        assert!(app.equip.1 == 0.0 && app.equip.0 > 0.0);
    }

    #[test]
    fn a_hit_on_a_player_keeps_the_hand_up_but_a_server_damage_change_lowers_it() {
        let mut app = app();
        app.game.entities.apply(EntityEvent::SpawnPlayer {
            id: 5,
            uuid: 5,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            held: 0,
            flags: None,
        });
        hold(&mut app, 267, 1, 0);
        app.wear_after_hit(5);
        assert_eq!(
            app.game.inv.held_stack().as_ref().map(|s| s.damage),
            Some(1)
        );
        app.equip_tick();
        assert_eq!(app.equip.0, 1.0);
        // S2F from the server alone: a new stack with more damage.
        app.game.inv.player[36].as_mut().unwrap().damage = 2;
        app.equip_tick();
        assert!(app.equip.0 < 1.0);
    }

    #[test]
    fn wear_follows_unbreaking_and_breaks_past_the_maximum() {
        let sword = |damage| ItemStack {
            id: 268,
            count: 1,
            damage,
            nbt: None,
        };
        assert_eq!(worn(sword(0), 1, 0, &mut |_| 0).damage, 1);
        // Unbreaking III: a roll above 0 skips the point.
        assert_eq!(worn(sword(0), 1, 3, &mut |_| 1).damage, 0);
        assert_eq!(worn(sword(0), 1, 3, &mut |_| 0).damage, 1);
        let broken = worn(sword(59), 1, 0, &mut |_| 0);
        assert_eq!((broken.count, broken.damage), (0, 0));
        assert_eq!([hit_wear(290), hit_wear(258), hit_wear(276)], [0, 2, 1]);
    }
}
