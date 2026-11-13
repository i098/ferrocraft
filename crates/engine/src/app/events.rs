//! Server events and menu effects: what the network and the screens ask the client to do.

use super::{boxed, App, View};
use crate::game::Effect;
use crate::game::Game;
use crate::link::Link;
use ferrocraft_net_client::{split_address, NetEvent, PlayerListEvent};
use gui::inventory::ContainerScreen;
use gui::screens::{Connecting, Death, Effect as UiEffect};
use gui::tablist::TabEntry;
use input_audio::sound::blocks::wire_pitch;
use render::Renderer;
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
            UiEffect::Sound(name) => {
                if let Some(s) = &mut self.sound {
                    s.play_ui(name);
                }
            }
            UiEffect::Connect { host, port } => self.connect(&host, port),
            UiEffect::CancelConnect => self.link = Link::Offline,
            UiEffect::Disconnect => self.leave_world(),
            UiEffect::Respawn => {
                let r = self.link.session().map(|s| s.respawn());
                self.send_result(r.unwrap_or(Ok(())));
            }
            UiEffect::Quit => self.quit = true,
            UiEffect::SettingsSaved => self.apply_settings(),
            UiEffect::LanguageChanged => self.change_language(),
            UiEffect::OpenUrl(url) => open_url(&url),
            UiEffect::SuperSecret => {
                if let Some(s) = &mut self.sound {
                    s.play_super_secret();
                }
                self.next_shader();
            }
            UiEffect::ResourcesChanged => self.refresh_resources(),
        }
    }

    fn connect(&mut self, host: &str, port: u16) {
        self.game = Game::new();
        let addr = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        self.link = Link::connect(addr, self.args.username.clone(), self.args.auth.clone());
    }

    /// `Minecraft.refreshResources` after a resource pack change: a new renderer over the new
    /// packs, then the chunks, texts and screens again.
    // shortcut: sounds and mod-registered textures still come from before the change; reload them when packs need it.
    fn refresh_resources(&mut self) {
        let Some(w) = self.window.clone() else { return };
        // Drop the old renderer first: a window holds one surface at a time.
        self.renderer = None;
        self.renderer = Some(Renderer::new(w, self.asset_source()));
        self.load_assets();
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
        self.resize_screens();
    }

    /// Back to the menus: forget the world and the connection.
    fn leave_world(&mut self) {
        self.link = Link::Offline;
        self.game = Game::new();
        self.hud = gui::Hud::default();
        self.container = None;
        self.particles.clear();
        self.ui.in_world = false;
        self.ui.difficulty = None;
        self.awaiting_terrain = false;
        self.perspective = 0;
        self.sneak_sent = false;
        self.sprint_sent = false;
        self.use_ticks = 0;
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

    pub(super) fn pump_network(&mut self) {
        if let Some(e) = self.link.poll_login() {
            let m = metrics!(self);
            let q = self.ui.connect_failed(&m, &e.to_string());
            self.run_effects(q);
        }
        let events = self.link.session().map(|s| s.poll()).unwrap_or_default();
        let mut clear = false;
        let mut dirty = Dirty::new();
        for ev in events {
            self.on_event(ev, &mut clear, &mut dirty);
        }
        if std::mem::take(&mut self.game.teleported) {
            self.send_pose();
        }
        if let Some(r) = &mut self.renderer {
            if clear {
                r.clear_chunks();
            }
            upload(r, &self.game, dirty);
        }
        if self.awaiting_terrain && self.game.player.is_some() {
            self.awaiting_terrain = false;
            let m = metrics!(self);
            let q = self.ui.close(&m);
            self.run_effects(q);
        }
        if let Some(reason) = self.game.disconnected.take() {
            self.leave_world();
            let m = metrics!(self);
            let q = self.ui.disconnected(&m, &reason);
            self.run_effects(q);
        }
    }

    fn on_event(&mut self, ev: NetEvent, clear: &mut bool, dirty: &mut Dirty) {
        self.note_session_state(&ev);
        self.entity_sound(&ev);
        if let NetEvent::Map { id, area, data } = &ev {
            if let Some(r) = &mut self.renderer {
                r.map_update(*id, *area, data);
            }
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
        if let Some(e) = self.game.apply(ev) {
            self.game_effect(e, clear, dirty);
        }
    }

    /// `handleStatusUpdate` hurt (2) and death (3) sounds, `handleCollectItem` pickup sounds.
    /// Plays at the entity before `Game` applies the event; the own player is not in `entities`.
    fn entity_sound(&mut self, ev: &NetEvent) {
        let NetEvent::Entity(ev) = ev else { return };
        let (id, status) = match ev {
            EntityEvent::Status {
                id,
                status: s @ (2 | 3),
            } => (*id, Some(*s)),
            EntityEvent::Pickup { id } => (*id, None),
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

    /// Join, held slot and difficulty change the menus and the container before `Game` sees them.
    fn note_session_state(&mut self, ev: &NetEvent) {
        match ev {
            NetEvent::JoinGame {
                difficulty,
                hardcore,
                ..
            } => {
                self.hud = gui::Hud::default();
                self.container = None;
                self.ui.difficulty = Some(*difficulty);
                self.ui.hardcore = *hardcore;
                self.ui.in_world = true;
                self.awaiting_terrain = true;
                self.synced_held = 0;
                let m = metrics!(self);
                let q = self.ui.joined(&m);
                self.run_effects(q);
                self.send_client_settings();
            }
            NetEvent::HeldItem(n) => self.synced_held = (*n).min(8),
            NetEvent::Difficulty(d) => self.ui.difficulty = Some(*d),
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
            s.play("random.explode", Some(pos.map(f64::from)), 4.0, 0.7);
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
            } => {
                sb.set_objective(&name, &display, mode);
            }
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
                prefix,
                suffix,
                members,
            } => sb.set_team(&name, mode, &prefix, &suffix, &members),
            other => return Err(other),
        }
        Ok(())
    }

    fn game_effect(&mut self, e: Effect, clear: &mut bool, dirty: &mut Dirty) {
        match e {
            Effect::Clear => {
                *clear = true;
                dirty.clear();
            }
            e @ (Effect::Upload(x, z) | Effect::Remove(x, z)) => {
                dirty.insert((x, z), e);
            }
            Effect::Chat(runs) => {
                let m = gui::Metrics(self.renderer.as_ref().and_then(Renderer::font));
                let open = self.input.chat_input().is_some();
                self.hud
                    .chat
                    .print(runs, 0, self.hud.update_counter, open, m);
            }
            Effect::ActionBar(text) => self.hud.set_action_bar(text),
            Effect::OpenWindow => {
                self.container = Some(ContainerScreen::new(
                    &self.game.inv,
                    self.ui.width,
                    self.ui.height,
                ));
            }
            Effect::CloseWindow => self.container = None,
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
            Effect::Died => {
                self.container = None;
                self.game.inv.close_window();
                let m = metrics!(self);
                let q = self.ui.open(
                    &m,
                    boxed(Death::new(self.game.death_message.clone(), self.game.score)),
                );
                self.run_effects(q);
            }
        }
    }

    /// `S28 Effect`: record, door and block-break sounds, and the shards of a broken block.
    fn world_effect(&mut self, id: i32, pos: [i32; 3], data: i32) {
        let at = pos.map(|v| f64::from(v) + 0.5);
        if let Some(s) = &mut self.sound {
            s.play_effect(id, data, at);
        }
        match id {
            2001 => {
                let state = (((data & 0xfff) << 4) | ((data >> 12) & 0xf)) as u16;
                self.particles.break_block(pos, state);
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

/// Opens a web page in the system browser.
fn open_url(url: &str) {
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
