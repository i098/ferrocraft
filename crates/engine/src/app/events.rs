//! Server events and menu effects: what the network and the screens ask the client to do.

use super::movement::SentMovement;
use super::{boxed, App, View};
use crate::game::Game;
use crate::game::{eye_height, Effect};
use crate::game_state::Reaction;
use crate::link::Link;
use ferrocraft_net_client::{split_address, NetEvent, PackStatus, PlayerListEvent, TileEvent};
use gui::inventory::ContainerScreen;
use gui::scoreboard::TeamInfo;
use gui::screens::{Connecting, Death, EditSign, Effect as UiEffect};
use gui::tablist::TabEntry;
use input_audio::sound::blocks::wire_pitch;
use render::{Banner, Renderer, Skull};
use std::collections::HashMap;
use world::entities::{is_baby, EntityEvent, Kind};
use world::entity_sound;

/// A change to one tab list entry.
type TabUpdate = Box<dyn FnOnce(&mut TabEntry)>;
type Dirty = HashMap<(i32, i32), Effect>;

impl App {
    /// The title screen, or the login screen when `--server` names a server.
    pub(super) fn start_screen(&mut self) {
        self.resize_screens();
        let m = metrics!(self);
        let mut q = self.ui.close(&m);
        if let Some(addr) = self.args.server.clone() {
            let (host, port) = split_address(&addr);
            let title = self.ui.title(&m);
            q.extend(self.ui.open(&m, boxed(Connecting::new(title, host, port))));
        }
        self.run_effects(q);
    }

    pub(super) fn resize_screens(&mut self) {
        let Some(r) = &self.renderer else { return };
        let px = r.size();
        let m = metrics!(self);
        let q = self.ui.resize(&m, px);
        let (w, h) = (self.ui.width, self.ui.height);
        if let Some(c) = &mut self.container {
            c.resize(w, h);
        }
        self.run_effects(q);
    }

    pub(super) fn run_effects(&mut self, q: Vec<UiEffect>) {
        for effect in q {
            self.run_effect(effect);
        }
    }

    fn run_effect(&mut self, effect: UiEffect) {
        match effect {
            UiEffect::Sound(name) => self.play_ui_sound(name),
            UiEffect::Connect { host, port } => self.connect(&host, port),
            UiEffect::CancelConnect => self.link = Link::Offline,
            UiEffect::Disconnect => self.leave_world(),
            UiEffect::Respawn => self.respawn(),
            UiEffect::RequestStats => {
                let r = self.link.session().map(|s| s.request_stats());
                self.send_result(r.unwrap_or(Ok(())));
            }
            UiEffect::SignText { pos, lines } => self.send_sign(pos, &lines),
            UiEffect::Quit => self.quit = true,
            UiEffect::SettingsSaved => {
                self.apply_settings();
                self.send_client_settings();
            }
            UiEffect::LanguageChanged => self.change_language(),
            UiEffect::OpenUrl(url) => open_url(&url),
            UiEffect::SuperSecret => self.super_secret(),
            UiEffect::ResourcesChanged => self.refresh_resources(),
            UiEffect::ServerPackAnswer {
                url,
                hash,
                accepted,
            } => self.answer_server_pack(&url, &hash, accepted),
        }
    }

    fn play_ui_sound(&mut self, name: &'static str) {
        if let Some(sound) = &mut self.sound {
            sound.play_ui(name);
        }
    }

    fn super_secret(&mut self) {
        if let Some(s) = &mut self.sound {
            s.play_super_secret();
        }
        self.next_shader();
    }

    /// `NetHandlerPlayClient.handlePlayerMove`: the reply `C06` carries the entity's position and
    /// angles with `onGround = false`, whatever the entity's own flag.
    fn send_teleport_echo(&mut self) {
        let Some(p) = &self.game.player else { return };
        let ([x, y, z], yaw, pitch) = (p.pos, p.yaw, self.game.pitch);
        if let Some(s) = self.link.session() {
            let r = s.send_position_look(x, y, z, yaw, pitch, false);
            self.send_result(r);
        }
    }

    fn respawn(&mut self) {
        let r = self.link.session().map(|s| s.respawn());
        self.send_result(r.unwrap_or(Ok(())));
    }

    /// `LocalClientPlayerEntity.doCloseMenu` and the openers: `displayGuiScreen` replaces chat and any menu.
    fn close_screens(&mut self) {
        self.input.close_chat();
        if self.ui.is_open() {
            let m = metrics!(self);
            let q = self.ui.close(&m);
            self.run_effects(q);
        }
    }

    fn open_sign_editor(&mut self, pos: [i32; 3]) {
        let state = self.game.store.get_block(pos[0], pos[1], pos[2]);
        let m = metrics!(self);
        let q = self.ui.open(&m, boxed(EditSign::new(pos, state)));
        self.run_effects(q);
    }

    fn send_sign(&mut self, pos: [i32; 3], lines: &[String; 4]) {
        if let Some(s) = self.link.session() {
            let r = s.update_sign(pos, lines);
            self.send_result(r);
        }
    }

    fn connect(&mut self, host: &str, port: u16) {
        self.game = Game::new();
        self.ui.stats.clear();
        if let Some(r) = &mut self.renderer {
            r.clear_maps();
        }
        let addr = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let brand = self
            .mods
            .as_ref()
            .and_then(|m| m.client_brand())
            .unwrap_or_else(|| "vanilla".into());
        self.server.clone_from(&addr);
        self.pack_mode = self.saved_pack_mode();
        self.link = Link::connect(
            addr,
            self.args.username.clone(),
            self.args.auth.clone(),
            brand,
        );
    }

