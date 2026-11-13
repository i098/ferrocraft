//! Window-free game state: chunks, the local player and the 20 TPS step.

use crate::swing::Swing;
use crate::weather::Weather;
use ferrocraft_net_client::{formatted_text, styled, CombatEventKind, NetEvent, Styled};
use std::collections::HashMap;
use world::border::WorldBorder;
use world::chunk::ChunkStore;
use world::dig::{self, Action, DigWorld, Digger, Frame, Hit};
use world::entities::{Entities, EntityEvent};
use world::inventory::Inventory;
use world::{Block, Input, Player, World};

/// `EntityPlayer.getEyeHeight`: 1.62 above the feet, 0.08 lower while sneaking.
pub fn eye_height(sneaking: bool) -> f64 {
    if sneaking {
        1.62 - 0.08
    } else {
        1.62
    }
}

/// `damagedTime`: length of the hit animation in ticks.
const HURT_TICKS: u8 = 10;
const CREATIVE: u8 = 1;
const SPECTATOR: u8 = 3;

/// Left-button state for one client tick, as `world::dig` wants it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attack {
    pub held: bool,
    pub presses: u32,
}

/// Changes that the renderer or the HUD must mirror.
#[derive(Debug, PartialEq)]
pub enum Effect {
    Upload(i32, i32),
    Remove(i32, i32),
    Clear,
    Chat(Vec<Styled>),
    ActionBar(String),
    OpenWindow,
    CloseWindow,
    /// The server opened a window type this client lacks: answer with `CloseWindow(id)`.
    RejectWindow(u8),
    /// A server reply that must be answered with `confirm_transaction`.
    Confirm {
        window: u8,
        action: i16,
    },
    /// Health fell to zero or below: the death screen.
    Died,
}

/// A 1.8 block state id as a physics block: what the renderer draws as a cube is solid, so no
/// collision is invisible. Water and lava are not solid; slabs and stairs are not modelled yet.
fn physics_block(state: u16) -> Block {
    match state >> 4 {
        8..=11 => Block::Air,
        79 | 174 | 212 => Block::Ice,
        _ if render::is_solid(state) => Block::Stone,
        _ => Block::Air,
    }
}

/// `World.getCollidingBoundingBoxes`: the chunks, and for a player inside the border every block outside it
/// is stone, which is what stops the walk at the wall. `walls` is `WorldBorder::bounds`.
struct Collision<'a> {
    store: &'a ChunkStore,
    walls: Option<[f64; 4]>,
}

impl World for Collision<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> Block {
        if self
            .walls
            .is_some_and(|w| !WorldBorder::contains_block(w, x, z))
        {
            return Block::Stone;
        }
        physics_block(self.store.get_block(x, y, z))
    }
}

/// `System.currentTimeMillis`, which the border's resize runs on.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Local block removals of the dig state machine, remembering which chunks changed.
struct Local<'a> {
    store: &'a mut ChunkStore,
    touched: &'a mut Vec<(i32, i32)>,
    /// Position and old state of every block removed, for break sounds and particles.
    broken: &'a mut Vec<([i32; 3], u16)>,
}

impl DigWorld for Local<'_> {
    fn state(&self, [x, y, z]: [i32; 3]) -> u16 {
        self.store.get_block(x, y, z)
    }

    fn remove(&mut self, [x, y, z]: [i32; 3]) {
        self.broken.push(([x, y, z], self.store.get_block(x, y, z)));
        self.store.set_block(x, y, z, 0);
        self.touched.push((x >> 4, z >> 4));
    }
}

pub struct Game {
    pub store: ChunkStore,
    pub entities: Entities,
    /// The local player's entity id, from `JoinGame`.
    pub entity_id: i32,
    pub entity_flags: u8,
    pub vehicle: Option<i32>,
    /// `World.getWorldBorder`, set by `S44`.
    pub border: WorldBorder,
    /// `Entity.isOutsideBorder` of the local player.
    border_outside: bool,
    /// `Minecraft.renderViewEntity` when it is not the local player: the spectated entity.
    /// Join and respawn set the player back without a shader change, as vanilla does.
    pub camera: Option<i32>,
    /// Creative mode (6 block entity reach instead of 3).
    pub creative: bool,
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub gamemode: u8,
    digger: Digger,
    /// `RenderGlobal.damagedBlocks` of other players: breaker id to block, stage `0..=9` and the tick of the last packet.
    cracks: HashMap<i32, ([i32; 3], u8, u64)>,
    pub player: Option<Player>,
    /// Player position before the latest tick, for partial-tick interpolation.
    pub prev_pos: [f64; 3],
    /// Set by S08 until the engine has echoed the new pose to the server.
    pub teleported: bool,
    pub pitch: f32,
    pub disconnected: Option<String>,
    pub weather: Weather,
    /// Lightning bolts of Spawn Global Entity and the flash they cause.
    pub bolts: render::Bolts,
    pub swing: Swing,
    /// Time of day from Time Update; the packet sends a negative value while the cycle is frozen.
    pub world_time: i64,
    /// `doDaylightCycle`, from the sign of the last Time Update.
    pub daylight_cycle: bool,
    /// `EntityPlayer.hasReducedDebug`: from `JoinGame` and entity status 22/23.
    pub reduced_debug: bool,
    /// Client ticks since the world loaded (`EntityRenderer.rendererUpdateCount`).
    pub ticks: u64,
    pub inv: Inventory,
    /// Blocks the player broke locally since the app last drained this.
    pub broken: Vec<([i32; 3], u16)>,
    pub xp_total: i32,
    /// `EntityPlayer.getScore` (metadata 18) for the death screen.
    pub score: i32,
    /// `S42` kind 2 message for the local player, formatted; shown by the death screen.
    pub death_message: Option<String>,
    /// Entity id of the killer from the same packet; -1 when none.
    pub killer_id: i32,
    pub hardcore: bool,
    pub difficulty: u8,
    pub health: f32,
    /// `EntityPlayerSP.hasValidHealth`: the first health packet after a join or respawn is no damage.
    health_seen: bool,
    /// `EntityLivingBase.hurtTime`: ticks left of the hit animation.
    pub hurt_timer: u8,
    /// `EntityLivingBase.deathTime`: ticks since health reached zero, capped at 20.
    pub death_ticks: u8,
    pub food: i32,
    pub saturation: f32,
    pub xp_bar: f32,
    pub xp_level: i32,
    /// `PlayerCapabilities`: bit 0 invulnerable, 1 flying, 2 allow flying, 3 creative.
    pub abilities: u8,
    /// `PlayerCapabilities.walkSpeed`.
    pub walk_speed: f32,
    /// `PlayerCapabilities.flySpeed`.
    pub fly_speed: f32,
    /// `EntityPlayer.flyToggleTimer`: ticks left to double-tap jump.
    fly_toggle_timer: u8,
    /// `MovementInput.jump` of the previous tick.
    jump_held: bool,
    /// `ItemStack.animationsToGo` of each hotbar slot (the pick-up pop).
    pub bob: [i32; 9],
    /// The held slot got another item stack or another index: vanilla's `EntityPlayer.onUpdate`
    /// then clears the item use. The app takes the flag each tick.
    pub held_replaced: bool,
    /// Status 9 for the own player (`finishUsingItem`); the app takes the flag each tick.
    pub finished_use: bool,
    /// `EntityLivingBase.activePotionsMap` of the local player.
    pub effects: world::effects::Effects,
    /// The local player's `generic.movementSpeed` attribute.
    pub speed: world::effects::Speed,
    /// Metadata index 7 and 8 of the local player: potion colour and ambient flag.
    pub potion_color: i32,
    pub potion_ambient: bool,
}

