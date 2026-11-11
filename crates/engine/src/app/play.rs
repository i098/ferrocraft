//! The 20 TPS step and the player's actions: movement packets, digging, attacking, using items.

use super::{boxed, App, View, MAX_TICKS_PER_FRAME, TICK};
use crate::game::{Attack, Effect};
use gui::inventory::ContainerScreen;
use gui::screens::Pause;
use input_audio::{apply_cursor_grab, scroll_slot, Action, DebugCombo};
use std::time::Instant;
use world::dig::{look_vector, Action as DigAction};
use world::entities::Kind;
use world::inventory::ItemStack;
use world::place::{self, Click, Outcome};

/// Items with the EAT use action: every food of 1.8.9.
pub(super) const FOOD: [i16; 26] = [
    260, 282, 297, 319, 320, 349, 350, 357, 360, 363, 364, 365, 366, 367, 375, 391, 392, 393, 394,
    396, 400, 411, 412, 413, 423, 424,
];

/// `rightClickDelayTimer` after a use.
const USE_DELAY: i32 = 4;

/// `Block.onBlockActivated` on the client: whether a right click on `state` is consumed.
fn activates(state: u16, held: Option<i16>, gamemode: u8) -> bool {
    let edit = gamemode != 2;
    match state >> 4 {
        23
        | 25
        | 26
        | 54
        | 58
        | 61
        | 62
        | 63
        | 64
        | 68
        | 69
        | 71
        | 77
        | 92
        | 96
        | 107
        | 116
        | 117
        | 118
        | 122
        | 130
        | 138
        | 143
        | 145
        | 146
        | 154
        | 158
        | 167
        | 183..=187
        | 193..=197 => true,
        93 | 94 | 149 | 150 | 151 | 178 => edit,
        84 => state & 1 != 0,
        137 => gamemode == 1,
        46 => matches!(held, Some(259 | 385)),
        _ => false,
    }
}

/// `Item.onItemUse` on the client for a click on `face` of `state`, with `above` over it:
/// whether the item is consumed. Adventure mode (`canPlayerEdit` false) stops most items.
fn item_used_on(id: i16, state: u16, above: u16, face: u8, gamemode: u8) -> bool {
    match id {
        355 | 383 | 385 => true,
        2256..=2267 => state >> 4 == 84 && state & 1 == 0,
        420 => matches!(state >> 4, 85 | 113 | 188..=192),
        _ => gamemode != 2 && edit_item_used_on(id, state, above, face),
    }
}

fn edit_item_used_on(id: i16, state: u16, above: u16, face: u8) -> bool {
    let block = state >> 4;
    match id {
        259 | 287 | 331 | 338 | 354 | 356 | 379 | 380 | 390 | 404 => true,
        397 | 416 | 425 => face != 0,
        321 | 389 => face >= 2,
        290..=295 | 361 | 362 | 391 | 392 | 372 => planted(id, state, face, above >> 4 == 0),
        328 | 342 | 343 | 407 | 408 | 422 => matches!(block, 27 | 28 | 66 | 157),
        381 => block == 120 && state & 4 == 0,
        _ => false,
    }
}

/// Seeds, nether wart and hoes: they need a free block over the clicked face.
fn planted(id: i16, state: u16, face: u8, free_above: bool) -> bool {
    let block = state >> 4;
    if face == 0 || !free_above {
        return false;
    }
    match id {
        295 | 361 | 362 | 391 | 392 => face == 1 && block == 60,
        372 => face == 1 && block == 88,
        _ => block == 2 || (block == 3 && state & 15 < 2),
    }
}

/// `EntityPlayerSP.cameraYaw`/`cameraPitch` and `distanceWalkedModified`, with the values of the
/// tick before, for `EntityRenderer.setupViewBobbing`.
#[derive(Default)]
pub(super) struct ViewBob {
    walked: [f32; 2],
    yaw: [f32; 2],
    pitch: [f32; 2],
}

impl ViewBob {
    fn tick(&mut self, d: [f64; 3], on_ground: bool, alive: bool) {
        let h = d[0].hypot(d[2]) as f32;
        let speed = if on_ground && alive { h.min(0.1) } else { 0.0 };
        let lift = if on_ground || !alive {
            0.0
        } else {
            (-d[1] as f32 * 0.2).atan() * 15.0
        };
        let [w, y, p] = [self.walked[0], self.yaw[0], self.pitch[0]];
        self.walked = [w + h * 0.6, w];
        self.yaw = [y + (speed - y) * 0.4, y];
        self.pitch = [p + (lift - p) * 0.8, p];
    }