    /// `Minecraft.refreshResources` after a resource pack change: a new renderer and sound system over the new
    /// packs, then the chunks, texts and screens again.
    // shortcut: mod-registered textures still come from before the change; reload them when packs need it.
    pub(super) fn refresh_resources(&mut self) {
        let Some(w) = self.window.clone() else { return };
        // Drop the old renderer first: a window holds one surface at a time.
        let maps = self.renderer.as_mut().map(|r| r.take_maps());
        self.renderer = None;
        self.renderer = Some(Renderer::new(w, self.asset_source()));
        self.skins.clear_tab();
        if let (Some(r), Some(maps)) = (&mut self.renderer, maps) {
            r.restore_maps(maps);
        }
        self.load_assets();
        self.load_sound();
        self.apply_settings();
        self.reload_chunks();
        // `onResourceManagerReload`: the cycle's effect, else the camera entity's.
        match render::SHADERS.get(self.shader_index) {
            Some(name) => self.load_shader(name),
            None => self.entity_shader(self.game.camera),
        }
        self.resize_screens();
    }

    /// `refreshResources`, then the font flag and a rescale, which relabels the screen.
    fn change_language(&mut self) {
        self.load_texts();
        self.apply_settings();
        self.send_client_settings();
        self.resize_screens();
    }

    /// Back to the menus: forget the world and the connection.
    fn leave_world(&mut self) {
        self.mod_leave_world();
        self.link = Link::Offline;
        self.game = Game::new();
        self.hud = gui::Hud::default();
        self.container = None;
        self.particles.clear();
        self.ui.in_world = false;
        self.ui.dead = None;
        self.ui.stats.clear();
        self.ui.difficulty = None;
        self.awaiting_terrain = false;
        self.pending_pack = None;
        self.perspective = 0;
        self.reset_sent_state();
        self.end_use();
        // `Minecraft.loadWorld(null)` drops the server pack and reloads.
        if self.server_pack.take().is_some() {
            self.refresh_resources();
        }
        if let Some(r) = &mut self.renderer {
            r.clear_chunks();
            // `runTick` without a world: `stopUseShader`.
            let _ = r.set_post(None);
        }
        self.shader_index = render::SHADERS.len();
    }

    /// `GameRenderer.loadShader(shaders/post/<name>.json)`; a failed load also ends the cycle.
    fn load_shader(&mut self, name: &str) {
        let Some(r) = &mut self.renderer else { return };
        if let Err(e) = r.set_post(Some(&format!("shaders/post/{name}.json"))) {
            crate::log::line(&format!("failed to load shader {name}: {e}"));
            self.shader_index = render::SHADERS.len();
        }
    }

    /// `GameRenderer.loadEntityShader`: drop the effect, then a creeper, spider (cave spiders
    /// too) or enderman camera sees through its own.
    pub(super) fn entity_shader(&mut self, camera: Option<i32>) {
        if let Some(r) = &mut self.renderer {
            let _ = r.set_post(None);
        }
        let kind = camera
            .and_then(|id| self.game.entities.get(id))
            .map(|e| e.kind);
        let name = match kind {
            Some(Kind::Mob(50)) => "creeper",
            Some(Kind::Mob(52 | 59)) => "spider",
            Some(Kind::Mob(58)) => "invert",
            _ => return,
        };
        self.load_shader(name);
    }

    /// "Super Secret Settings...", `GameRenderer.activateNextShader`: the next effect of the
    /// cycle, then none, while a player is the camera.
    fn next_shader(&mut self) {
        let player = match self.game.camera {
            Some(id) => self
                .game
                .entities
                .get(id)
                .is_some_and(|e| e.kind == Kind::Player),
            None => self.game.player.is_some(),
        };
        if !player {
            return;
        }
        if let Some(r) = &mut self.renderer {
            let _ = r.set_post(None);
        }
        self.shader_index = (self.shader_index + 1) % (render::SHADERS.len() + 1);
        if let Some(name) = render::SHADERS.get(self.shader_index) {
            self.load_shader(name);
        }
    }

    /// The first position packet closes `GuiDownloadingTerrain`; a held pack prompt opens then.
    fn finish_terrain_wait(&mut self) {
        if self.awaiting_terrain && self.game.player.is_some() {
            self.awaiting_terrain = false;
            let m = metrics!(self);
            let q = self.ui.close(&m);
            self.run_effects(q);
            if let Some((url, hash)) = self.pending_pack.take() {
                self.offer_server_pack(&url, &hash);
            }
        }
    }

    pub(super) fn pump_network(&mut self) {
        if let Some(e) = self.link.poll_login() {
            let m = metrics!(self);
            let q = self.ui.connect_failed(&m, &e.to_string());
            self.run_effects(q);
        }
        let events = self.link.session().map(|s| s.poll()).unwrap_or_default();
        let mut dirty = Dirty::new();
        for ev in events {
            self.on_event(ev, &mut dirty);
        }
        if std::mem::take(&mut self.game.teleported) {
            self.send_teleport_echo();
        }
        if let Some(r) = &mut self.renderer {
            upload(r, &self.game, dirty);
        }
        self.finish_terrain_wait();
        if let Some(reason) = self.game.disconnected.take() {
            self.leave_world();
            let m = metrics!(self);
            let q = self.ui.disconnected(&m, &reason);
            self.run_effects(q);
        }
    }

    fn on_event(&mut self, ev: NetEvent, dirty: &mut Dirty) {
        self.mod_net_event(&ev);
        self.note_session_state(&ev);
        self.entity_sound(&ev);
        if let NetEvent::Map { id, area, data } = &ev {
            if let Some(r) = &mut self.renderer {
                r.map_update(*id, *area, data);
            }
            return;
        }
        if let NetEvent::MapIcons { id, icons } = &ev {
            if let Some(r) = &mut self.renderer {
                r.map_icons(*id, icons);
            }
            return;
        }
        if let NetEvent::Statistics(values) = &ev {
            self.ui.stats_arrived(values);
            return;
        }
        let ev = match self.world_event(ev, dirty) {
            Ok(()) => return,
            Err(ev) => ev,
        };
        let ev = match self.hud_event(ev) {
            Ok(()) => return,
            Err(ev) => ev,
        };
        let health = matches!(ev, NetEvent::Health { .. });
        if let Some(e) = self.game.apply(ev) {
            self.game_effect(e, dirty);
        }
        if health {
            self.ui.dead = (self.game.health <= 0.0).then_some(self.game.score);
        }
    }