impl Game {
    pub fn new() -> Game {
        Game {
            store: ChunkStore::default(),
            entities: Entities::default(),
            entity_id: -1,
            entity_flags: 0,
            vehicle: None,
            border: WorldBorder::default(),
            border_outside: false,
            camera: None,
            creative: false,
            gamemode: 0,
            digger: Digger::default(),
            cracks: HashMap::new(),
            player: None,
            prev_pos: [0.0; 3],
            teleported: false,
            pitch: 0.0,
            disconnected: None,
            weather: Weather::default(),
            bolts: render::Bolts::default(),
            swing: Swing::default(),
            world_time: 6000,
            daylight_cycle: true,
            reduced_debug: false,
            ticks: 0,
            inv: Inventory::default(),
            broken: Vec::new(),
            xp_total: 0,
            score: 0,
            death_message: None,
            killer_id: -1,
            hardcore: false,
            difficulty: 1,
            health: 20.0,
            health_seen: false,
            hurt_timer: 0,
            death_ticks: 0,
            food: 20,
            saturation: 5.0,
            xp_bar: 0.0,
            xp_level: 0,
            abilities: 0,
            walk_speed: 0.1,
            fly_speed: 0.05,
            fly_toggle_timer: 0,
            jump_held: false,
            bob: [0; 9],
            held_replaced: false,
            finished_use: false,
            effects: world::effects::Effects::default(),
            speed: world::effects::Speed::default(),
            potion_color: 0,
            potion_ambient: false,
        }
    }

    /// Spawn a lightning bolt; the seed only shapes the bolt, so any entropy will do.
    fn strike(&mut self, pos: [f64; 3]) {
        let entropy = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        self.bolts.spawn(pos, entropy);
    }

    pub fn apply(&mut self, ev: NetEvent) -> Option<Effect> {
        let ev = match self.apply_player(ev) {
            Ok(e) => return e,
            Err(ev) => ev,
        };
        let ev = match self.apply_window(ev) {
            Ok(e) => return e,
            Err(ev) => ev,
        };
        self.apply_world(ev)
    }

    /// Score metadata and the S42 death message of the local player.
    fn apply_death_info(&mut self, ev: NetEvent) -> Option<Effect> {
        match ev {
            NetEvent::PlayerScore { entity_id, score } if entity_id == self.entity_id => {
                self.score = score;
            }
            NetEvent::Combat(CombatEventKind::EntityDead {
                player_id,
                entity_id,
                message,
            }) if player_id.0 == self.entity_id => {
                self.death_message = Some(formatted_text(&styled(&message)));
                self.killer_id = entity_id;
                return Some(Effect::Died);
            }
            _ => {}
        }
        None
    }