    /// Translation x and y, roll and pitch of the view at `partial`.
    pub(super) fn offsets(&self, partial: f32) -> [f32; 4] {
        let lerp = |v: [f32; 2]| v[1] + (v[0] - v[1]) * partial;
        let a =
            -(self.walked[0] + (self.walked[0] - self.walked[1]) * partial) * std::f32::consts::PI;
        let yaw = lerp(self.yaw);
        [
            a.sin() * yaw * 0.5,
            -(a.cos() * yaw).abs(),
            a.sin() * yaw * 3.0,
            ((a - 0.2).cos() * yaw).abs() * 5.0 + lerp(self.pitch),
        ]
    }
}

impl App {
    pub(super) fn run_ticks(&mut self) {
        let now = Instant::now();
        let mut ran = 0;
        while self.next_tick <= now && ran < MAX_TICKS_PER_FRAME {
            self.next_tick += TICK;
            ran += 1;
            self.tick_once();
        }
        if ran == MAX_TICKS_PER_FRAME {
            self.next_tick = now + TICK;
        }
    }

    fn tick_once(&mut self) {
        self.chat_tick();
        self.hud.tick();
        if self.game.player.is_some() {
            self.hud.update_held(self.game.inv.held_stack());
        }
        let m = metrics!(self);
        let q = self.ui.tick(&m);
        self.run_effects(q);
        if let Some(c) = &mut self.container {
            c.tick();
            c.sync_game_mode(&self.game.inv);
        }
        let playing = self.game.player.is_some() && !self.ui.is_open() && self.container.is_none();
        if playing {
            self.dig();
            self.use_tick();
        }
        self.fov_tick();
        if !playing {
            self.attack_presses = 0;
        }
        self.limb_tick();
        let host = self.host_player_tick();
        let input = if playing {
            let native = |s: &mut Self| {
                s.game
                    .player
                    .as_ref()
                    .map_or_else(world::Input::default, |p| {
                        s.input
                            .movement(p.on_ground, p.collided_horizontally, s.game.food > 6)
                    })
            };
            match host {
                Some(h) => self.input.movement_from_host(h.sprinting, h.sneaking),
                None => native(self),
            }
        } else {
            self.input.set_host_sneak(host.map(|h| h.sneaking));
            world::Input::default()
        };
        let mut abilities = self.game.fly_toggle(input.jump);
        self.water_sounds();
        self.game.tick(input);
        self.fall_sounds();
        abilities |= self.game.fly_land();
        if abilities {
            self.send_abilities();
        }
        if std::mem::take(&mut self.game.held_replaced) {
            self.use_ticks = 0;
        }
        self.consume_sounds();
        self.use_ticks = (self.use_ticks - 1).max(0);
        self.boss_tick();
        self.step_sound();
        self.rain_particles();
        self.potion_particles();
        self.entity_fx();
        self.particles
            .tick(|x, y, z| self.game.store.get_block(x, y, z));
        self.equip_tick();
        self.send_state();
        self.send_pose();
        self.sound_tick();
    }

    /// Crit bursts, death puffs and the sprint dust of every sprinting entity not in water.
    fn entity_fx(&mut self) {
        for fx in self.game.entities.take_fx() {
            match fx {
                world::entities::Fx::Hit { magic, pos, size } => {
                    self.particles.hit_burst(magic, pos, size)
                }
                world::entities::Fx::Death { pos, size } => self.particles.death_puff(pos, size),
            }
        }
        let g = &self.game;
        let block = |x, y, z| g.store.get_block(x, y, z);
        let mut dust = Vec::new();
        let mut add = |pos: [f64; 3], vel: [f64; 3], (width, height): (f32, f32)| {
            if let Some(state) = sprint_dust_state(pos, height, block) {
                dust.push((pos, vel, width, state));
            }
        };
        for e in g.entities.iter().filter(|e| e.sprinting()) {
            let vel = [0, 1, 2].map(|i| e.pos[i] - e.prev_pos[i]);
            add(e.pos, vel, e.size());
        }
        if let Some(p) = g.player.as_ref().filter(|p| p.sprinting) {
            add(p.pos, p.vel, (0.6, 1.8));
        }
        for (pos, vel, width, state) in dust {
            self.particles.sprint_dust(pos, vel, width, state);
        }
    }

    /// `EntityLivingBase.onEntityUpdate`: swirls of every living entity with potion effects.
    fn potion_particles(&mut self) {
        let g = &self.game;
        for e in g
            .entities
            .iter()
            .filter(|e| e.potion_color > 0 && !e.is_dead())
        {
            self.particles.potion_swirl(
                e.pos,
                e.size(),
                e.potion_color,
                e.potion_ambient,
                e.invisible(),
            );
        }
        self.local_potion_particles();
    }