    /// Entity launch, hurt, death and pickup sounds, before `Game` applies the event.
    fn entity_sound(&mut self, ev: &NetEvent) {
        let NetEvent::Entity(ev) = ev else { return };
        // `EntityFireworkRocket.onUpdate` at age 0: the launch sound, 3.0 loud, at the rocket.
        if let EntityEvent::SpawnObject { kind: 76, pos, .. } = ev {
            if let Some(s) = &mut self.sound {
                s.play(
                    "fireworks.launch",
                    Some(pos.map(|v| f64::from(v) / 32.0)),
                    3.0,
                    1.0,
                );
            }
            return;
        }
        self.entity_status_sound(ev);
    }

    /// The own player is not in `entities`.
    fn entity_status_sound(&mut self, ev: &EntityEvent) {
        let (id, status) = match ev {
            EntityEvent::Status {
                id,
                status: s @ (2 | 3),
            } => (*id, Some(*s)),
            EntityEvent::Pickup { id, .. } => (*id, None),
            _ => return,
        };
        let own = id == self.game.entity_id;
        let found = self.game.entities.get(id);
        let at = match (found, &self.game.player) {
            (Some(e), _) => e.pos,
            (None, Some(p)) if own => p.pos,
            _ => return,
        };
        let Some(s) = &mut self.sound else { return };
        let Some(status) = status else {
            let orb = found.is_some_and(|e| e.kind == Kind::ExperienceOrb);
            let (name, jitter) = (if orb { "random.orb" } else { "random.pop" }, s.rand_diff());
            return s.play(name, Some(at), 0.2, (jitter * 0.7 + 1.0) * 2.0);
        };
        let kind = found.map_or(Kind::Player, |e| e.kind);
        let meta = found.map(|e| e.meta).unwrap_or_default();
        let water = |at: [f64; 3]| {
            let state = self.game.store.get_block(
                at[0].floor() as i32,
                at[1].floor() as i32,
                at[2].floor() as i32,
            );
            matches!(state >> 4, 8 | 9)
        };
        let sound = if status == 2 {
            entity_sound::hurt(kind, &meta, water(at))
        } else {
            entity_sound::death(kind, &meta, water(at))
        };
        if let Some((name, volume)) = sound {
            let jitter = s.rand_diff();
            let baby = if is_baby(kind, &meta) { 0.5 } else { 0.0 };
            s.play(name, Some(at), volume, jitter * 0.2 + 1.0 + baby);
        }
    }

    fn reset_sent_state(&mut self) {
        self.sneak_sent = false;
        self.sprint_sent = false;
        self.sent_move = SentMovement::default();
    }

    /// `handlePlayerRespawn`: a dimension change builds a new `WorldClient`, and `Minecraft.loadWorld` stops
    /// the sounds and clears the particles, then opens `GuiDownloadingTerrain`. Every respawn closes
    /// `GuiGameOver`; the new player has no open container.
    fn respawn_screens(&mut self, dimension_changed: bool) {
        self.reset_sent_state();
        self.ui.dead = None;
        self.container = None;
        self.game.inv.close_window();
        let m = metrics!(self);
        let mut q = Vec::new();
        if dimension_changed {
            if let Some(s) = &mut self.sound {
                s.stop_all();
            }
            self.particles.clear();
            self.game.player = None;
            self.awaiting_terrain = true;
            q = self.ui.joined(&m);
        } else if self.ui.is_game_over() {
            q = self.ui.close(&m);
        }
        self.run_effects(q);
    }

    /// Join, held slot and difficulty change the menus and the container before `Game` sees them.
    fn note_session_state(&mut self, ev: &NetEvent) {
        match ev {
            NetEvent::JoinGame {
                difficulty,
                hardcore,
                ..
            } => {
                self.hud = gui::Hud::default();
                self.hud.tablist.encrypted = self.link.session().is_some_and(|s| s.encrypted);
                self.container = None;
                self.ui.difficulty = Some(*difficulty);
                self.ui.hardcore = *hardcore;
                self.ui.in_world = true;
                self.awaiting_terrain = true;
                self.synced_held = 0;
                self.reset_sent_state();
                let m = metrics!(self);
                let q = self.ui.joined(&m);
                self.run_effects(q);
                self.send_client_settings();
                let r = self.session_send(|s| s.send_brand());
                self.send_result(r);
            }
            // `Minecraft.respawnPlayer` builds a new `EntityPlayerSP`: `sentSneaking` and `sentSprinting` start false.
            NetEvent::Respawn { dimension_changed } => self.respawn_screens(*dimension_changed),
            NetEvent::ResourcePack { url, hash } => self.offer_server_pack(url, hash),
            NetEvent::ResourcePackReady { hash, path } => {
                // The loader skips a pack it cannot open, as vanilla does; the server still hears about it.
                let status = if render::AssetSource::open(path).is_some() {
                    self.server_pack = Some(path.clone());
                    self.refresh_resources();
                    PackStatus::Loaded
                } else {
                    PackStatus::Failed
                };
                let r = self.session_send(|s| s.send_pack_status(hash, status));
                self.send_result(r);
            }
            NetEvent::Difficulty(d) => self.ui.difficulty = Some(*d),
            NetEvent::TabComplete(names) => {
                if let Some(line) = self.input.tab_response(names, &metrics!(self)) {
                    self.chat_note(&line);
                }
            }
            // `handleCustomPayload`: MC|BOpen opens the held written book.
            NetEvent::PluginMessage { channel, .. } if channel == "MC|BOpen" => {
                let held = self.game.inv.held_stack().as_ref();
                if let Some(book) = held.and_then(gui::screens::Book::for_stack) {
                    let m = metrics!(self);
                    let q = self.ui.open(&m, Box::new(book));
                    self.run_effects(q);
                }
            }
            _ => {}
        }
    }