    /// Join, vitals, game mode and abilities. `Err` hands back an event it does not own.
    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn apply_player(&mut self, ev: NetEvent) -> Result<Option<Effect>, NetEvent> {
        match ev {
            NetEvent::JoinGame {
                entity_id,
                gamemode,
                hardcore,
                difficulty,
                reduced_debug_info,
                ..
            } => {
                self.entity_id = entity_id;
                self.health_seen = false;
                (self.death_message, self.killer_id, self.score) = (None, -1, 0);
                self.vehicle = None;
                self.border = WorldBorder::default();
                self.camera = None;
                self.hardcore = hardcore;
                self.difficulty = difficulty;
                self.reduced_debug = reduced_debug_info;
                self.inv = Inventory::default();
                self.set_gamemode(gamemode);
            }
            NetEvent::WorldBorder(u) => self.border.apply(u, now_ms()),
            NetEvent::Difficulty(d) => self.difficulty = d,
            NetEvent::Entity(EntityEvent::Metadata { id, flags, .. }) if id == self.entity_id => {
                self.entity_flags = flags.unwrap_or(self.entity_flags);
            }
            NetEvent::Health {
                health,
                food,
                saturation,
            } => {
                let was_alive = self.health > 0.0;
                if self.health_seen && health < self.health {
                    self.hurt_timer = HURT_TICKS;
                }
                self.health_seen = true;
                (self.health, self.food, self.saturation) = (health, food, saturation);
                return Ok((was_alive && health <= 0.0).then_some(Effect::Died));
            }
            ev @ (NetEvent::PlayerScore { .. } | NetEvent::Combat(_)) => {
                return Ok(self.apply_death_info(ev));
            }
            NetEvent::Experience { bar, level, total } => {
                (self.xp_bar, self.xp_level, self.xp_total) = (bar, level, total);
            }
            NetEvent::Abilities {
                flags,
                fly_speed,
                walk_speed,
            } => (self.abilities, self.fly_speed, self.walk_speed) = (flags, fly_speed, walk_speed),
            NetEvent::GameMode(mode) => self.set_gamemode(mode),
            other => return self.apply_effects(other).map(|()| None),
        }
        Ok(None)
    }

    /// Potion effects and attributes of the local player. `Err` hands back an event it does not own.
    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn apply_effects(&mut self, ev: NetEvent) -> Result<(), NetEvent> {
        match ev {
            NetEvent::EntityEffect { entity_id, effect } if entity_id == self.entity_id => {
                self.effects.add(effect);
            }
            NetEvent::RemoveEntityEffect {
                entity_id,
                effect_id,
            } if entity_id == self.entity_id => self.effects.remove(effect_id),
            NetEvent::MovementSpeed { entity_id, speed } if entity_id == self.entity_id => {
                self.speed = speed;
            }
            // Effects and attributes of other entities are not tracked.
            NetEvent::EntityEffect { .. }
            | NetEvent::RemoveEntityEffect { .. }
            | NetEvent::MovementSpeed { .. } => {}
            other => return Err(other),
        }
        Ok(())
    }

    /// `S2FSetSlot`: also starts the pick-up pop and flags a replaced held stack.
    fn set_slot(&mut self, window: i8, slot: i16, item: world::inventory::Slot) {
        self.held_replaced |= window == 0 && slot == 36 + i16::from(self.inv.held);
        if window == 0 && (36..45).contains(&slot) {
            let old = &self.inv.player[slot as usize];
            let grew = item
                .as_ref()
                .is_some_and(|n| old.as_ref().is_none_or(|o| o.count < n.count));
            self.bob[(slot - 36) as usize] = if grew { 5 } else { 0 };
        }
        self.inv.set_slot(window, slot, item);
    }

    /// Hotbar, slots and container windows.
    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn apply_window(&mut self, ev: NetEvent) -> Result<Option<Effect>, NetEvent> {
        let effect = match ev {
            NetEvent::HeldItem(slot) => {
                self.held_replaced |= self.inv.held != slot.min(8);
                self.inv.held = slot.min(8);
                None
            }
            NetEvent::SetSlot { window, slot, item } => {
                self.set_slot(window, slot, item);
                None
            }
            NetEvent::WindowItems { window, items } => {
                self.held_replaced |= window == 0;
                self.inv.set_items(window, items);
                None
            }
            NetEvent::OpenWindow {
                id,
                kind,
                title,
                slots,
            } => Some(if self.inv.open_window(id, &kind, title, slots) {
                Effect::OpenWindow
            } else {
                Effect::RejectWindow(id)
            }),
            NetEvent::CloseWindow => {
                self.inv.close_window();
                Some(Effect::CloseWindow)
            }
            NetEvent::WindowProperty {
                window,
                property,
                value,
            } => {
                self.inv.set_property(window, property, value);
                None
            }
            NetEvent::Transaction {
                window,
                action,
                accepted,
            } => (!accepted).then_some(Effect::Confirm { window, action }),
            other => return Err(other),
        };
        Ok(effect)
    }

    /// Chunks, blocks, entities, time, weather, chat and disconnects.
    fn apply_world(&mut self, ev: NetEvent) -> Option<Effect> {
        match ev {
            NetEvent::Respawn { dimension_changed } => {
                return Some(self.respawn(dimension_changed))
            }
            NetEvent::Chunk(c) => {
                let at = (c.x, c.z);
                self.store.insert(*c);
                return Some(Effect::Upload(at.0, at.1));
            }
            NetEvent::UnloadChunk(x, z) => {
                self.store.remove(x, z);
                return Some(Effect::Remove(x, z));
            }
            NetEvent::BlockChange { x, y, z, state } => {
                self.store.get(x >> 4, z >> 4)?;
                self.store.set_block(x, y, z, state);
                return Some(Effect::Upload(x >> 4, z >> 4));
            }
            NetEvent::Chat(m) => return Some(Effect::Chat(m)),
            NetEvent::ActionBar(m) => return Some(Effect::ActionBar(m)),
            NetEvent::BlockBreakAnim { id, pos, stage } => self.set_crack(id, pos, stage),
            NetEvent::Lightning { pos } => self.strike(pos),
            ev => self.apply_state(ev),
        }
        None
    }

