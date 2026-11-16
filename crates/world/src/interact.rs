//! The client half of `PlayerEntity.interact`: whether a right click on an entity is consumed, so
//! `Minecraft.doUse` skips the use-item step. Only the state the 1.8.9 client really has counts;
//! the server side (trade GUI, riding, horse inventory) is not run here.
//!
//! Ported from `VillagerEntity.interactMob`, `BoatEntity.interact`, `RideableMinecartEntity.interact`,
//! `InventoryMinecartEntity.interact`, `HopperMinecartEntity.interact`, `FurnaceMinecartEntity.interact`
//! and `HorseBaseEntity.interactMob`.

use crate::entities::{is_baby, Entities, Entity, Kind};
use crate::horse;

/// Who sits on the clicked entity (`Entity.rider`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rider {
    None,
    /// The local player.
    Me,
    /// Another player.
    Player,
    /// Any other entity.
    Other,
}

const SPAWN_EGG: i16 = 383;
const SADDLE: i16 = 329;
const CHEST_BLOCK: i16 = 54;
/// Iron, golden and diamond horse armor.
const HORSE_ARMOR: [i16; 3] = [417, 418, 419];
/// Wheat, sugar, hay block, apple, golden carrot, golden apple: every one heals and ages a foal.
const HORSE_FOOD: [i16; 6] = [296, 353, 170, 260, 396, 322];
/// `MinecartEntity.Type` ids: chest, furnace, hopper. These carts return true; TNT, spawner and
/// command block carts return false.
const CART_CLICKED: [u8; 3] = [1, 2, 5];
/// The lowest `generic.maxHealth` of a horse. The client does not know the real value.
const HORSE_MIN_MAX_HEALTH: f32 = 15.0;

/// The rider of entity `id`: the local player when `my_vehicle` is `id`, else the first entity
/// that rides it.
pub fn rider_of(entities: &Entities, my_vehicle: Option<i32>, id: i32) -> Rider {
    if my_vehicle == Some(id) {
        return Rider::Me;
    }
    match entities.iter().find(|r| r.vehicle == Some(id)) {
        None => Rider::None,
        Some(r) if r.kind == Kind::Player => Rider::Player,
        Some(_) => Rider::Other,
    }
}

/// True when `PlayerEntity.interact` returns true on the client for a non-spectator holding
/// `held` (an item id) in the hand.
///
/// shortcut: a tame adult horse fed food counts as healed only below 15 health, because the max
/// health is not synced; upgrade when entity attributes are tracked.
pub fn client_interact(e: &Entity, held: Option<i16>, sneaking: bool, rider: Rider) -> bool {
    match e.kind {
        Kind::Mob(120) => villager(e, held),
        Kind::Mob(100) => horse_click(e, held, sneaking, rider),
        Kind::Object(1) => true,
        Kind::Object(10..=12) => cart(e, rider),
        _ => false,
    }
}

fn villager(e: &Entity, held: Option<i16>) -> bool {
    let alive = e.health.is_none_or(|h| h > 0.0);
    // Not an egg, alive, no customer on the client, adult: trade. An egg goes to `PassiveEntity`, true.
    held == Some(SPAWN_EGG) || (alive && !is_baby(e.kind, &e.meta))
}

fn cart(e: &Entity, rider: Rider) -> bool {
    match e.meta.vehicle.cart.unwrap_or(0) {
        0 => rider != Rider::Other,
        t => CART_CLICKED.contains(&t),
    }
}

fn horse_click(e: &Entity, held: Option<i16>, sneaking: bool, rider: Rider) -> bool {
    if held == Some(SPAWN_EGG) {
        return true;
    }
    let kind = e.meta.horse.unwrap_or(0);
    let flags = e.meta.horse_flags.unwrap_or(0);
    let (tame, baby) = (horse::is_tame(flags), is_baby(e.kind, &e.meta));
    let undead = kind == 3 || kind == 4;
    if !tame && undead {
        return false;
    }
    if tame && !baby && sneaking {
        return true;
    }
    let adult = !baby;
    if adult && rider != Rider::None {
        return false;
    }
    if held.is_some_and(|item| horse_item(e, item, tame, baby)) {
        return true;
    }
    adult
}