    /// `handleChangeGameState` reason 10: the elder guardian ghost and its sound at the player.
    fn guardian_curse(&mut self) {
        let Some(pos) = self.game.player.as_ref().map(|p| p.pos) else {
            return;
        };
        self.particles
            .spawn_packet(41, pos.map(|v| v as f32), [0.0; 3], 0.0, 0, &[]);
        if let Some(s) = &mut self.sound {
            s.play("mob.guardian.curse", Some(pos), 1.0, 1.0);
        }
    }

    /// `handleChangeGameState` reasons 0, 5 and 6: the bed message, the demo help and the arrow hit ding.
    fn game_state(&mut self, reason: u8, value: f32) {
        let Some(feet) = self.game.player.as_ref().map(|p| p.pos) else {
            return;
        };
        let eye = [
            feet[0],
            feet[1] + eye_height(self.input.sneaking()),
            feet[2],
        ];
        match crate::game_state::react(reason, value, eye) {
            Some(Reaction::Sound {
                name,
                pos,
                volume,
                pitch,
            }) => {
                if let Some(s) = &mut self.sound {
                    s.play(name, Some(pos), volume, pitch);
                }
            }
            Some(Reaction::Message { key, keys }) => {
                let texts = &self.ui.texts;
                let names: Vec<String> = keys
                    .iter()
                    .filter_map(|d| self.ui.settings.keys.iter().find(|k| k.desc == *d))
                    .map(|k| gui::screens::key_name(k.code, texts))
                    .collect();
                let text = texts.fmt(key, &names.iter().map(String::as_str).collect::<Vec<_>>());
                self.chat_line(vec![ferrocraft_net_client::Styled {
                    text,
                    ..Default::default()
                }]);
            }
            None => {}
        }
    }

    /// Sounds, particles and explosions; `Err` hands back an event it does not own.
    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn world_event(&mut self, ev: NetEvent, dirty: &mut Dirty) -> Result<(), NetEvent> {
        match ev {
            NetEvent::Sound {
                name,
                pos,
                volume,
                pitch,
            } => {
                if let Some(s) = &mut self.sound {
                    s.play(&name, Some(pos), volume, wire_pitch(pitch));
                }
            }
            NetEvent::Particle {
                id,
                pos,
                offset,
                speed,
                count,
                data,
                ..
            } => self
                .particles
                .spawn_packet(id, pos, offset, speed, count, &data),
            NetEvent::Effect { id, pos, data, .. } => self.world_effect(id, pos, data),
            NetEvent::GameState { reason: 10, .. } => self.guardian_curse(),
            NetEvent::GameState {
                reason: reason @ (0 | 5 | 6),
                value,
            } => self.game_state(reason, value),
            NetEvent::Explosion {
                pos,
                radius,
                records,
                motion,
            } => self.explode(pos, radius, &records, motion, dirty),
            // `handleCamera` ignores an entity the world does not have.
            NetEvent::Camera(id) => {
                let own = id == self.game.entity_id;
                if own || self.game.entities.get(id).is_some() {
                    self.game.camera = (!own).then_some(id);
                    self.entity_shader(self.game.camera);
                }
            }
            other => return Err(other),
        }
        Ok(())
    }

    fn explode(
        &mut self,
        pos: [f32; 3],
        radius: f32,
        records: &[[i8; 3]],
        motion: [f32; 3],
        dirty: &mut Dirty,
    ) {
        if let Some(p) = &mut self.game.player {
            for (v, m) in p.vel.iter_mut().zip(motion) {
                *v += f64::from(m);
            }
        }
        if let Some(s) = &mut self.sound {
            let pitch = explode_pitch(s.rand_diff());
            s.play("random.explode", Some(pos.map(f64::from)), 4.0, pitch);
        }
        let base = pos.map(|v| v as i32);
        let blocks: Vec<[i32; 3]> = records
            .iter()
            .map(|r| [0, 1, 2].map(|i| base[i] + i32::from(r[i])))
            .collect();
        self.particles
            .explosion(pos.map(f64::from), radius, &blocks);
        for r in records {
            let at = [0, 1, 2].map(|i| base[i] + i32::from(r[i]));
            if self.game.store.get(at[0] >> 4, at[2] >> 4).is_some() {
                self.game.store.set_block(at[0], at[1], at[2], 0);
                dirty.insert(
                    (at[0] >> 4, at[2] >> 4),
                    Effect::Upload(at[0] >> 4, at[2] >> 4),
                );
            }
        }
    }

    /// Tab list, title, scoreboard and teams; `Err` hands back an event it does not own.
    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn hud_event(&mut self, ev: NetEvent) -> Result<(), NetEvent> {
        match ev {
            NetEvent::PlayerList { action } => self.player_list(action),
            NetEvent::TabHeaderFooter { header, footer } => {
                self.hud.tablist.header = header;
                self.hud.tablist.footer = footer;
            }
            NetEvent::Title {
                action,
                text,
                fade_in,
                stay,
                fade_out,
            } => self
                .hud
                .title
                .set_title(action, text, fade_in, stay, fade_out),
            other => return self.scoreboard_event(other),
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)] // `Err` hands the unmatched event back
    fn scoreboard_event(&mut self, ev: NetEvent) -> Result<(), NetEvent> {
        let sb = &mut self.hud.scoreboard;
        match ev {
            NetEvent::ScoreboardObjective {
                name,
                display,
                mode,
                hearts,
            } => sb.set_objective(&name, &display, mode, hearts),
            NetEvent::ScoreUpdate {
                name,
                objective,
                value,
                remove,
            } => sb.set_score(&name, &objective, value, remove),
            NetEvent::DisplayScoreboard { position, name } => sb.set_display(position, &name),
            NetEvent::Team {
                name,
                mode,
                display_name,
                prefix,
                suffix,
                visibility,
                color,
                friendly,
                members,
            } => {
                let info = TeamInfo {
                    display_name,
                    prefix,
                    suffix,
                    visibility,
                    color,
                    friendly,
                };
                sb.set_team(&name, mode, &info, &members);
            }
            other => return Err(other),
        }
        Ok(())
    }

