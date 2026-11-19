//! Sounds the 1.8.9 client plays for its own player and that no packet carries: water contact,
//! swimming, landing, and eating or drinking. The server plays the same sounds for other players
//! through Sound Effect (0x29), which `events.rs` already handles.

use super::crumbs::crumb_item;
use super::play::FOOD;
use super::{eye_height, App};
use world::chunk::ChunkStore;
use world::motion_sound::{self, Fall};
use world::World;

/// Player hitbox height (sneaking does not change the box in 1.8.9).
const HEIGHT: f64 = 1.8;
/// `PlayerCapabilities.allowFlying` in the abilities flags.
const CAN_FLY: u8 = 4;
/// Use time left at which a consume sound repeats: `itemUseTimer <= 25 && itemUseTimer % 4 == 0`.
const CONSUME_FROM: i32 = 25;

/// The loaded chunks as the block source of the physics queries.
struct Blocks<'a>(&'a ChunkStore);

impl World for Blocks<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        self.0.get_block(x, y, z)
    }
}

#[derive(Clone, Copy)]
enum Consume {
    /// Carries the `ITEM_CRACK` data of the food.
    Eat(i32),
    Drink,
}

impl Consume {
    /// `Item.getItemUseAction` of the held item: milk and potions drink, food eats.
    fn of(item: i16, damage: i16) -> Option<Self> {
        match item {
            335 | 373 => Some(Self::Drink),
            id if FOOD.contains(&id) => Some(Self::Eat(crumb_item(id, damage))),
            _ => None,
        }
    }
}