    fn respawn(&mut self, dimension_changed: bool) -> Effect {
        self.entity_flags = 0;
        self.vehicle = None;
        if dimension_changed {
            self.border = WorldBorder::default();
        }
        self.camera = None;
        (self.health_seen, self.hurt_timer, self.death_ticks) = (false, 0, 0);
        (self.death_message, self.killer_id, self.score) = (None, -1, 0);
        self.effects.clear();
        self.speed = world::effects::Speed::default();
        self.potion_color = 0;
        self.potion_ambient = false;
        self.digger = Digger::default();
        self.cracks.clear();
        self.bolts.clear();
        self.store.clear();
        Effect::Clear
    }

    fn apply_entity(&mut self, e: EntityEvent) {
        if let EntityEvent::Destroy(ids) = &e {
            if self.vehicle.is_some_and(|v| ids.contains(&v)) {
                self.vehicle = None;
            }
        }
        if let EntityEvent::Attach { id, vehicle } = &e {
            if *id == self.entity_id {
                self.vehicle = (*vehicle >= 0).then_some(*vehicle);
            }
        }
        self.entities.apply(e)
    }

    fn apply_state(&mut self, ev: NetEvent) {
        match ev {
            NetEvent::Entity(EntityEvent::Velocity { id, vel }) if id == self.entity_id => {
                if let Some(p) = &mut self.player {
                    p.vel = vel.map(|v| f64::from(v) / 8000.0);
                }
            }
            NetEvent::Entity(EntityEvent::Status { id, status: 9 }) if id == self.entity_id => {
                self.finished_use = true;
            }
            NetEvent::Entity(EntityEvent::Potion { id, color, ambient })
                if id == self.entity_id =>
            {
                self.potion_color = color.unwrap_or(self.potion_color);
                self.potion_ambient = ambient.unwrap_or(self.potion_ambient);
            }
            NetEvent::Entity(EntityEvent::Status {
                id,
                status: s @ (22 | 23),
            }) if id == self.entity_id => self.reduced_debug = s == 22,
            // `doEvent(2)` and animation 1 (`animateDamage`) start the hit tilt.
            NetEvent::Entity(
                EntityEvent::Status { id, status: 2 } | EntityEvent::Animation { id, animation: 1 },
            ) if id == self.entity_id => self.hurt_timer = HURT_TICKS,
            NetEvent::Entity(e) => self.apply_entity(e),
            NetEvent::PlayerPosLook {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => self.teleport([x, y, z], yaw, pitch, flags),
            NetEvent::TimeUpdate { time_of_day, .. } => {
                self.daylight_cycle = time_of_day >= 0;
                self.world_time = time_of_day.wrapping_abs();
            }
            NetEvent::GameState { reason, value } => self.weather.apply(reason, value),
            NetEvent::Disconnect(r) => self.disconnected = Some(r),
            _ => {}
        }
    }

    /// Stage `0..=9` shows a crack, anything else clears the breaker's entry.
    fn set_crack(&mut self, id: i32, pos: [i32; 3], stage: i8) {
        if (0..10).contains(&stage) {
            self.cracks.insert(id, (pos, stage as u8, self.ticks));
        } else {
            self.cracks.remove(&id);
        }
    }

    /// S08: bits 0..=2 make x/y/z relative, bit 3 yaw, bit 4 pitch.
    fn teleport(&mut self, pos: [f64; 3], yaw: f32, pitch: f32, flags: u8) {
        let rel = |bit: u8| flags & (1 << bit) != 0;
        let (old_pos, old_yaw) = self
            .player
            .as_ref()
            .map_or(([0.0; 3], 0.0), |p| (p.pos, p.yaw));
        let mut at = pos;
        for (i, v) in at.iter_mut().enumerate() {
            if rel(i as u8) {
                *v += old_pos[i];
            }
        }
        let yaw = if rel(3) { old_yaw + yaw } else { yaw };
        self.pitch = if rel(4) { self.pitch + pitch } else { pitch };
        // `Player::new` rebuilds the private bounding box, which a bare `pos` write would not.
        self.player = Some(Player::new(at, yaw));
        self.prev_pos = at;
        self.teleported = true;
    }

    /// Apply a look delta in degrees; pitch clamps to +-90 like `Entity.setAngles`.
    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        if let Some(p) = &mut self.player {
            p.yaw += dyaw;
        }
        self.pitch = (self.pitch + dpitch).clamp(-90.0, 90.0);
    }

    /// Whether the local player burns visibly (spectators show no fire).
    pub fn burning(&self) -> bool {
        self.entity_flags & world::entities::flag::BURNING != 0 && self.gamemode != 3
    }

    /// The flight toggle of `EntityPlayerSP.onLivingUpdate`, before the movement: a second jump press within
    /// 7 ticks flips `isFlying` of a player that may fly, and a spectator always flies. `true` means `abilities`
    /// changed and vanilla's `sendPlayerAbilities` (C13) is due.
    pub fn fly_toggle(&mut self, jump: bool) -> bool {
        let was_held = std::mem::replace(&mut self.jump_held, jump);
        let pressed = jump && !was_held;
        let mut changed = false;
        if self.player.is_some() && self.abilities & 4 != 0 {
            if self.gamemode == SPECTATOR {
                changed = self.abilities & 2 == 0;
                self.abilities |= 2;
            } else if pressed {
                if self.fly_toggle_timer == 0 {
                    self.fly_toggle_timer = 7;
                } else {
                    self.abilities ^= 2;
                    self.fly_toggle_timer = 0;
                    changed = true;
                }
            }
        }
        // `EntityPlayer.onLivingUpdate`, right after the toggle.
        self.fly_toggle_timer = self.fly_toggle_timer.saturating_sub(1);
        changed
    }