    /// A right click on a villager still held when its window opens clicks the slot under the
    /// pointer (half a stack); the release is then ignored.
    pub(super) fn replay_held_mouse(&mut self) {
        if !std::mem::take(&mut self.replay_held_mouse) || self.container.is_none() {
            return;
        }
        for button in self.input.held_mouse_buttons().collect::<Vec<_>>() {
            self.mouse_button(true, button);
        }
    }

    /// At once: block entity data that follows in the same poll must survive.
    fn clear_chunks(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.clear_chunks();
        }
    }

    fn game_effect(&mut self, e: Effect, dirty: &mut Dirty) {
        match e {
            Effect::Clear => {
                self.clear_chunks();
                dirty.clear();
            }
            e @ (Effect::Upload(x, z) | Effect::Remove(x, z)) => {
                dirty.insert((x, z), e);
            }
            Effect::Chat(runs) => self.print_chat(runs),
            Effect::ActionBar(text) => self.hud.set_action_bar(text),
            Effect::Mounted => {
                let key = self.ui.settings.keys.iter().find(|k| k.desc == "key.sneak");
                let name = key.map_or(String::new(), |k| {
                    gui::screens::key_name(k.code, &self.ui.texts)
                });
                self.hud
                    .set_action_bar(self.ui.texts.fmt("mount.onboard", &[&name]));
            }
            Effect::OpenWindow => {
                self.close_screens();
                self.container = Some(ContainerScreen::new(
                    &self.game.inv,
                    self.ui.width,
                    self.ui.height,
                ));
                self.replay_held_mouse = true;
            }
            Effect::CloseWindow => {
                self.close_screens();
                self.container = None;
            }
            Effect::EditSign(pos) => {
                self.close_screens();
                self.open_sign_editor(pos)
            }
            Effect::RejectWindow(id) => {
                self.container = None;
                let r = self.session_send(|s| s.close_window(id));
                self.send_result(r);
            }
            Effect::Confirm { window, action } => {
                if let Some(s) = self.link.session() {
                    let r = s.confirm_transaction(window, action, true);
                    self.send_result(r);
                }
            }
            Effect::Tile(t) => {
                if let Some(r) = &mut self.renderer {
                    mirror_tile(r, t, self.game.difficulty, self.game.world_time);
                }
            }
            Effect::Died => {
                self.container = None;
                self.game.inv.close_window();
                let m = metrics!(self);
                let q = self.ui.open(&m, boxed(Death::new(self.game.score)));
                self.run_effects(q);
            }
        }
    }

    /// Prints a received chat line, through the mod host when one is live; a mod that cancels it
    /// stops the line.
    fn print_chat(&mut self, mut runs: Vec<ferrocraft_net_client::Styled>) {
        if let Some(mods) = &self.mods {
            match mods.chat(
                ferrocraft_modhost::ChatKind::Chat,
                &ferrocraft_net_client::formatted_text(&runs),
            ) {
                ferrocraft_modhost::ChatVerdict::Keep => {}
                ferrocraft_modhost::ChatVerdict::Cancel
                | ferrocraft_modhost::ChatVerdict::Taken => return,
                ferrocraft_modhost::ChatVerdict::Replace(text) => {
                    runs = vec![ferrocraft_net_client::Styled {
                        text,
                        ..Default::default()
                    }];
                }
            }
        }
        let m = gui::Metrics(self.renderer.as_ref().and_then(Renderer::font));
        let open = self.input.chat_input().is_some();
        self.hud
            .chat
            .print(runs, 0, self.hud.update_counter, open, m);
    }

    /// Tells the mod host that the chat input opened or closed, so a mod's `GuiNewChat` draws the open chat. A no-op
    /// while the state is the same as the last tick; the reply does not matter.
    pub(super) fn mod_chat_state(&mut self, open: bool) {
        if open == self.chat_open {
            return;
        }
        if let Some(mods) = &self.mods {
            let kind = if open {
                ferrocraft_modhost::ChatKind::InputOpen
            } else {
                ferrocraft_modhost::ChatKind::InputClosed
            };
            mods.chat(kind, "");
        }
    }

    /// Client half of `DoorBlock`, `TrapdoorBlock` and `FenceGateBlock` `onUse`: toggle the block
    /// and play the open or close sound. The server skips the clicking player's event.
    pub(super) fn toggle_openable(&mut self, pos: [i32; 3]) {
        let yaw = self.game.player.as_ref().map_or(0.0, |p| p.yaw);
        let store = &self.game.store;
        let at = |p: [i32; 3]| store.get_block(p[0], p[1], p[2]);
        let Some(t) = world::door::toggle(&at, pos, yaw) else {
            return;
        };
        for ([x, y, z], state) in t.set {
            self.game.store.set_block(x, y, z, state);
            if let (Some(r), Some(c)) = (&mut self.renderer, self.game.store.get(x >> 4, z >> 4)) {
                r.upload_chunk(&View(c));
            }
        }
        self.world_effect(t.event, pos, 0);
    }

    /// `S28 Effect`: record, door and block-break sounds, and the shards of a broken block.
    pub(super) fn world_effect(&mut self, id: i32, pos: [i32; 3], data: i32) {
        let at = pos.map(|v| f64::from(v) + 0.5);
        if let Some(s) = &mut self.sound {
            if id == 1005 {
                s.play_record(pos, data);
            } else {
                s.play_effect(id, data, at);
            }
        }
        match id {
            2001 => {
                let state = (((data & 0xfff) << 4) | ((data >> 12) & 0xf)) as u16;
                self.particles.break_block(pos, state, |x, z| {
                    self.game.store.get(x >> 4, z >> 4).map_or(1, |c| {
                        c.biome(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
                    })
                });
            }
            2002 => {
                let (rgb, instant) = world::potion_brew::color_and_instant(data);
                self.particles.splash_potion(pos, data, rgb, instant);
            }
            2005 => {
                let at = |p: [i32; 3]| self.game.store.get_block(p[0], p[1], p[2]);
                if at(pos) != 0 {
                    let top = world::dig::selection_box(&at, pos).map_or(1.0, |b| b[4]);
                    self.particles.bone_meal(pos, data, top);
                }
            }
            _ => self.particles.world_effect(id, pos, data),
        }
    }

    fn player_list(&mut self, action: PlayerListEvent) {
        let list = &mut self.hud.tablist;
        let (uuid, update): (u128, TabUpdate) = match action {
            PlayerListEvent::Add(players) => {
                for p in players {
                    list.add(TabEntry {
                        uuid: p.uuid,
                        name: p.name,
                        ping: p.ping,
                        gamemode: p.gamemode,
                        display: p.display,
                        textures: p.textures,
                        ..TabEntry::default()
                    });
                }
                return;
            }
            PlayerListEvent::Remove(ids) => {
                for u in ids {
                    list.remove(u);
                }
                return;
            }
            PlayerListEvent::Gamemode(u, g) => (u, Box::new(move |e| e.gamemode = g)),
            PlayerListEvent::Latency(u, ping) => (u, Box::new(move |e| e.ping = ping)),
            PlayerListEvent::DisplayName(u, d) => (u, Box::new(move |e| e.display = d)),
        };
        if let Some(e) = list.entry_mut(uuid) {
            update(e);
        }
    }
}