    fn local_potion_particles(&mut self) {
        let g = &self.game;
        if let Some(p) = &g.player {
            let invisible = g.effects.has(world::effects::id::INVISIBILITY);
            self.particles.potion_swirl(
                p.pos,
                (0.6, 1.8),
                g.potion_color,
                g.potion_ambient,
                invisible,
            );
        }
    }

    /// `EntityPlayerSP.sendPlayerAbilities`: C13 with the flags, fly speed and walk speed.
    fn send_abilities(&mut self) {
        let g = &self.game;
        let (flags, fly, walk) = (g.abilities, g.fly_speed, g.walk_speed);
        let r = self.session_send(|s| s.abilities(flags, fly, walk));
        self.send_result(r);
    }

    /// One tick of the mod host's stand-in `EntityPlayerSP.onLivingUpdate`, where mods hook sprint and sneak.
    fn host_player_tick(&mut self) -> Option<ferrocraft_modhost::PlayerInput> {
        let (mods, p) = (self.mods.as_ref()?, self.game.player.as_ref()?);
        mods.player_tick(ferrocraft_modhost::PlayerState {
            on_ground: p.on_ground,
            collided_horizontally: p.collided_horizontally,
            food: self.game.food,
            allow_flying: self.game.abilities & 4 != 0,
            flying: self.game.abilities & 2 != 0,
            using_item: self.use_ticks > 0,
        })
    }

    fn chat_tick(&mut self) {
        if let Some(line) = self.input.take_chat_submit() {
            if let Some(s) = self.link.session() {
                let r = s.send_chat(&line);
                self.send_result(r);
            }
        }
        let chat_open = self.input.chat_input().is_some();
        if self.chat_open && !chat_open {
            self.hud.chat.reset_scroll();
        }
        self.chat_open = chat_open;
    }

    /// `EntityRenderer.addRainParticles`, run once per client tick.
    fn rain_particles(&mut self) {
        let Some(p) = self.game.player.as_ref() else {
            return;
        };
        let store = &self.game.store;
        let top = |x: i32, z: i32| {
            store.get(x >> 4, z >> 4)?;
            Some(
                (0..256)
                    .rev()
                    .find(|&y| render::stops_rain(store.get_block(x, y, z)))
                    .map_or(0, |y| y + 1),
            )
        };
        let (rain, _) = self.game.weather.strengths(1.0);
        let settings = &self.ui.settings;
        let drops = render::rain_particles(
            rain,
            settings.fancy_graphics,
            settings.particles,
            self.game.ticks,
            p.pos,
            top,
            |x, y, z| store.get_block(x, y, z),
        );
        for (kind, at) in drops {
            self.particles.spawn(kind, at, [0.0; 3], 0);
        }
    }

    fn sound_tick(&mut self) {
        let at = self.game.player.as_ref().map(|p| p.pos);
        if let Some(s) = &mut self.sound {
            if let Some(at) = at {
                s.set_listener(at);
            }
            s.tick(self.ui.in_world);
        }
    }

    /// `BossStatus.setBossStatus` for a Wither (id 64, 300 hp) or Ender Dragon (id 63, 200 hp) in the world.
    fn boss_tick(&mut self) {
        let boss = self.game.entities.iter().find_map(|e| {
            let (default, max) = match e.kind {
                Kind::Mob(64) => ("Wither", 300.0),
                Kind::Mob(63) => ("Ender Dragon", 200.0),
                _ => return None,
            };
            Some((
                e.name.clone().unwrap_or_else(|| default.into()),
                max,
                e.health?,
            ))
        });
        if let Some((name, max, health)) = boss {
            self.hud.set_boss(name, (health / max).clamp(0.0, 1.0));
        }
    }

    /// Sprint and sneak changes go to the server as `C0B EntityAction`.
    fn send_state(&mut self) {
        if self.game.player.is_none() {
            return;
        }
        let (sneak, sprint) = (
            self.input.sneaking(),
            self.game.player.as_ref().is_some_and(|p| p.sprinting),
        );
        let id = self.game.entity_id;
        let mut sends = Vec::new();
        if sneak != self.sneak_sent {
            self.sneak_sent = sneak;
            sends.push(if sneak { 0 } else { 1 });
        }
        if sprint != self.sprint_sent {
            self.sprint_sent = sprint;
            sends.push(if sprint { 3 } else { 4 });
        }
        for a in sends {
            if let Some(s) = self.link.session() {
                let r = s.entity_action(id, a);
                self.send_result(r);
            }
        }
    }