/// A sound the client plays: event, volume, pitch.
type Cue = (&'static str, f32, f32);

/// The sounds of `EntityPlayer.onConsumeItem` (drink at half volume; eat at 0.5 or 1.0 with a jittered pitch)
/// and, when `finish` is set for a food, `ItemFood.onItemUseFinish` (`random.burp` at 0.5, pitch `0.9..1.0`).
/// `roll` is `Random.nextFloat`; the draws run in vanilla order.
fn consume_cues(c: Consume, finish: bool, roll: &mut impl FnMut() -> f32) -> Vec<Cue> {
    match c {
        Consume::Drink => vec![("random.drink", 0.5, roll() * 0.1 + 0.9)],
        Consume::Eat(_) => {
            let volume = if roll() < 0.5 { 0.5 } else { 1.0 };
            let pitch = (roll() - roll()) * 0.2 + 1.0;
            let mut cues = vec![("random.eat", volume, pitch)];
            if finish {
                cues.push(("random.burp", 0.5, roll() * 0.1 + 0.9));
            }
            cues
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    /// `Entity.inWater`; `None` before the first tick, which never splashes (`firstTick`).
    water: Option<bool>,
    fall: Fall,
    /// What the player was consuming, kept until the finish status (9) arrives.
    consuming: Option<Consume>,
}

impl State {
    pub(super) fn fall_distance(&self) -> f32 {
        self.fall.distance()
    }

    pub(super) fn swimming(&self) -> bool {
        self.water == Some(true)
    }
}

impl App {
    /// `Entity.onEntityUpdate`: `checkWaterCollisions` runs before the move, with the velocity
    /// the last tick left. Entering water splashes; water also resets the fall distance.
    pub(super) fn water_sounds(&mut self) {
        let Some(p) = &self.game.player else { return };
        let store = &self.game.store;
        let now = motion_sound::in_water(p.pos, HEIGHT, |x, y, z| store.get_block(x, y, z));
        let splash = now && self.own_sounds.water == Some(false) && self.game.gamemode != 3;
        let (at, vel) = (p.pos, p.vel);
        self.own_sounds.water = Some(now);
        self.terrain_fall(now);
        if splash {
            let pitch = self.pitch_jitter(0.4);
            self.play_own(
                "game.player.swim.splash",
                at,
                motion_sound::splash_volume(vel),
                pitch,
            );
        }
    }

    /// Fall-distance rules of the terrain around the player: water and `Entity.onEntityUpdate`'s lava halving,
    /// and `moveEntityWithHeading`'s reset on a ladder or vine.
    fn terrain_fall(&mut self, in_water: bool) {
        let Some(p) = &self.game.player else { return };
        let store = &self.game.store;
        let fall = &mut self.own_sounds.fall;
        if in_water {
            fall.reset();
        }
        if p.in_lava(&Blocks(store)) {
            fall.halve();
        }
        let feet = store.get_block(
            p.pos[0].floor() as i32,
            p.pos[1].floor() as i32,
            p.pos[2].floor() as i32,
        );
        if motion_sound::climbing(feet, self.game.gamemode == 3) {
            fall.reset();
        }
    }

    /// `Entity.moveEntity`'s fall check after the move; a landing that hurts plays the fall
    /// sound and the step sound of the block below.
    pub(super) fn fall_sounds(&mut self) {
        let Some(p) = &self.game.player else { return };
        if self.game.vehicle.is_some() {
            self.own_sounds.fall.reset();
            return;
        }
        let dist = self
            .own_sounds
            .fall
            .step(p.pos[1] - self.game.prev_pos[1], p.on_ground);
        // `Entity.setInWeb` runs in `doBlockCollisions`, after the fall check of the move.
        if p.in_web() {
            self.own_sounds.fall.reset();
        }
        let Some(dist) = dist else {
            return;
        };
        if self.game.abilities & CAN_FLY != 0 {
            return;
        }
        let at = p.pos;
        let block = self.game.store.get_block(
            at[0].floor() as i32,
            (at[1] - 0.2).floor() as i32,
            at[2].floor() as i32,
        );
        let Some(name) = motion_sound::landing(dist, block, self.input.sneaking(), p.jump_boost)
        else {
            return;
        };
        self.play_own(name, at, 1.0, 1.0);
        if block >> 4 != 0 {
            if let Some(s) = &mut self.sound {
                s.land(block >> 4, at);
            }
        }
    }

    /// The swim sound that goes with a footstep in water; `d` is the move just made.
    pub(super) fn swim_sound(&mut self, d: [f64; 3], at: [f64; 3]) {
        if self.own_sounds.swimming() {
            let pitch = self.pitch_jitter(0.4);
            self.play_own("game.player.swim", at, motion_sound::swim_volume(d), pitch);
        }
    }

    /// `EntityPlayer.onUpdate` while an item is in use, and `finishUsingItem` on status 9.
    /// Call before `use_ticks` counts down.
    pub(super) fn consume_sounds(&mut self) {
        let Some(at) = self.game.player.as_ref().map(|p| p.pos) else {
            return;
        };
        if self.in_use {
            let held = self
                .game
                .inv
                .held_stack()
                .as_ref()
                .map(|s| (s.id, s.damage));
            if let Some(c) = held.and_then(|(id, damage)| Consume::of(id, damage)) {
                self.own_sounds.consuming = Some(c);
                if self.use_ticks <= CONSUME_FROM && self.use_ticks % 4 == 0 {
                    self.play_consume(c, at, 5, false);
                }
            }
        }
        if std::mem::take(&mut self.game.finished_use) {
            self.end_use();
            if let Some(c) = self.own_sounds.consuming.take() {
                self.play_consume(c, at, 16, true);
            }
        }
    }

    /// `EntityPlayer.onConsumeItem(stack, count)`: eat `count` crumbs and play the cues. `finishUsingItem`
    /// passes 16 and `finish`, the use tick 5 and none.
    fn play_consume(&mut self, c: Consume, at: [f64; 3], count: u32, finish: bool) {
        if let Consume::Eat(item) = c {
            if let Some(p) = &self.game.player {
                let eye = eye_height(self.input.sneaking()) as f32;
                self.particles
                    .eat_crumbs(count, p.pos, eye, (p.yaw, self.game.pitch), item);
            }
        }
        let cues = consume_cues(c, finish, &mut || self.roll());
        for (event, volume, pitch) in cues {
            self.play_own(event, at, volume, pitch);
        }
    }

    /// `(nextFloat - nextFloat) * spread + 1`.
    fn pitch_jitter(&mut self, spread: f32) -> f32 {
        (self.roll() - self.roll()) * spread + 1.0
    }

    fn roll(&mut self) -> f32 {
        self.sound.as_mut().map_or(0.0, |s| s.roll())
    }

    /// `EntityPlayerSP.onLivingUpdate`: `portal.trigger` at 0.25 volume, without attenuation.
    pub(super) fn play_portal_trigger(&mut self) {
        let pitch = self.roll() * 0.4 + 0.8;
        if let Some(s) = &mut self.sound {
            s.play("portal.trigger", None, 0.25, pitch);
        }
    }

    fn play_own(&mut self, event: &str, at: [f64; 3], volume: f32, pitch: f32) {
        if let Some(s) = &mut self.sound {
            s.play(event, Some(at), volume, pitch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolls(v: &[f32]) -> impl FnMut() -> f32 + '_ {
        let mut i = 0;
        move || {
            i += 1;
            v[i - 1]
        }
    }

    /// `EntityPlayer.updateItemUse(stack, 5)` on the use tick: one `random.eat`, no burp.
    #[test]
    fn a_use_tick_plays_only_the_eat_sound() {
        let c = consume_cues(Consume::Eat(322), false, &mut rolls(&[0.7, 0.9, 0.4]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "random.eat");
        assert_eq!(c[0].1, 1.0, "0.5 + 0.5 * nextInt(2)");
        assert!((c[0].2 - 1.1).abs() < 1e-6, "(0.9 - 0.4) * 0.2 + 1");
        let low = consume_cues(Consume::Eat(322), false, &mut rolls(&[0.2, 0.5, 0.5]));
        assert_eq!(low[0].1, 0.5);
    }

    /// `finishUsingItem`: `updateItemUse(stack, 16)` plays the eat sound, then `ItemFood` plays `random.burp`.
    #[test]
    fn finishing_a_food_burps_after_the_last_bite() {
        let c = consume_cues(Consume::Eat(322), true, &mut rolls(&[0.7, 0.5, 0.5, 0.5]));
        let names: Vec<_> = c.iter().map(|x| x.0).collect();
        assert_eq!(names, ["random.eat", "random.burp"]);
        assert_eq!(c[1].1, 0.5);
        assert!((c[1].2 - 0.95).abs() < 1e-6, "nextFloat * 0.1 + 0.9");
    }

    /// Milk and potions drink at half volume and do not burp (`ItemBucketMilk`, `ItemPotion`).
    #[test]
    fn a_drink_never_burps() {
        let c = consume_cues(Consume::Drink, true, &mut rolls(&[0.5]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "random.drink");
        assert_eq!(c[0].1, 0.5);
    }
}