/// A held item that makes the horse take it: armor, a tame-up try, food, a chest or a saddle.
fn horse_item(e: &Entity, item: i16, tame: bool, baby: bool) -> bool {
    let kind = e.meta.horse.unwrap_or(0);
    let flags = e.meta.horse_flags.unwrap_or(0);
    let undead = kind == 3 || kind == 4;
    if kind == 0 && HORSE_ARMOR.contains(&item) || !tame {
        return true;
    }
    let hurt = e.health.is_some_and(|h| h < HORSE_MIN_MAX_HEALTH);
    let fed = !undead && HORSE_FOOD.contains(&item) && (hurt || baby);
    let chest = matches!(kind, 1 | 2) && !horse::is_chested(flags) && item == CHEST_BLOCK;
    let saddle = !baby && !horse::is_saddled(flags) && item == SADDLE;
    fed || chest || saddle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::EntityEvent;

    fn spawn_mob(es: &mut Entities, id: i32, kind: u8) {
        es.apply(EntityEvent::SpawnMob {
            id,
            kind,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            head_yaw: 0,
            vel: [0; 3],
            flags: None,
            name: None,
            health: None,
            size: Default::default(),
        });
    }

    #[test]
    fn a_villager_is_clicked_unless_it_is_a_child_or_dead() {
        let mut es = Entities::default();
        spawn_mob(&mut es, 1, 120);
        let v = es.get(1).unwrap().clone();
        assert!(client_interact(&v, Some(267), false, Rider::None), "sword");
        assert!(client_interact(&v, None, false, Rider::None), "empty hand");
        let mut dead = v.clone();
        dead.health = Some(0.0);
        assert!(!client_interact(&dead, Some(267), false, Rider::None));
        assert!(client_interact(&dead, Some(SPAWN_EGG), false, Rider::None));
    }

    #[test]
    fn carts_follow_their_type_and_rider() {
        let mut es = Entities::default();
        spawn_mob(&mut es, 1, 120);
        let mut c = es.get(1).unwrap().clone();
        c.kind = Kind::Object(10);
        for (ty, other, want) in [
            (0, Rider::None, true),
            (0, Rider::Player, true),
            (0, Rider::Other, false),
            (1, Rider::None, true),
            (2, Rider::None, true),
            (3, Rider::None, false),
            (4, Rider::None, false),
            (5, Rider::None, true),
            (6, Rider::None, false),
        ] {
            c.meta.vehicle.cart = Some(ty);
            assert_eq!(client_interact(&c, None, false, other), want, "{ty}");
        }
        c.kind = Kind::Object(1);
        assert!(client_interact(&c, Some(267), false, Rider::Other));
    }

    #[test]
    fn horses_follow_tame_age_rider_and_item() {
        let mut es = Entities::default();
        spawn_mob(&mut es, 1, 100);
        let mut h = es.get(1).unwrap().clone();
        assert!(client_interact(&h, None, false, Rider::None), "mount");
        assert!(!client_interact(&h, None, false, Rider::Me), "ridden");
        assert!(
            client_interact(&h, Some(267), false, Rider::None),
            "wild horse, sword"
        );
        h.meta.horse_flags = Some(2);
        assert!(
            client_interact(&h, None, true, Rider::Me),
            "sneak opens the inventory"
        );
        assert!(client_interact(&h, Some(329), false, Rider::None), "saddle");
        h.meta.horse_flags = Some(2 | 4);
        h.health = Some(30.0);
        assert!(
            client_interact(&h, Some(267), false, Rider::None),
            "mount with a sword"
        );
        assert!(
            !client_interact(&h, Some(267), false, Rider::Other),
            "ridden"
        );
        h.meta.horse = Some(3);
        h.meta.horse_flags = Some(0);
        assert!(
            !client_interact(&h, None, false, Rider::None),
            "wild zombie horse"
        );
    }
}
