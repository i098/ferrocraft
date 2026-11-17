//! Sounds the 1.8.9 client plays for its own player and that no packet carries: water contact,
//! swimming, landing, and eating or drinking. The server plays the same sounds for other players
//! through Sound Effect (0x29), which `events.rs` already handles.

use super::crumbs::crumb_item;
use super::play::FOOD;
use super::{eye_height, App};
use world::motion_sound::{self, Fall};

/// Player hitbox height (sneaking does not change the box in 1.8.9).
const HEIGHT: f64 = 1.8;
/// `PlayerCapabilities.allowFlying` in the abilities flags.
const CAN_FLY: u8 = 4;
/// Use time left at which a consume sound repeats: `itemUseTimer <= 25 && itemUseTimer % 4 == 0`.
const CONSUME_FROM: i32 = 25;

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
        if now {
            self.own_sounds.fall.reset();
        }
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

    /// `Entity.moveEntity`'s fall check after the move; a landing that hurts plays the fall
    /// sound and the step sound of the block below.
    pub(super) fn fall_sounds(&mut self) {
        let Some(p) = &self.game.player else { return };
        if self.game.vehicle.is_some() {
            return;
        }
        let Some(dist) = self
            .own_sounds
            .fall
            .step(p.pos[1] - self.game.prev_pos[1], p.on_ground)
        else {
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
        let Some(name) = motion_sound::landing(dist, block, self.input.sneaking()) else {
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
                    self.play_consume(c, at, 5);
                }
            }
        }
        if std::mem::take(&mut self.game.finished_use) {
            self.end_use();
            if let Some(c) = self.own_sounds.consuming.take() {
                self.play_consume(c, at, 16);
            }
        }
    }

    /// `EntityPlayer.onConsumeItem(stack, count)`: drink at half volume; eat `count` crumbs and a sound at 0.5 or 1.0.
    /// `finishUsingItem` passes 16, the use tick 5.
    fn play_consume(&mut self, c: Consume, at: [f64; 3], count: u32) {
        match c {
            Consume::Drink => {
                let pitch = self.roll() * 0.1 + 0.9;
                self.play_own("random.drink", at, 0.5, pitch);
            }
            Consume::Eat(item) => {
                if let Some(p) = &self.game.player {
                    let eye = eye_height(self.input.sneaking()) as f32;
                    self.particles
                        .eat_crumbs(count, p.pos, eye, (p.yaw, self.game.pitch), item);
                }
                let volume = if self.roll() < 0.5 { 0.5 } else { 1.0 };
                let pitch = self.pitch_jitter(0.2);
                self.play_own("random.eat", at, volume, pitch);
            }
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
