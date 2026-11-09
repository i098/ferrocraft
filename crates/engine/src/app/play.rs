//! The 20 TPS step and the player's actions: movement packets, digging, attacking, using items.

use super::{boxed, App, View, MAX_TICKS_PER_FRAME, TICK};
use crate::game::{Attack, Effect, EYE_HEIGHT};
use gui::inventory::ContainerScreen;
use gui::screens::Pause;
use input_audio::{apply_cursor_grab, scroll_slot, Action, DebugCombo};
use std::time::Instant;
use winit::window::Fullscreen;
use world::dig::{look_vector, Action as DigAction};

/// `rightClickDelayTimer` after a use.
const USE_DELAY: i32 = 4;

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
        if let Some(line) = self.input.take_chat_submit() {
            self.hud.chat.add_sent(&line);
            if let Some(s) = self.link.session() {
                let r = s.send_chat(&line);
                self.send_result(r);
            }
        }
        self.hud.tick();
        let m = metrics!(self);
        let q = self.ui.tick(&m);
        self.run_effects(q);
        if let Some(c) = &mut self.container {
            c.tick();
        }
        let playing = self.game.player.is_some() && !self.ui.is_open() && self.container.is_none();
        if playing {
            self.dig();
            self.use_tick();
        }
        self.limb_tick();
        let input = if playing {
            self.input.movement()
        } else {
            world::Input::default()
        };
        self.game.tick(input);
        self.equip_tick();
        self.send_state();
        self.send_pose();
        let at = self.game.player.as_ref().map(|p| p.pos);
        if let (Some(s), Some(at)) = (&mut self.sound, at) {
            s.set_listener(at);
            s.tick(self.ui.in_world);
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

    fn limb_tick(&mut self) {
        let Some(p) = &self.game.player else { return };
        let dx = p.pos[0] - self.game.prev_pos[0];
        let dz = p.pos[2] - self.game.prev_pos[2];
        let l = &mut self.limbs;
        l.prev_amount = l.amount;
        let f = ((dx * dx + dz * dz).sqrt() as f32 * 4.0).min(1.0);
        l.amount += (f - l.amount) * 0.4;
        l.swing += l.amount;
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

    /// The entity under the crosshair (`EntityRenderer.getMouseOver`, entity part).
    // shortcut: a block in front of the entity does not hide it; the dig `Hit` has no distance.
    pub(super) fn entity_target(&self) -> Option<i32> {
        let p = self.game.player.as_ref()?;
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let dir = look_vector(p.yaw, self.game.pitch);
        let reach = if self.game.creative { 6.0 } else { 3.0 };
        self.game
            .entities
            .pick(eye, dir, reach, self.game.entity_id, None)
    }

    fn attack(&mut self) {
        match self.entity_target() {
            Some(id) => {
                if let Some(s) = self.link.session() {
                    let r = s.attack_entity(id);
                    self.send_result(r);
                }
            }
            None => self.attack_presses += 1,
        }
    }

    /// One tick of block breaking: packets, local removal, hit shards and break sounds.
    fn dig(&mut self) {
        let attack = Attack {
            held: self.input.attack_held(),
            presses: std::mem::take(&mut self.attack_presses),
        };
        let blocked = self.entity_target().is_some();
        let (actions, effects) = self.game.dig_tick(attack, blocked);
        for action in actions {
            let sent = match action {
                DigAction::Dig { status, pos, face } => {
                    self.session_send(|s| s.send_dig(status as u8, pos, face))
                }
                DigAction::Swing => self.session_send(|s| s.send_swing()),
            };
            self.send_result(sent);
        }
        for (pos, state) in std::mem::take(&mut self.game.broken) {
            self.particles.break_block(pos, state);
            if let Some(s) = &mut self.sound {
                s.dig(state, pos.map(|v| f64::from(v) + 0.5));
            }
        }
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

    fn hit_shard(&mut self) {
        let Some(p) = &self.game.player else { return };
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        if let Some(h) = self.game.target(eye) {
            let state = self.game.store.get_block(h.pos[0], h.pos[1], h.pos[2]);
            if state != 0 {
                self.particles.dig_hit(h.pos, h.face, state);
            }
        }
    }

    fn session_send(
        &mut self,
        f: impl FnOnce(&mut ferrocraft_net_client::Session) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        match self.link.session() {
            Some(s) => f(s),
            None => Ok(()),
        }
    }

    /// Right button held: use or place every `USE_DELAY` ticks.
    fn use_tick(&mut self) {
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        if !self.input.use_held() && self.using_item {
            self.using_item = false;
            let r = self.session_send(|s| s.release_use_item());
            self.send_result(r);
        }
    }

    /// Right click: interact with the entity, place on the targeted block, or use in the air.
    fn use_item(&mut self) {
        self.game.swing.start();
        self.use_delay = USE_DELAY;
        if let Some(id) = self.entity_target() {
            let r = self.session_send(|s| s.interact_entity(id));
            self.send_result(r);
            return;
        }
        let Some(p) = &self.game.player else { return };
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let held = self.game.inv.held_stack().clone();
        let hit = self.game.target(eye);
        let r = match hit {
            Some(h) if self.game.gamemode != 3 => {
                self.session_send(|s| s.place_block(h.pos, h.face, &held, [8, 8, 8]))
            }
            _ if held.is_some() => {
                self.using_item = true;
                self.session_send(|s| s.place_block([-1, -1, -1], 255, &held, [0, 0, 0]))
            }
            _ => Ok(()),
        };
        self.send_result(r);
        let r = self.session_send(|s| s.send_swing());
        self.send_result(r);
    }

    pub(super) fn sync_cursor(&mut self) {
        let screen = self.ui.is_open() || self.container.is_some();
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
            if action == Action::ToggleFullscreen {
                self.toggle_fullscreen();
            } else if in_game {
                self.game_action(action);
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        if let Some(w) = &self.window {
            let on = w.fullscreen().is_none();
            w.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        }
    }

    fn game_action(&mut self, action: Action) {
        match action {
            Action::Attack => {
                self.game.swing.start();
                self.attack();
            }
            Action::Use => {
                if self.use_delay <= 0 {
                    self.use_item();
                }
            }
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
            Action::Debug(DebugCombo::ClearChat) => self.hud.chat.clear(),
            Action::ChatScroll(step) => {
                let open = self.input.chat_input().is_some();
                self.hud.chat.scroll(step * 7, open);
            }
            _ => {}
        }
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
            if id != 0 {
                let r = self.session_send(|s| s.close_window(id));
                self.send_result(r);
            }
        }
    }
}