    /// After the movement: landing ends flight outside spectator mode. `true` as for `fly_toggle`.
    pub fn fly_land(&mut self) -> bool {
        let landed = self.player.as_ref().is_some_and(|p| p.on_ground);
        let flying = self.abilities & 2 != 0 && self.gamemode != SPECTATOR;
        if landed && flying {
            self.abilities &= !2;
        }
        landed && flying
    }

    /// One 20 TPS step. The player stays frozen until the chunk under it has arrived, so
    /// it does not fall through the void while the world loads.
    pub fn tick(&mut self, input: Input) {
        self.ticks += 1;
        if self.daylight_cycle {
            self.world_time += 1;
        }
        self.weather.tick();
        self.bolts.tick();
        self.swing.tick();
        for b in &mut self.bob {
            *b = (*b - 1).max(0);
        }
        self.hurt_timer = self.hurt_timer.saturating_sub(1);
        self.death_ticks = if self.health > 0.0 {
            0
        } else {
            (self.death_ticks + 1).min(20)
        };
        self.entities.tick();
        if self.ticks.is_multiple_of(20) {
            // `RenderGlobal.updateDamagedBlocks`: a stage nobody refreshed for 400 ticks goes.
            let now = self.ticks;
            self.cracks.retain(|_, c| now - c.2 <= 400);
        }
        self.effects.tick();
        let Some(p) = &mut self.player else { return };
        self.prev_pos = p.pos;
        let (cx, cz) = (
            (p.pos[0].floor() as i32) >> 4,
            (p.pos[2].floor() as i32) >> 4,
        );
        // `Entity.updateRiderPosition`: the rider sits on its vehicle (the player's `getYOffset` is -0.35).
        if let Some(v) = self.vehicle.and_then(|id| self.entities.get(id)) {
            let off = v.kind.mounted_y_offset() + f64::from(-0.35f32);
            p.pos = [v.pos[0], v.pos[1] + off, v.pos[2]];
            p.vel = [0.0; 3];
            p.on_ground = false;
        } else if self.store.get(cx, cz).is_some() {
            let flying = self.abilities & 2 != 0;
            let input = world::Input {
                fly_speed: flying.then_some(self.fly_speed),
                ..input
            };
            p.speed_attr = Some(self.speed.value(input.sprint));
            p.jump_boost = self
                .effects
                .get(world::effects::id::JUMP_BOOST)
                .map_or(0.0, |e| f32::from(e.amplifier) + 1.0);
            let now = now_ms();
            self.border.settle(now);
            let walls = self.border.bounds(now);
            // `isInsideBorder` decides, and `setOutsideBorder` remembers, whether the walls are solid.
            let inside = WorldBorder::inside(walls, p.pos[0], p.pos[2], self.border_outside);
            self.border_outside = !inside;
            let collision = Collision {
                store: &self.store,
                walls: inside.then_some(walls),
            };
            p.tick(&collision, input);
        }
    }

    fn set_gamemode(&mut self, mode: u8) {
        self.gamemode = mode;
        self.creative = mode == CREATIVE;
        self.inv.creative = self.creative;
    }

    /// World-space outline box of the targeted block, grown by 0.002 like vanilla.
    pub fn selection(&self, eye: [f64; 3], yaw: f32, pitch: f32) -> Option<[f64; 6]> {
        let (hit, _) = self.ray_dist(eye, yaw, pitch)?;
        let at = |p: [i32; 3]| self.store.get_block(p[0], p[1], p[2]);
        let b = dig::selection_box(&at, hit.pos)?;
        let o = hit.pos.map(f64::from);
        let e = 0.002;
        Some([
            o[0] + b[0] - e,
            o[1] + b[1] - e,
            o[2] + b[2] - e,
            o[0] + b[3] + e,
            o[1] + b[4] + e,
            o[2] + b[5] + e,
        ])
    }

    /// The block under the crosshair of a viewer at `eye` looking along `yaw`, `pitch`, within
    /// the game mode's reach, plus the distance to the hit point.
    pub fn ray_dist(&self, eye: [f64; 3], yaw: f32, pitch: f32) -> Option<(Hit, f64)> {
        let at = |pos: [i32; 3]| self.store.get_block(pos[0], pos[1], pos[2]);
        dig::raycast_dist(&at, eye, yaw, pitch, dig::reach(self.creative))
    }

    /// A selectable block lies between `from` and `to`.
    pub fn line_blocked(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let d = [0, 1, 2].map(|i| to[i] - from[i]);
        let len = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        if len < 1e-6 {
            return false;
        }
        let yaw = (-d[0]).atan2(d[2]).to_degrees() as f32;
        let pitch = (-d[1] / len).asin().to_degrees() as f32;
        let at = |pos: [i32; 3]| self.store.get_block(pos[0], pos[1], pos[2]);
        dig::raycast_dist(&at, from, yaw, pitch, len).is_some()
    }

    /// The local player's bounding box `[x0, y0, z0, x1, y1, z1]`, which blocks a placement.
    pub fn body(&self) -> [f64; 6] {
        let [x, y, z] = self.player.as_ref().map_or([0.0; 3], |p| p.pos);
        [x - 0.3, y, z - 0.3, x + 0.3, y + 1.8, z + 0.3]
    }

    /// Every box `World.checkNoEntityCollision` tests: the local player and each entity with
    /// `preventEntitySpawning`.
    pub fn bodies(&self) -> Vec<[f64; 6]> {
        let mut v = vec![self.body()];
        v.extend(
            self.entities
                .iter()
                .filter(|e| e.blocks_placement())
                .map(|e| {
                    let (min, max) = e.bounds();
                    [min[0], min[1], min[2], max[0], max[1], max[2]]
                }),
        );
        v
    }