/// Mirror chunk changes into the renderer once per chunk per poll: a burst of block
/// changes would otherwise copy and remesh the same chunk once per block.
pub(super) fn upload(r: &mut Renderer, game: &Game, dirty: Dirty) {
    for ((x, z), effect) in dirty {
        match effect {
            Effect::Upload(..) => {
                if let Some(c) = game.store.get(x, z) {
                    r.upload_chunk(&View(c));
                }
            }
            _ => r.remove_chunk(x, z),
        }
    }
}

/// Hands chest, sign, skull and banner data to the renderer.
/// `Explosion.doExplosionA`: pitch `(1 + (r1 - r2) * 0.2) * 0.7`, 0.56 to 0.84.
fn explode_pitch(rand_diff: f32) -> f32 {
    (1.0 + rand_diff * 0.2) * 0.7
}

#[cfg(test)]
mod explode_pitch_tests {
    use super::explode_pitch;

    #[test]
    fn pitch_spans_vanilla_range() {
        assert!((explode_pitch(-1.0) - 0.56).abs() < 1e-6);
        assert!((explode_pitch(1.0) - 0.84).abs() < 1e-6);
        assert_ne!(explode_pitch(0.25), explode_pitch(-0.25));
        assert!((explode_pitch(0.0) - 0.7).abs() < 1e-6);
    }
}

fn mirror_tile(r: &mut Renderer, tile: TileEvent, difficulty: u8, world_time: i64) {
    match tile {
        TileEvent::Action {
            pos,
            block,
            event,
            value,
        } => r.block_action(pos, block, event, value),
        TileEvent::Sign { pos, lines } => r.set_sign_text(pos, lines),
        TileEvent::Skull {
            pos,
            kind,
            rotation,
            owner,
        } => {
            r.set_skull(
                pos,
                Skull {
                    kind,
                    rotation,
                    owner,
                },
            );
        }
        TileEvent::FlowerPot { pos, plant, data } => {
            r.set_block(
                pos[0],
                pos[1],
                pos[2],
                render::flower_pot_state(plant, data),
            );
        }
        TileEvent::Banner { pos, base, layers } => r.set_banner(pos, Banner { base, layers }),
        TileEvent::Spawner {
            pos,
            entity,
            delay,
            min_delay,
        } => r.set_spawner(pos, (&entity, delay, min_delay), difficulty, world_time),
    }
}

/// Opens a web page in the system browser.
pub(super) fn open_url(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    match std::process::Command::new(opener).arg(url).spawn() {
        // Reap the opener so it does not stay a zombie.
        Ok(mut c) => drop(std::thread::spawn(move || c.wait())),
        Err(e) => crate::log::line(&format!("Couldn't open link: {e}")),
    }
}