    /// `Entity.moveEntity`: a footstep each time the walked distance passes the next whole step.
    fn step_sound(&mut self) {
        let Some(p) = &self.game.player else { return };
        if self.game.gamemode == 3
            || self.game.abilities & 2 != 0
            || (self.input.sneaking() && p.on_ground)
        {
            return;
        }
        let at = [p.pos[0], p.pos[1], p.pos[2]];
        let id = self.game.store.get_block(
            at[0].floor() as i32,
            (at[1] - 0.2).floor() as i32,
            at[2].floor() as i32,
        ) >> 4;
        let d = [0, 1, 2].map(|a| p.pos[a] - self.game.prev_pos[a]);
        let dy = if id == 65 { d[1] } else { 0.0 };
        self.walked += (d[0] * d[0] + dy * dy + d[2] * d[2]).sqrt() as f32 * 0.6;
        if self.walked <= self.next_step as f32 || id == 0 {
            return;
        }
        self.next_step = self.walked as i32 + 1;
        self.swim_sound(d, at);
        if let (Some(s), false) = (&mut self.sound, (8..=11).contains(&id)) {
            s.step(id, at);
        }
    }

    fn limb_tick(&mut self) {
        let Some(p) = &self.game.player else { return };
        let dx = p.pos[0] - self.game.prev_pos[0];
        let dz = p.pos[2] - self.game.prev_pos[2];
        let l = &mut self.limbs;
        l.prev_amount = l.amount;
        let f = ((dx * dx + dz * dz).sqrt() as f32 * 4.0).min(1.0);
        l.amount += (f - l.amount) * 0.4;
        l.swing += l.amount;
        let dy = p.pos[1] - self.game.prev_pos[1];
        let alive = self.game.health > 0.0;
        self.view_bob.tick([dx, dy, dz], p.on_ground, alive);
    }

    /// `ItemRenderer.updateEquippedItem`: the hand lowers on a change of item and rises again.
    fn equip_tick(&mut self) {
        let held = self
            .game
            .inv
            .held_stack()
            .as_ref()
            .map(|s| (s.id, s.damage));
        let (cur, _, shown) = self.equip;
        let same = shown.map(|s| s.0) == held.map(|h| h.0);
        let target = if same { 1.0 } else { 0.0 };
        let next = cur + (target - cur).clamp(-0.4, 0.4);
        let shown = if next <= 0.0 && !same { held } else { shown };
        let shown = if same { held } else { shown };
        self.equip = (next, cur, shown);
    }

    pub(super) fn send_pose(&mut self) {
        let Some(p) = &self.game.player else { return };
        let [x, y, z] = p.pos;
        let (yaw, on_ground) = (p.yaw, p.on_ground);
        let pitch = self.game.pitch;
        if let Some(s) = self.link.session() {
            let r = s.send_position_look(x, y, z, yaw, pitch, on_ground);
            self.send_result(r);
        }
    }

    /// The entity under the crosshair (`EntityRenderer.getMouseOver`): a nearer block hides it.
    pub(super) fn entity_target(&self) -> Option<(i32, f64)> {
        let p = self.game.player.as_ref()?;
        let eye = self.game.eye(self.input.sneaking())?;
        let dir = look_vector(p.yaw, self.game.pitch);
        let reach = if self.game.creative { 6.0 } else { 3.0 };
        let block = self.game.target_dist(eye).map(|(_, d)| d);
        self.game.entities.pick(
            eye,
            dir,
            reach,
            self.game.entity_id,
            self.game.vehicle,
            block,
        )
    }

    /// One tick of block breaking: packets, local removal, hit shards and break sounds.
    fn dig(&mut self) {
        let presses = std::mem::take(&mut self.attack_presses);
        if self.use_ticks > 0 {
            return;
        }
        let attack = Attack {
            held: self.input.attack_held(),
            presses,
        };
        let entity = self.entity_target().map(|(id, _)| id);
        let (actions, effects) = self.game.dig_tick(attack, entity, self.input.sneaking());
        for action in actions {
            let sent = match action {
                DigAction::Dig { status, pos, face } => {
                    self.session_send(|s| s.send_dig(status as u8, pos, face))
                }
                DigAction::Swing => {
                    self.game.swing.start();
                    self.session_send(|s| s.send_swing())
                }
                DigAction::Attack(id) => self.session_send(|s| s.attack_entity(id)),
            };
            self.send_result(sent);
        }
        self.broken_effects();
        if attack.held && !self.game.creative {
            self.dig_ticks += 1;
            if self.dig_ticks.is_multiple_of(4) {
                self.hit_shard();
            }
        } else {
            self.dig_ticks = 0;
        }
        for effect in effects {
            if let (Effect::Upload(x, z), Some(r)) = (effect, &mut self.renderer) {
                if let Some(c) = self.game.store.get(x, z) {
                    r.upload_chunk(&View(c));
                }
            }
        }
    }