    /// The client half of `ItemBlock.onItemUse`: set the blocks and use up one item outside
    /// creative mode. Unloaded chunks are skipped.
    pub fn place_blocks(&mut self, blocks: &[([i32; 3], u16)]) -> Vec<Effect> {
        let mut touched = Vec::new();
        for &([x, y, z], state) in blocks {
            if self.store.get(x >> 4, z >> 4).is_some() {
                self.store.set_block(x, y, z, state);
                touched.push((x >> 4, z >> 4));
            }
        }
        if !touched.is_empty() && !self.creative {
            let slot = 36 + usize::from(self.inv.held);
            let stack = &mut self.inv.player[slot];
            if let Some(s) = stack {
                s.count -= 1;
                if s.count <= 0 {
                    *stack = None;
                }
            }
        }
        touched.sort_unstable();
        touched.dedup();
        touched
            .into_iter()
            .map(|(x, z)| Effect::Upload(x, z))
            .collect()
    }

    /// Blocks to draw cracked: the local dig and other players' digs within 32 blocks.
    pub fn cracks(&self) -> Vec<([i32; 3], u8)> {
        let near = |at: [i32; 3]| {
            self.player.as_ref().is_none_or(|p| {
                (0..3)
                    .map(|i| (f64::from(at[i]) - p.pos[i]).powi(2))
                    .sum::<f64>()
                    <= 1024.0
            })
        };
        self.digger
            .stage()
            .into_iter()
            .chain(self.cracks.values().map(|c| (c.0, c.1)))
            .filter(|c| near(c.0))
            .collect()
    }

    /// `PlayerControllerMP.getIsHittingBlock`: right click does nothing while a block is dug.
    pub fn is_hitting_block(&self) -> bool {
        self.digger.is_hitting()
    }

    /// One client tick of block breaking. Returns the packets to send and the chunks whose
    /// blocks the player removed locally. `entity` is the entity under the crosshair: it hides
    /// the block `block` and a click attacks it, as in vanilla's `objectMouseOver`. Only
    /// survival and creative can dig.
    pub fn dig_tick(
        &mut self,
        attack: Attack,
        entity: Option<i32>,
        block: Option<Hit>,
    ) -> (Vec<Action>, Vec<Effect>) {
        let (mut actions, mut touched) = (Vec::new(), Vec::new());
        let Some(p) = &self.player else {
            return (actions, Vec::new());
        };
        let frame = Frame {
            held: attack.held,
            presses: attack.presses,
            grabbed: false,
            creative: self.creative,
            no_dig: self.gamemode > CREATIVE,
            on_ground: p.on_ground,
            target: block.filter(|_| entity.is_none()),
            entity,
        };
        let mut local = Local {
            store: &mut self.store,
            touched: &mut touched,
            broken: &mut self.broken,
        };
        self.digger.tick(&frame, &mut local, &mut actions);
        touched.sort_unstable();
        touched.dedup();
        let effects = touched.into_iter().map(|(x, z)| Effect::Upload(x, z));
        (actions, effects.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::chunk::Chunk;

    fn floor_chunk() -> Chunk {
        let mut c = Chunk::new(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                c.set_block(x, 63, z, 1 << 4);
            }
        }
        c
    }

    const IDLE: Input = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
        sneak: false,
        using_item: false,
        fly_speed: None,
    };

    #[test]
    fn lightning_event_flashes_the_sky_until_the_bolt_expires() {
        let mut g = Game::new();
        assert_eq!(g.bolts.flash(0.0), 0.0);
        g.apply(NetEvent::Lightning {
            pos: [1.0, 64.0, 1.0],
        });
        g.tick(IDLE);
        assert!(g.bolts.flash(0.0) > 0.0);
        // A bolt strikes at most 3 times, each followed by a delay under 10 ticks.
        for _ in 0..100 {
            g.tick(IDLE);
        }
        assert!(g.bolts.is_empty());
        assert_eq!(g.bolts.flash(0.0), 0.0);
    }

    #[test]
    fn a_new_held_stack_or_slot_ends_the_item_use() {
        let mut g = Game::new();
        g.apply(NetEvent::HeldItem(0));
        g.apply(NetEvent::SetSlot {
            window: 0,
            slot: 20,
            item: None,
        });
        assert!(!g.held_replaced, "same slot and another slot keep the use");
        g.apply(NetEvent::SetSlot {
            window: 0,
            slot: 36,
            item: None,
        });
        assert!(std::mem::take(&mut g.held_replaced));
        g.apply(NetEvent::HeldItem(3));
        assert!(std::mem::take(&mut g.held_replaced));
    }

    #[test]
    fn placing_uses_up_one_item_outside_creative() {
        let mut g = Game::new();
        g.store.insert(world::chunk::Chunk::new(0, 0));
        let stack = |n| {
            Some(world::inventory::ItemStack {
                id: 1,
                count: n,
                damage: 0,
                nbt: None,
            })
        };
        g.inv.player[36] = stack(2);
        assert_eq!(
            g.place_blocks(&[([1, 4, 1], 1 << 4)]),
            vec![Effect::Upload(0, 0)]
        );
        assert_eq!(g.store.get_block(1, 4, 1), 1 << 4);
        assert_eq!(g.inv.player[36], stack(1));
        g.place_blocks(&[([2, 4, 1], 1 << 4)]);
        assert_eq!(g.inv.player[36], None);
        g.inv.player[36] = stack(5);
        g.creative = true;
        g.place_blocks(&[([3, 4, 1], 1 << 4)]);
        assert_eq!(g.inv.player[36], stack(5));
    }