#[cfg(test)]
mod tests {
    //! A fake server logs a real `Session` in, puts `item` in the held slot, then sends a custom
    //! payload on `channel`. Vanilla `handleCustomPayload` opens `GuiScreenBook` only for
    //! `MC|BOpen` with a written book (id 387) in hand.
    use super::*;
    use crate::args::Args;
    use ferrocraft_net_client::Session;
    use ferrocraft_protocol::codec::{ItemStack, Rest};
    use ferrocraft_protocol::frame::FrameEncoder;
    use ferrocraft_protocol::nbt::{Nbt, Tag};
    use ferrocraft_protocol::play::clientbound as cb;
    use ferrocraft_protocol::{login, PacketSet};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    fn server(item: Option<i16>, channel: &'static str) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 256];
            let _ = s.read(&mut buf);
            let mut enc = FrameEncoder::default();
            let mut send = |p: Vec<u8>| {
                let mut out = Vec::new();
                enc.encode(&p, &mut out).unwrap();
                s.write_all(&out).unwrap();
            };
            send(
                login::clientbound::Packet::from(login::clientbound::LoginSuccess {
                    uuid: "00000000-0000-0000-0000-000000000001".into(),
                    username: "Ferro".into(),
                })
                .encode(),
            );
            send(
                cb::Packet::from(cb::JoinGame {
                    entity_id: 1,
                    gamemode: 0,
                    dimension: 0,
                    difficulty: 0,
                    max_players: 4,
                    level_type: "flat".into(),
                    reduced_debug_info: false,
                })
                .encode(),
            );
            let item = item.map(|id| ItemStack {
                id,
                count: 1,
                damage: 0,
                nbt: Some(Nbt {
                    name: String::new(),
                    root: vec![(
                        "pages".into(),
                        Tag::List(vec![Tag::String("\"p1\"".into())]),
                    )],
                }),
            });
            // slot 36 is hotbar 0, the held slot after join
            send(
                cb::Packet::from(cb::SetSlot {
                    window_id: 0,
                    slot: 36,
                    item,
                })
                .encode(),
            );
            send(
                cb::Packet::from(cb::PluginMessage {
                    channel: channel.into(),
                    data: Rest(vec![]),
                })
                .encode(),
            );
            while s.read(&mut buf).is_ok_and(|n| n > 0) {}
        });
        port
    }

    /// Whether a screen is open after the client handled everything the server sent.
    fn screen_opens(item: Option<i16>, channel: &'static str) -> bool {
        let port = server(item, channel);
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        let mut session = Session::connect(&format!("127.0.0.1:{port}"), "Ferro", None).unwrap();
        let (mut got_slot, mut got_msg) = (false, false);
        let end = Instant::now() + Duration::from_secs(10);
        while !got_msg {
            assert!(Instant::now() < end, "server events never arrived");
            for ev in session.poll() {
                got_slot |= matches!(ev, NetEvent::SetSlot { .. });
                if matches!(ev, NetEvent::PluginMessage { .. }) {
                    got_msg = true;
                    // drop the title and downloading-terrain screens so only the book can open one
                    let m = metrics!(app);
                    app.ui.close(&m);
                    assert!(!app.ui.is_open());
                }
                app.on_event(ev, &mut Dirty::new());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(got_slot);
        app.ui.is_open()
    }

    #[test]
    fn bopen_opens_only_for_a_held_written_book() {
        assert!(screen_opens(Some(387), "MC|BOpen"), "written book");
        assert!(!screen_opens(Some(386), "MC|BOpen"), "book and quill");
        assert!(!screen_opens(None, "MC|BOpen"), "empty hand");
        assert!(!screen_opens(Some(387), "MC|Brand"), "other channel");
    }

    /// Vanilla `attackTargetEntityWithCurrentItem`: a sprinting hit on a remote player scales x/z by 0.6 and
    /// ends the sprint; a mob target (client `attackEntityFrom` is false) changes nothing.
    #[test]
    fn hit_slows_attacker_only_for_a_player_target() {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.game.player = Some(world::Player::new([0.0; 3], 0.0));
        use world::entities::EntityEvent as E;
        app.game.entities.apply(E::SpawnPlayer {
            id: 7,
            uuid: 7,
            pos: [0; 3],
            yaw: 0,
            pitch: 0,
            held: 0,
            flags: None,
        });
        app.game.entities.apply(E::SpawnMob {
            id: 8,
            kind: 54,
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
        let set = |app: &mut App| {
            let p = app.game.player.as_mut().unwrap();
            p.vel = [1.0, 0.5, -1.0];
            p.sprinting = true;
        };
        set(&mut app);
        app.slow_after_hit(8);
        let p = app.game.player.as_ref().unwrap();
        assert_eq!((p.vel, p.sprinting), ([1.0, 0.5, -1.0], true), "mob hit");
        app.slow_after_hit(7);
        let p = app.game.player.as_ref().unwrap();
        assert_eq!(
            (p.vel, p.sprinting),
            ([0.6, 0.5, -0.6], false),
            "player hit"
        );
    }

    #[test]
    fn leaving_server_discards_its_statistics() {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.on_event(
            NetEvent::Statistics(vec![("achievement.mineWood".into(), 1)]),
            &mut Dirty::new(),
        );
        assert_eq!(app.ui.stats.read("achievement.mineWood"), 1);
        app.leave_world();
        app.on_event(
            NetEvent::Statistics(vec![("achievement.openInventory".into(), 1)]),
            &mut Dirty::new(),
        );
        assert_eq!(app.ui.stats.read("achievement.openInventory"), 1);
        assert_eq!(app.ui.stats.read("achievement.mineWood"), 0);
    }

    #[test]
    fn player_id_is_the_auth_uuid_with_the_username_offline() {
        for (auth, expected) in [
            (None, "Ferro"),
            (
                Some(ferrocraft_net_client::Auth::new(
                    "unused".into(),
                    "01234567-89ab-cdef-0123-456789abcdef",
                )),
                "0123456789abcdef0123456789abcdef",
            ),
        ] {
            let app = App::new(Args {
                username: "Ferro".into(),
                auth,
                assets_dir: None,
                server: None,
                game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
                ..Default::default()
            });
            assert_eq!(app.ui.player_id, expected);
        }
    }

    #[test]
    fn pack_prompt_waits_for_terrain_screen_to_close() {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.ui.in_world = true;
        let m = metrics!(app);
        app.ui.close(&m);
        app.awaiting_terrain = true;
        app.offer_server_pack("http://h/p.zip", "abc");
        assert!(
            !app.ui.is_open(),
            "prompt must not open over the terrain screen"
        );
        app.game.player = Some(world::Player::new([0.0; 3], 0.0));
        app.finish_terrain_wait();
        assert!(
            app.ui.is_open(),
            "prompt opens once the terrain screen closes"
        );
    }

    /// A new player entity has not told the server about sneak or sprint, so the next `send_state` must.
    #[test]
    fn respawn_clears_sent_sneak_and_sprint() {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.sneak_sent = true;
        app.sprint_sent = true;
        app.note_session_state(&NetEvent::Respawn {
            dimension_changed: false,
        });
        assert!(!app.sneak_sent && !app.sprint_sent);
    }

    /// An app in a world with a chest screen, a death screen, rain, thunder and particles, then a `Respawn`.
    fn respawned(dimension_changed: bool) -> App {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        app.ui.in_world = true;
        app.game.player = Some(world::Player::new([0.0; 3], 0.0));
        app.container = Some(ContainerScreen::new(&app.game.inv, 320, 240));
        app.game.weather.apply(7, 1.0);
        app.game.weather.apply(8, 1.0);
        app.particles
            .spawn_packet(41, [0.0; 3], [0.0; 3], 0.0, 0, &[]);
        assert!(!app.particles.is_empty());
        let m = metrics!(app);
        app.ui.open(&m, boxed(Death::new(0)));
        assert!(app.ui.is_game_over());
        app.on_event(NetEvent::Respawn { dimension_changed }, &mut Dirty::new());
        app
    }

    /// The server sends skull data right after the respawn: the clear must not eat it.
    #[test]
    #[ignore = "needs a GPU or software adapter"]
    fn respawn_clear_keeps_skull_data_of_the_same_poll_and_drops_the_old() {
        let mut app = respawned(true);
        app.renderer = Some(Renderer::new_offscreen(16, 16, None));
        let skull = |pos, kind| TileEvent::Skull {
            pos,
            kind,
            rotation: 0,
            owner: None,
        };
        let mut dirty = Dirty::new();
        app.game_effect(Effect::Tile(skull([0, 64, 0], 1)), &mut dirty);
        app.game_effect(Effect::Clear, &mut dirty);
        app.game_effect(Effect::Tile(skull([5, 64, 5], 3)), &mut dirty);
        let r = app.renderer.as_ref().unwrap();
        assert_eq!(r.skull([0, 64, 0]), None);
        assert_eq!(r.skull([5, 64, 5]).map(|s| s.kind), Some(3));
    }

    #[test]
    fn dimension_respawn_ends_rain_thunder_particles_and_container() {
        let app = respawned(true);
        assert_eq!(app.game.weather.strengths(), (0.0, 0.0));
        assert!(app.particles.is_empty());
        assert!(app.container.is_none());
    }

    #[test]
    fn dimension_respawn_opens_the_terrain_screen_over_the_death_screen() {
        let app = respawned(true);
        assert!(app.awaiting_terrain);
        assert!(app.ui.is_open() && !app.ui.is_game_over());
    }

    #[test]
    fn same_dimension_respawn_closes_death_screen_and_container_but_keeps_weather() {
        let app = respawned(false);
        assert!(!app.ui.is_open());
        assert!(!app.awaiting_terrain);
        assert!(app.container.is_none());
        assert_eq!(app.game.weather.strengths().0, 1.0);
    }

    #[test]
    fn first_position_packet_closes_the_terrain_screen() {
        let mut app = respawned(true);
        app.game.player = Some(world::Player::new([0.0; 3], 0.0));
        app.finish_terrain_wait();
        assert!(!app.ui.is_open() && !app.awaiting_terrain);
    }

    #[test]
    fn dimension_respawn_keeps_terrain_screen_until_new_position() {
        let mut app = respawned(true);
        app.finish_terrain_wait();
        assert!(app.ui.is_open() && app.awaiting_terrain);
    }

    /// A portal tick closes non-pausing screens (`EntityPlayerSP.onLivingUpdate`); `displayGuiScreen(null)`
    /// then opens `GuiGameOver` again for a dead player.
    #[test]
    fn death_screen_survives_a_close_and_ends_with_respawn() {
        let mut app = respawned(false);
        let health = NetEvent::Health {
            health: 0.0,
            food: 20,
            saturation: 5.0,
        };
        app.on_event(health, &mut Dirty::new());
        assert!(app.ui.is_game_over());
        app.close_screens();
        assert!(
            app.ui.is_game_over(),
            "a dead player must not stay in the world"
        );
        app.on_event(
            NetEvent::Respawn {
                dimension_changed: false,
            },
            &mut Dirty::new(),
        );
        assert!(!app.ui.is_open());
    }

    fn chat_app() -> App {
        let mut app = App::new(Args {
            username: "Ferro".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
            ..Default::default()
        });
        let m = metrics!(app);
        app.ui.close(&m);
        app.input.on_key(winit::keyboard::KeyCode::KeyT, true, None);
        assert!(app.input.chat_input().is_some());
        app
    }

    /// `doCloseMenu` (S2E) and `openChestMenu` (S2D) replace the chat screen.
    #[test]
    fn s2e_closes_chat_and_s2d_shows_the_container_without_chat() {
        let mut app = chat_app();
        app.on_event(NetEvent::CloseWindow, &mut Dirty::new());
        assert!(app.input.chat_input().is_none());

        let mut app = chat_app();
        app.on_event(
            NetEvent::OpenWindow {
                id: 1,
                kind: "minecraft:chest".into(),
                title: String::new(),
                slots: 27,
                horse: None,
            },
            &mut Dirty::new(),
        );
        assert!(app.input.chat_input().is_none());
        assert!(app.container.is_some());
    }

    /// `handleEntityAttach`: a known vehicle mounts and shows the hint; an unknown id dismounts.
    #[test]
    fn attach_to_a_known_horse_shows_the_dismount_hint_and_an_unknown_id_does_not_mount() {
        let mut app = chat_app();
        app.ui.texts = gui::screens::Texts::parse("mount.onboard=Press %1$s to dismount", "");
        let own = app.game.entity_id;
        let attach =
            |vehicle| NetEvent::Entity(world::entities::EntityEvent::Attach { id: own, vehicle });
        app.on_event(attach(99), &mut Dirty::new());
        assert_eq!(app.game.vehicle, None);
        assert_eq!(app.hud.action_bar(), None);
        app.game
            .entities
            .apply(world::entities::EntityEvent::SpawnMob {
                id: 99,
                kind: 100,
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
        app.on_event(attach(99), &mut Dirty::new());
        assert_eq!(app.game.vehicle, Some(99));
        assert_eq!(
            app.hud.action_bar().map(|a| a.0),
            Some("Press LSHIFT to dismount")
        );
    }
}