    fn broken_effects(&mut self) {
        for (pos, state) in std::mem::take(&mut self.game.broken) {
            self.particles.break_block(pos, state);
            if let Some(s) = &mut self.sound {
                s.dig(state >> 4, pos.map(|v| f64::from(v) + 0.5));
            }
        }
    }

    fn hit_shard(&mut self) {
        let Some(eye) = self.game.eye(self.input.sneaking()) else {
            return;
        };
        if let Some(h) = self.game.target(eye) {
            let at = |p: [i32; 3]| self.game.store.get_block(p[0], p[1], p[2]);
            let state = at(h.pos);
            if state != 0 {
                let bounds =
                    world::dig::selection_box(&at, h.pos).unwrap_or([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
                self.particles.dig_hit(h.pos, h.face, state, bounds);
                if let Some(s) = &mut self.sound {
                    s.dig_hit(state >> 4, h.pos.map(|v| f64::from(v) + 0.5));
                }
            }
        }
    }

    pub(super) fn session_send(
        &mut self,
        f: impl FnOnce(&mut ferrocraft_net_client::Session) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        match self.link.session() {
            Some(s) => f(s),
            None => Ok(()),
        }
    }

    /// Right button held: use or place again every `USE_DELAY` ticks; releasing ends an item use.
    fn use_tick(&mut self) {
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        let held = self.input.use_held();
        if self.use_ticks > 0 && !held {
            self.use_ticks = 0;
            let r = self.session_send(|s| s.release_use_item());
            self.send_result(r);
        } else if held && self.use_delay == 0 && self.use_ticks == 0 {
            self.use_item();
        }
    }

    /// `EntityRenderer.updateFovModifierHand` toward `AbstractClientPlayer.getFovModifier`.
    fn fov_tick(&mut self) {
        let mut f = 1.0_f32;
        if let Some(p) = &self.game.player {
            if self.game.abilities & 2 != 0 {
                f *= 1.1;
            }
            // `getEntityAttribute(movementSpeed).getAttributeValue()`: speed potions and the sprint boost.
            let speed = self.game.speed.value(p.sprinting);
            let walk = self.game.walk_speed;
            f = (f64::from(f) * ((speed / f64::from(walk) + 1.0) / 2.0)) as f32;
            if walk == 0.0 || !f.is_finite() {
                f = 1.0;
            }
            let bow = self
                .game
                .inv
                .held_stack()
                .as_ref()
                .is_some_and(|s| s.id == 261);
            if self.use_ticks > 0 && bow {
                let t = (72_000 - self.use_ticks) as f32 / 20.0;
                f *= 1.0 - if t > 1.0 { 1.0 } else { t * t } * 0.15;
            }
        }
        let cur = self.fov_mod.0;
        self.fov_mod = ((cur + (f - cur) * 0.5).clamp(0.1, 1.5), cur);
    }

    /// `Item.getMaxItemUseDuration` of an item whose right click starts a timed use
    /// (`setItemInUse`): food, potions and milk 32 ticks, bows and swords 72000; 0 for others.
    fn use_duration(&self, s: &ItemStack) -> i32 {
        let g = &self.game;
        let uses = match s.id {
            261 => {
                g.creative
                    || g.inv.player[5..]
                        .iter()
                        .any(|t| t.as_ref().is_some_and(|t| t.id == 262))
            }
            322 => g.abilities & 1 == 0,
            267 | 268 | 272 | 276 | 283 | 335 => true,
            373 => s.damage & 16384 == 0,
            id => FOOD.contains(&id) && g.abilities & 1 == 0 && g.food < 20,
        };
        if !uses {
            0
        } else if [261, 267, 268, 272, 276, 283].contains(&s.id) {
            72_000
        } else {
            32
        }
    }

    /// `Minecraft.rightClickMouse`: interact with the entity or the block under the crosshair,
    /// and when neither consumed the click use the held item in the air.
    fn use_item(&mut self) {
        if self.game.is_hitting_block() {
            return;
        }
        self.use_delay = USE_DELAY;
        let sneaking = self.input.sneaking();
        let (Some(p), Some(eye)) = (&self.game.player, self.game.eye(sneaking)) else {
            return;
        };
        let dir = look_vector(p.yaw, self.game.pitch);
        let held = self.game.inv.held_stack().clone();
        let gamemode = self.game.gamemode;
        if let Some((id, dist)) = self.entity_target() {
            let Some(e) = self.game.entities.get(id) else {
                return;
            };
            let at = [0, 1, 2].map(|a| (eye[a] + dir[a] * dist - e.pos[a]) as f32);
            let handled = gamemode != 3 && e.kind == Kind::Object(78);
            let r = self.session_send(|s| s.interact_entity_at(id, at));
            self.send_result(r);
            if handled {
                return;
            }
            let r = self.session_send(|s| s.interact_entity(id));
            self.send_result(r);
        } else if let Some((h, dist)) = self.game.target_dist(eye) {
            if self.use_block(h, dist, eye, dir, sneaking, &held) {
                return;
            }
        }
        if let Some(s) = &held {
            self.use_ticks = self.use_duration(s);
            let r = self.session_send(|s| s.place_block([-1, -1, -1], 255, &held, [0, 0, 0]));
            self.send_result(r);
        }
    }

    /// `PlayerControllerMP.onPlayerRightClick` on the block under the crosshair. True when the
    /// click is done: the block or item consumed it, or there is no item to use in the air.
    fn use_block(
        &mut self,
        h: world::dig::Hit,
        dist: f64,
        eye: [f64; 3],
        dir: [f64; 3],
        sneaking: bool,
        held: &Option<ItemStack>,
    ) -> bool {
        let gamemode = self.game.gamemode;
        let held_id = held.as_ref().map(|s| s.id);
        let cursor = [0, 1, 2].map(|a| (eye[a] + dir[a] * dist - f64::from(h.pos[a])) as f32);
        let click = Click {
            pos: h.pos,
            face: h.face,
            cursor,
        };
        let [x, y, z] = h.pos;
        let (state, above) = (
            self.game.store.get_block(x, y, z),
            self.game.store.get_block(x, y + 1, z),
        );
        let activated =
            gamemode != 3 && (!sneaking || held.is_none()) && activates(state, held_id, gamemode);
        let Some(outcome) = self.client_outcome(held, click, activated) else {
            return false;
        };
        let packet = self
            .session_send(|s| s.place_block(h.pos, h.face, held, cursor.map(|c| (c * 16.0) as u8)));
        self.send_result(packet);
        let handled = gamemode == 3
            || activated
            || outcome != Outcome::Refused
            || held_id.is_some_and(|id| item_used_on(id, state, above, h.face, gamemode));
        if let Outcome::Placed(blocks) = outcome {
            self.placed(&blocks);
        }
        if handled {
            self.game.swing.start();
            let r = self.session_send(|s| s.send_swing());
            self.send_result(r);
        }
        handled || held.is_none()
    }

    /// What the client does with a click before it sends C08: `None` when
    /// `canPlaceBlockOnSide` refuses and nothing is sent, else the local placement.
    fn client_outcome(
        &self,
        held: &Option<ItemStack>,
        click: Click,
        activated: bool,
    ) -> Option<Outcome> {
        let gamemode = self.game.gamemode;
        let Some(s) = held.as_ref().filter(|_| gamemode != 3 && !activated) else {
            return Some(Outcome::Refused);
        };
        let at = |p: [i32; 3]| self.game.store.get_block(p[0], p[1], p[2]);
        let bodies = self.game.bodies();
        let item_block = (1..=255).contains(&s.id);
        if item_block && !place::can_place_on_side(&at, s.id, s.damage, click, &bodies) {
            return None;
        }
        if gamemode == 2 {
            return Some(Outcome::Refused);
        }
        let yaw = self.game.player.as_ref().map_or(0.0, |p| p.yaw);
        Some(place::use_item(&at, s.id, s.damage, click, yaw, &bodies))
    }

    /// The client half of `ItemBlock.onItemUse`: set the blocks, play the place sound, use up
    /// the item. The server's `BlockChange` corrects any difference.
    fn placed(&mut self, blocks: &[([i32; 3], u16)]) {
        for effect in self.game.place_blocks(blocks) {
            if let (Effect::Upload(x, z), Some(r)) = (effect, &mut self.renderer) {
                if let Some(c) = self.game.store.get(x, z) {
                    r.upload_chunk(&View(c));
                }
            }
        }
        if let (Some(s), Some(&(pos, state))) = (&mut self.sound, blocks.first()) {
            s.dig(state >> 4, pos.map(|v| f64::from(v) + 0.5));
        }
    }

    /// `Minecraft.middleClickMouse`: select the targeted block's item on the hotbar; creative
    /// puts it there when missing.
    fn pick_block(&mut self) {
        let Some(eye) = self.game.eye(self.input.sneaking()) else {
            return;
        };
        let Some(h) = self.game.target(eye) else {
            return;
        };
        let state = self.game.store.get_block(h.pos[0], h.pos[1], h.pos[2]);
        let (block, meta) = (state >> 4, state & 15);
        let id = block as i16;
        let item = world::inventory::items::item(id);
        if item.id != id {
            return;
        }
        let damage = pick_damage(block, meta, item.subtypes);
        let hotbar = &self.game.inv.player[36..45];
        let found = hotbar.iter().position(|s| {
            s.as_ref()
                .is_some_and(|s| s.id == id && (!item.subtypes || s.damage == damage))
        });
        if let Some(i) = found {
            self.select_slot(i as u8);
        } else if self.game.creative {
            if let Some(i) = hotbar.iter().position(Option::is_none) {
                self.select_slot(i as u8);
            }
            let stack = Some(ItemStack {
                id,
                count: 1,
                damage,
                nbt: None,
            });
            let slot = 36 + usize::from(self.game.inv.held);
            self.game.inv.player[slot] = stack.clone();
            self.use_ticks = 0;
            let r = self.session_send(|s| s.creative_slot(slot as i16, &stack));
            self.send_result(r);
        }
    }

    pub(super) fn sync_cursor(&mut self) {
        let screen = self.modal();
        if screen != self.input_modal {
            self.input_modal = screen;
            self.input.set_screen_open(screen);
        }
        let want = self.input.wants_cursor_grab();
        if want == self.grabbed {
            return;
        }
        let Some(w) = &self.window else { return };
        self.grabbed = apply_cursor_grab(w, want);
    }

    pub(super) fn handle_actions(&mut self) {
        let in_game = self.game.player.is_some() && self.ui.in_world;
        for action in self.input.take_actions() {
            match action {
                Action::ToggleFullscreen => self.toggle_fullscreen(),
                Action::Screenshot => {
                    if let Some(r) = &mut self.renderer {
                        r.request_screenshot();
                    }
                }
                other if in_game => self.game_action(other),
                _ => {}
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.ui.settings.fullscreen = !self.ui.settings.fullscreen;
        if let Err(e) = self.ui.settings.save() {
            crate::log::line(&format!("Failed to save options: {e}"));
        }
        self.apply_settings();
    }

    /// `RenderGlobal.loadRenderers`: rebuild every chunk mesh.
    fn reload_chunks(&mut self) {
        let Some(r) = &mut self.renderer else { return };
        r.clear_chunks();
        let at: Vec<_> = self.game.store.positions().collect();
        for (x, z) in at {
            if let Some(c) = self.game.store.get(x, z) {
                r.upload_chunk(&View(c));
            }
        }
    }

    fn game_action(&mut self, action: Action) {
        match action {
            Action::Attack => self.attack_presses += 1,
            Action::Use => {
                if self.use_ticks == 0 {
                    self.use_item();
                }
            }
            Action::PickBlock => self.pick_block(),
            Action::Hotbar(n) => self.select_slot(n),
            Action::HotbarScroll(step) => self.select_slot(scroll_slot(self.game.inv.held, step)),
            Action::Inventory => self.open_inventory(),
            Action::Drop { whole_stack } => {
                let r = self.session_send(|s| s.drop_item(whole_stack));
                self.send_result(r);
            }
            Action::Pause => self.open_pause(),
            Action::CloseScreen => self.close_container(),
            other => self.view_action(other),
        }
    }

    /// Actions that change only what the client shows.
    fn view_action(&mut self, action: Action) {
        match action {
            Action::ToggleDebug => self.show_debug = !self.show_debug,
            Action::ToggleHud => self.hide_hud = !self.hide_hud,
            Action::TogglePerspective => self.perspective = (self.perspective + 1) % 3,
            Action::Debug(DebugCombo::ClearChat) => {
                self.hud.chat.clear();
                self.input.clear_chat_history();
            }
            Action::ChatScroll(step) => {
                let open = self.input.chat_input().is_some();
                self.hud.chat.scroll(step * 7, open);
            }
            Action::Debug(DebugCombo::AdvancedTooltips) => {
                let on = !self.ui.settings.advanced_item_tooltips;
                self.ui.settings.advanced_item_tooltips = on;
                if let Err(e) = self.ui.settings.save() {
                    crate::log::line(&format!("Failed to save options: {e}"));
                }
                self.debug_chat(if on {
                    "Advanced tooltips: shown"
                } else {
                    "Advanced tooltips: hidden"
                });
            }
            Action::Debug(DebugCombo::Help) => {
                self.debug_chat("Key bindings:");
                for line in [
                    "F3 + A = Reload chunks",
                    "F3 + D = Clear chat",
                    "F3 + H = Advanced tooltips",
                    "F3 + P = Pause on lost focus",
                    "F3 + Q = Show this list",
                ] {
                    self.chat_line(vec![ferrocraft_net_client::Styled {
                        format: String::new(),
                        text: line.into(),
                    }]);
                }
            }
            Action::Debug(DebugCombo::ReloadChunks) => {
                self.reload_chunks();
                self.debug_chat("Reloading all chunks");
            }
            _ => {}
        }
    }

    pub(super) fn chat_line(&mut self, runs: Vec<ferrocraft_net_client::Styled>) {
        let m = gui::Metrics(self.renderer.as_ref().and_then(render::Renderer::font));
        let open = self.input.chat_input().is_some();
        self.hud
            .chat
            .print(runs, 0, self.hud.update_counter, open, m);
    }

    /// `Minecraft.debugFeedbackTranslated`: a `[Debug]:` line in the chat.
    fn debug_chat(&mut self, text: &str) {
        let styled = |format: &str, text: String| ferrocraft_net_client::Styled {
            format: format.into(),
            text,
        };
        self.chat_line(vec![
            styled("\u{a7}e\u{a7}l", "[Debug]:".into()),
            styled("", format!(" {text}")),
        ]);
    }

    fn open_pause(&mut self) {
        if !self.ui.is_open() && self.container.is_none() {
            let m = metrics!(self);
            let q = self.ui.open(&m, boxed(Pause::default()));
            self.run_effects(q);
        }
    }

    fn select_slot(&mut self, n: u8) {
        if n == self.game.inv.held {
            return;
        }
        self.game.inv.held = n;
        self.synced_held = n;
        self.use_ticks = 0;
        let r = self.session_send(|s| s.held_item_change(n));
        self.send_result(r);
    }

    fn open_inventory(&mut self) {
        if self.container.is_none() && !self.ui.is_open() {
            self.container = Some(ContainerScreen::new(
                &self.game.inv,
                self.ui.width,
                self.ui.height,
            ));
        }
    }

    /// Close the open container: tell the server, drop the cursor stack locally.
    pub(super) fn close_container(&mut self) {
        if self.container.take().is_some() {
            let id = self.game.inv.window_id();
            self.game.inv.close_window();
            let r = self.session_send(|s| s.close_window(id));
            self.send_result(r);
        }
    }
}

fn pick_damage(block: u16, meta: u16, subtypes: bool) -> i16 {
    let mask = match block {
        17 | 18 | 161 | 162 => 3,
        44 | 126 => 7,
        _ => 15,
    };
    if subtypes {
        (meta & mask) as i16
    } else {
        0
    }
}

/// `Entity.createRunningParticles`: the block state under the feet, or `None` for air or water.
fn sprint_dust_state(
    pos: [f64; 3],
    height: f32,
    block: impl Fn(i32, i32, i32) -> u16,
) -> Option<u16> {
    let state = block(
        pos[0].floor() as i32,
        (pos[1] - 0.2).floor() as i32,
        pos[2].floor() as i32,
    );
    // Air has render type -1 and makes no dust.
    (state >> 4 != 0 && !world::motion_sound::in_water(pos, f64::from(height), block))
        .then_some(state)
}
#[cfg(test)]
mod tests {
    use super::{activates, item_used_on, sprint_dust_state};

    #[test]
    fn sprint_dust_needs_a_solid_block_and_no_water() {
        let grass = 2 << 4;
        let water = 9 << 4;
        let pos = [0.5, 64.0, 0.5];
        let floor = |x: i32, y: i32, z: i32| {
            if y == 63 && (x, z) == (0, 0) {
                grass
            } else {
                0
            }
        };
        assert_eq!(sprint_dust_state(pos, 1.8, floor), Some(grass));
        assert_eq!(sprint_dust_state(pos, 1.8, |_, _, _| 0), None);
        let flooded = |x: i32, y: i32, z: i32| if y >= 64 { water } else { floor(x, y, z) };
        assert_eq!(sprint_dust_state(pos, 1.8, flooded), None);
    }

    #[test]
    fn right_click_tables_follow_vanilla() {
        assert!(activates(54 << 4, None, 0));
        assert!(!activates(1 << 4, None, 0));
        assert!(!activates(93 << 4, None, 2));
        assert!(activates(46 << 4, Some(259), 0) && !activates(46 << 4, None, 0));
        assert!(item_used_on(259, 1 << 4, 0, 1, 0) && !item_used_on(259, 1 << 4, 0, 1, 2));
        assert!(item_used_on(295, 60 << 4, 0, 1, 0) && !item_used_on(295, 2 << 4, 0, 1, 0));
        assert!(!item_used_on(325, 1 << 4, 0, 1, 0));
    }
}