    #[test]
    fn relative_teleport_adds_and_absolute_replaces() {
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 1.0,
            y: 70.0,
            z: 2.0,
            yaw: 10.0,
            pitch: 5.0,
            flags: 0,
        });
        g.apply(NetEvent::PlayerPosLook {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            yaw: 5.0,
            pitch: 5.0,
            flags: 0b11111,
        });
        let p = g.player.as_ref().unwrap();
        assert!(g.teleported);
        assert_eq!(g.prev_pos, [2.0, 71.0, 3.0]);
        assert_eq!(p.pos, [2.0, 71.0, 3.0]);
        assert_eq!(p.yaw, 15.0);
        assert_eq!(g.pitch, 10.0);
    }

    #[test]
    fn look_clamps_pitch_to_vertical_and_accumulates_yaw() {
        // Vanilla `Entity.setAngles` clamps pitch to [-90, 90].
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.look(30.0, 1000.0);
        assert_eq!(g.pitch, 90.0);
        g.look(0.0, 10.0);
        assert_eq!(g.pitch, 90.0);
        g.look(0.0, -1000.0);
        assert_eq!(g.pitch, -90.0);
        g.look(15.0, 45.0);
        assert_eq!(g.pitch, -45.0);
        assert_eq!(g.player.as_ref().unwrap().yaw, 45.0);
    }

    #[test]
    fn player_waits_for_chunk_then_lands_on_floor() {
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 8.5,
            y: 66.0,
            z: 8.5,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.tick(IDLE);
        assert_eq!(g.player.as_ref().unwrap().pos[1], 66.0);
        assert_eq!(
            g.apply(NetEvent::Chunk(Box::new(floor_chunk()))),
            Some(Effect::Upload(0, 0))
        );
        for _ in 0..40 {
            g.tick(IDLE);
        }
        let p = g.player.as_ref().unwrap();
        assert_eq!(p.pos[1], 64.0);
        assert!(p.on_ground);
    }

    #[test]
    fn block_change_in_unloaded_chunk_is_dropped() {
        let mut g = Game::new();
        assert_eq!(
            g.apply(NetEvent::BlockChange {
                x: 0,
                y: 1,
                z: 0,
                state: 16
            }),
            None
        );
    }

    #[test]
    fn only_drawn_cubes_collide_and_respawn_drops_chunks() {
        // stone, oak door, glass pane, carpet, snow layer, water, lava, ice
        let solid = [1, 64, 102, 171, 78, 8, 10, 79].map(|id| physics_block(id << 4));
        let want = [
            Block::Stone,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Air,
            Block::Ice,
        ];
        assert!(solid.iter().zip(&want).all(|(a, b)| a == b));
        let mut g = Game::new();
        g.apply(NetEvent::Chunk(Box::new(floor_chunk())));
        assert_eq!(
            g.apply(NetEvent::Respawn {
                dimension_changed: false
            }),
            Some(Effect::Clear)
        );
        assert!(g.store.is_empty());
    }

    #[test]
    fn health_loss_and_status_start_hurt_and_death_counts_to_twenty() {
        let mut g = Game::new();
        g.entity_id = 7;
        let hp = |health| NetEvent::Health {
            health,
            food: 20,
            saturation: 5.0,
        };
        g.apply(hp(20.0));
        assert_eq!(g.hurt_timer, 0, "first packet is no damage");
        g.apply(hp(16.0));
        assert_eq!(g.hurt_timer, 10);
        g.tick(IDLE);
        assert_eq!(g.hurt_timer, 9);
        g.apply(NetEvent::Entity(EntityEvent::Status { id: 7, status: 2 }));
        assert_eq!(g.hurt_timer, 10);
        g.apply(hp(0.0));
        for _ in 0..30 {
            g.tick(IDLE);
        }
        assert_eq!(g.death_ticks, 20);
    }

    #[test]
    fn time_runs_unless_frozen_and_local_velocity_and_status_apply() {
        let mut g = Game::new();
        g.entity_id = 7;
        g.apply(NetEvent::TimeUpdate {
            world_age: 0,
            time_of_day: 100,
        });
        g.tick(IDLE);
        assert_eq!(g.world_time, 101);
        g.apply(NetEvent::TimeUpdate {
            world_age: 0,
            time_of_day: -200,
        });
        g.tick(IDLE);
        assert_eq!(g.world_time, 200);
        g.apply(NetEvent::PlayerPosLook {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.apply(NetEvent::Entity(EntityEvent::Velocity {
            id: 7,
            vel: [8000, -4000, 0],
        }));
        assert_eq!(g.player.as_ref().unwrap().vel, [1.0, -0.5, 0.0]);
        g.apply(NetEvent::Entity(EntityEvent::Status { id: 7, status: 22 }));
        assert!(g.reduced_debug);
    }

    fn flier(abilities: u8, gamemode: u8) -> Game {
        let mut g = Game::new();
        g.apply(NetEvent::PlayerPosLook {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        g.abilities = abilities;
        g.gamemode = gamemode;
        g
    }

    /// Presses jump on tick 0 and again `gap` ticks later; `true` when the second press flips flight.
    fn second_press_toggles(g: &mut Game, gap: usize) -> bool {
        assert!(!g.fly_toggle(true));
        for _ in 1..gap {
            assert!(!g.fly_toggle(false));
        }
        g.fly_toggle(true)
    }

    #[test]
    fn a_second_jump_press_within_the_window_toggles_flight() {
        // Oracle: decompiled 1.8.9 `EntityPlayerSP.onLivingUpdate` sets `flyToggleTimer` 7 and
        // `EntityPlayer.onLivingUpdate` counts it down in the same tick, so the 6th tick after the first press
        // is still a toggle and the 7th is a new first press.
        let mut g = flier(4, 1);
        assert!(second_press_toggles(&mut g, 6));
        assert_eq!(g.abilities, 6);
        let mut g = flier(4, 1);
        assert!(!second_press_toggles(&mut g, 7));
        assert_eq!(g.abilities, 4);
        let mut g = flier(6, 1);
        assert!(second_press_toggles(&mut g, 2));
        assert_eq!(g.abilities, 4, "a second double tap ends flight");
    }

    #[test]
    fn a_held_jump_or_a_player_that_may_not_fly_never_toggles() {
        let mut g = flier(4, 1);
        assert!(!g.fly_toggle(true));
        for _ in 0..20 {
            assert!(!g.fly_toggle(true));
        }
        let mut g = flier(0, 0);
        assert!(!second_press_toggles(&mut g, 2));
        assert_eq!(g.abilities, 0);
    }

    #[test]
    fn landing_ends_flight_but_not_in_spectator_mode() {
        let mut g = flier(6, 1);
        g.player.as_mut().unwrap().on_ground = false;
        assert!(!g.fly_land());
        g.player.as_mut().unwrap().on_ground = true;
        assert!(g.fly_land());
        assert_eq!(g.abilities, 4);
        assert!(!g.fly_land(), "only the landing sends");
        let mut g = flier(6, 3);
        g.player.as_mut().unwrap().on_ground = true;
        assert!(!g.fly_land());
        assert_eq!(g.abilities, 6);
    }

    #[test]
    fn a_flying_player_hovers_and_is_not_landed() {
        // Oracle: decompiled 1.8.9 `EntityPlayer.moveEntityWithHeading` keeps 0.6 of the vertical speed while
        // `isFlying`, so with no key held a player at rest stays in the air; `fly_land` then never fires.
        let mut g = flier(6, 1);
        g.store.insert(floor_chunk());
        g.player = Some(world::Player::new([0.5, 70.0, 0.5], 0.0));
        for _ in 0..40 {
            g.tick(IDLE);
            assert!(!g.fly_land());
        }
        let y = g.player.as_ref().unwrap().pos[1];
        assert_eq!(y, 70.0, "no gravity while flying");
        assert_eq!(g.abilities, 6);
        g.abilities = 4;
        for _ in 0..40 {
            g.tick(IDLE);
        }
        assert!(
            g.player.as_ref().unwrap().on_ground,
            "a walking player falls to the floor"
        );
        assert!(g.player.as_ref().unwrap().pos[1] < y);
    }

    #[test]
    fn a_spectator_that_may_fly_starts_flying() {
        let mut g = flier(4, 3);
        assert!(g.fly_toggle(false));
        assert_eq!(g.abilities, 6);
        assert!(!g.fly_toggle(false));
    }

    #[test]
    fn combat_death_keeps_message_killer_and_score_until_respawn() {
        use ferrocraft_protocol::codec::VarInt;
        let mut g = Game::new();
        g.entity_id = 7;
        g.apply(NetEvent::PlayerScore {
            entity_id: 7,
            score: 42,
        });
        let dead = |player| {
            NetEvent::Combat(CombatEventKind::EntityDead {
                player_id: VarInt(player),
                entity_id: 9,
                message: "{\"text\":\"Steve was slain by Bob\"}".into(),
            })
        };
        assert_eq!(g.apply(dead(8)), None, "another player's death is ignored");
        assert_eq!(g.apply(dead(7)), Some(Effect::Died));
        assert_eq!(g.score, 42);
        assert_eq!(g.killer_id, 9);
        assert!(g
            .death_message
            .as_deref()
            .unwrap()
            .contains("Steve was slain by Bob"));
        g.apply(NetEvent::Respawn {
            dimension_changed: false,
        });
        assert_eq!(g.death_message, None);
        assert_eq!(g.score, 0);
        assert_eq!(g.killer_id, -1);
        g.apply(dead(7));
        g.apply(NetEvent::JoinGame {
            entity_id: 7,
            gamemode: 0,
            hardcore: false,
            difficulty: 1,
            max_players: 20,
            reduced_debug_info: false,
        });
        assert_eq!(
            (g.death_message.as_deref(), g.killer_id, g.score),
            (None, -1, 0)
        );
    }

    #[test]
    fn local_fire_metadata_survives_partial_updates_and_resets_on_respawn() {
        let mut g = Game::new();
        g.entity_id = 7;
        let flags = |id, flags| {
            NetEvent::Entity(EntityEvent::Metadata {
                id,
                flags,
                name: None,
                health: None,
                stack: None,
                size: world::entities::SizeMeta::default(),
            })
        };
        g.apply(flags(8, Some(1)));
        assert_eq!(g.entity_flags, 0);
        g.apply(flags(7, Some(1)));
        g.apply(flags(7, None));
        assert_eq!(g.entity_flags, 1);
        g.apply(flags(7, Some(0)));
        assert_eq!(g.entity_flags, 0);
        g.apply(flags(7, Some(1)));
        g.apply(NetEvent::Respawn {
            dimension_changed: false,
        });
        assert_eq!(g.entity_flags, 0);
    }

    #[test]
    fn respawn_resets_border_only_on_dimension_change() {
        let mut g = Game::new();
        g.apply(NetEvent::WorldBorder(world::border::Update::SetSize(100.0)));
        assert_ne!(g.border, WorldBorder::default());
        g.apply(NetEvent::Respawn {
            dimension_changed: false,
        });
        assert_ne!(g.border, WorldBorder::default());
        g.apply(NetEvent::Respawn {
            dimension_changed: true,
        });
        assert_eq!(g.border, WorldBorder::default());
    }
}
