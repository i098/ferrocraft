//! The 20 TPS step and the player's actions: movement packets, digging, attacking, using items.

use super::{boxed, App, View, MAX_TICKS_PER_FRAME, TICK};
use crate::game::{Attack, Effect, EYE_HEIGHT};
use gui::inventory::ContainerScreen;
use gui::screens::Pause;
use input_audio::{apply_cursor_grab, scroll_slot, Action, DebugCombo};
use std::time::Instant;
use world::dig::{look_vector, Action as DigAction};
use world::inventory::ItemStack;

/// `rightClickDelayTimer` after a use.
const USE_DELAY: i32 = 4;

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
        self.hud.tick();
        if self.game.player.is_some() {
            self.hud.update_held(self.game.inv.held_stack());
        }
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
        self.step_sound();
        self.particles
            .tick(|x, y, z| render::is_solid(self.game.store.get_block(x, y, z)));
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
    pub(super) fn entity_target(&self) -> Option<i32> {
        let p = self.game.player.as_ref()?;
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let dir = look_vector(p.yaw, self.game.pitch);
        let reach = if self.game.creative { 6.0 } else { 3.0 };
        let block = self.game.target_dist(eye).map(|(_, d)| d);
        self.game
            .entities
            .pick(eye, dir, reach, self.game.entity_id, block)
    }

    /// One tick of block breaking: packets, local removal, hit shards and break sounds.
    fn dig(&mut self) {
        let attack = Attack {
            held: self.input.attack_held(),
            presses: std::mem::take(&mut self.attack_presses),
        };
        let entity = self.entity_target();
        let (actions, effects) = self.game.dig_tick(attack, entity);
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
        for (pos, state) in std::mem::take(&mut self.game.broken) {
            self.particles.break_block(pos, state);
            if let Some(s) = &mut self.sound {
                s.dig(state >> 4, pos.map(|v| f64::from(v) + 0.5));
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
        if !self.input.use_held() {
            if self.using_item {
                self.using_item = false;
                let r = self.session_send(|s| s.release_use_item());
                self.send_result(r);
            }
        } else if self.use_delay == 0 && !self.using_item {
            self.use_item();
        }
    }

    /// `ItemFood`, `ItemBow`, `ItemSword`, `ItemPotion` and the milk bucket start a timed use
    /// (`setItemInUse`), which the client then ends with a release packet.
    fn starts_item_use(&self, s: &ItemStack) -> bool {
        const FOOD: [i16; 26] = [
            260, 282, 297, 319, 320, 349, 350, 357, 360, 363, 364, 365, 366, 367, 375, 391, 392,
            393, 394, 396, 400, 411, 412, 413, 423, 424,
        ];
        let g = &self.game;
        match s.id {
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
        }
    }

    /// Right click: interact with the entity, place on the targeted block, or use in the air.
    fn use_item(&mut self) {
        if self.game.is_hitting_block() {
            return;
        }
        self.use_delay = USE_DELAY;
        if let Some(id) = self.entity_target() {
            let r = self.session_send(|s| s.interact_entity(id));
            self.send_result(r);
            return;
        }
        let Some(p) = &self.game.player else { return };
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let dir = look_vector(p.yaw, self.game.pitch);
        let held = self.game.inv.held_stack().clone();
        let uses = held.as_ref().is_some_and(|s| self.starts_item_use(s));
        if let Some((h, dist)) = self
            .game
            .target_dist(eye)
            .filter(|_| self.game.gamemode != 3)
        {
            let cursor =
                [0, 1, 2].map(|a| ((eye[a] + dir[a] * dist - f64::from(h.pos[a])) * 16.0) as u8);
            let r = self.session_send(|s| s.place_block(h.pos, h.face, &held, cursor));
            self.send_result(r);
            if !uses {
                self.game.swing.start();
                let r = self.session_send(|s| s.send_swing());
                self.send_result(r);
                return;
            }
        }
        if held.is_some() {
            self.using_item = uses;
            let r = self.session_send(|s| s.place_block([-1, -1, -1], 255, &held, [0, 0, 0]));
            self.send_result(r);
        }
    }

    /// `Minecraft.middleClickMouse`: select the targeted block's item on the hotbar; creative
    /// puts it there when missing.
    fn pick_block(&mut self) {
        let Some(p) = &self.game.player else { return };
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
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
        let mask = match block {
            17 | 18 | 161 | 162 => 3,
            44 | 126 => 7,
            _ => 15,
        };
        let damage = if item.subtypes {
            (meta & mask) as i16
        } else {
            0
        };
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
            let r = self.session_send(|s| s.creative_slot(slot as i16, &stack));
            self.send_result(r);
        }
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
            Action::Use => self.use_item(),
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
