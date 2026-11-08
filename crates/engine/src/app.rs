//! winit glue: window, event routing, the 20 TPS clock and the render call.

use crate::args::Args;
use crate::game::{Effect, Game, EYE_HEIGHT};
use ferrocraft_net_client::Session;
use input_audio::{apply_cursor_grab, scroll_slot, Action, InputState};
use render::{Camera, ChunkBlocks, Class, EntityDraw, Renderer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowId};
use world::chunk::Chunk;
use world::entities::{look_vector, Entity, Kind};

const TICK: Duration = Duration::from_millis(50);
/// Catch-up cap after a stall, like vanilla's `elapsedTicks` clamp.
const MAX_TICKS_PER_FRAME: u32 = 10;

pub struct App {
    args: Args,
    session: Session,
    game: Game,
    input: InputState,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    hud: gui::Hud,
    /// Rendered frames in the last second, for the F3 `fps` line.
    fps: (u32, u32, Instant),
    next_tick: Instant,
    grabbed: bool,
    show_debug: bool,
    hide_hud: bool,
    /// `FERROCRAFT_WINDOW_SECS`: exit after this long, for smoke tests.
    deadline: Option<Instant>,
}

impl App {
    pub fn new(args: Args, session: Session) -> App {
        App {
            args,
            session,
            game: Game::new(),
            input: InputState::new(0.5),
            window: None,
            renderer: None,
            hud: gui::Hud::default(),
            fps: (0, 0, Instant::now()),
            next_tick: Instant::now(),
            grabbed: false,
            show_debug: false,
            hide_hud: false,
            deadline: std::env::var("FERROCRAFT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(|s| Instant::now() + Duration::from_secs(s)),
        }
    }

    pub fn exit_reason(&self) -> Option<&str> {
        self.game.disconnected.as_deref()
    }

    /// Mirror chunk changes into the renderer once per chunk per poll: a burst of block
    /// changes would otherwise copy and remesh the same chunk once per block.
    fn pump_network(&mut self) {
        let mut clear = false;
        let mut dirty = HashMap::new();
        for ev in self.session.poll() {
            match self.game.apply(ev) {
                Some(Effect::Clear) => {
                    clear = true;
                    dirty.clear();
                }
                Some(e @ (Effect::Upload(x, z) | Effect::Remove(x, z))) => {
                    dirty.insert((x, z), e);
                }
                Some(Effect::Chat(runs)) => {
                    let m = gui::Metrics(self.renderer.as_ref().and_then(Renderer::font));
                    let open = self.input.chat_input().is_some();
                    self.hud
                        .chat
                        .print(runs, 0, self.hud.update_counter, open, m);
                }
                Some(Effect::ActionBar(text)) => self.hud.set_action_bar(text),
                None => {}
            }
        }
        if std::mem::take(&mut self.game.teleported) {
            self.send_pose();
        }
        let Some(r) = &mut self.renderer else { return };
        if clear {
            r.clear_chunks();
        }
        for ((x, z), effect) in dirty {
            match effect {
                Effect::Upload(..) => {
                    if let Some(c) = self.game.store.get(x, z) {
                        r.upload_chunk(&View(c));
                    }
                }
                _ => r.remove_chunk(x, z),
            }
        }
    }

    fn run_ticks(&mut self) {
        let now = Instant::now();
        let mut ran = 0;
        while self.next_tick <= now && ran < MAX_TICKS_PER_FRAME {
            self.next_tick += TICK;
            ran += 1;
            if let Some(line) = self.input.take_chat_submit() {
                self.hud.chat.add_sent(&line);
                let r = self.session.send_chat(&line);
                self.send_result(r);
            }
            self.hud.tick();
            self.game.tick(self.input.movement());
            self.send_pose();
        }
        if ran == MAX_TICKS_PER_FRAME {
            self.next_tick = now + TICK;
        }
    }

    fn send_pose(&mut self) {
        let Some(p) = &self.game.player else { return };
        let [x, y, z] = p.pos;
        let r = self
            .session
            .send_position_look(x, y, z, p.yaw, self.game.pitch, p.on_ground);
        self.send_result(r);
    }
    fn send_result(&mut self, r: std::io::Result<()>) {
        if let Err(e) = r {
            self.game
                .disconnected
                .get_or_insert(format!("send failed: {e}"));
        }
    }

    /// Left click: hit the entity under the crosshair (`EntityRenderer.getMouseOver`, entity part).
    /// Reach is 3 blocks in survival, 6 in creative; the swing is sent either way.
    fn attack(&mut self) {
        let Some(p) = &self.game.player else { return };
        let eye = [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]];
        let dir = look_vector(p.yaw, self.game.pitch);
        let reach = if self.game.creative { 6.0 } else { 3.0 };
        let hit = self
            .game
            .entities
            .pick(eye, dir, reach, self.game.entity_id, None);
        let r = match hit {
            Some(id) => self.session.attack_entity(id),
            None => self.session.swing_arm(),
        };
        self.send_result(r);
    }

    fn sync_cursor(&mut self) {
        let want = self.input.wants_cursor_grab();
        if want == self.grabbed {
            return;
        }
        let Some(w) = &self.window else { return };
        self.grabbed = apply_cursor_grab(w, want);
    }

    fn handle_actions(&mut self) {
        for action in self.input.take_actions() {
            match action {
                Action::ToggleDebug => self.show_debug = !self.show_debug,
                Action::ToggleHud => self.hide_hud = !self.hide_hud,
                Action::ToggleFullscreen => {
                    if let Some(w) = &self.window {
                        let on = w.fullscreen().is_none();
                        w.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
                    }
                }
                other => self.hud_action(other),
            }
        }
    }

    fn hud_action(&mut self, action: Action) {
        match action {
            Action::Hotbar(n) => self.hud.selected_slot = i32::from(n),
            Action::HotbarScroll(step) => {
                let slot = scroll_slot(self.hud.selected_slot as u8, step);
                self.hud.selected_slot = i32::from(slot);
            }
            // shortcut: vanilla scrolls 1 line with Shift held; the action carries no Shift state.
            Action::ChatScroll(step) => {
                let open = self.input.chat_input().is_some();
                self.hud.chat.scroll(step * 7, open);
            }
            Action::Attack => self.attack(),
            _ => {}
        }
    }

    fn draw(&mut self) {
        let until = self.next_tick.saturating_duration_since(Instant::now());
        let frac = (1.0 - until.as_secs_f64() / TICK.as_secs_f64()).clamp(0.0, 1.0);
        let quads = self.overlay(frac);
        let Some(r) = &mut self.renderer else { return };
        self.fps.0 += 1;
        if self.fps.2.elapsed() >= Duration::from_secs(1) {
            self.fps = (0, self.fps.0, Instant::now());
        }
        let cam = match &self.game.player {
            Some(p) => {
                let [px, py, pz] = p.pos;
                let [qx, qy, qz] = self.game.prev_pos;
                let lerp = |prev: f64, now: f64| prev + (now - prev) * frac;
                Camera {
                    pos: [lerp(qx, px), lerp(qy, py) + EYE_HEIGHT, lerp(qz, pz)],
                    yaw: p.yaw,
                    pitch: self.game.pitch,
                    fov_deg: 70.0,
                }
            }
            None => Camera {
                pos: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                fov_deg: 70.0,
            },
        };
        let draws: Vec<EntityDraw> = self
            .game
            .entities
            .iter()
            .map(|e| entity_draw(e, frac))
            .collect();
        r.set_entities(&draws);
        r.render(&cam, &quads);
    }

    /// `GuiIngame.renderGameOverlay` laid out as quads; F1 (`hideGUI`) skips it unless a
    /// screen (chat) is open, as in `EntityRenderer`.
    fn overlay(&mut self, frac: f64) -> Vec<render::Quad> {
        let Some(r) = &self.renderer else {
            return Vec::new();
        };
        if self.hide_hud && self.input.chat_input().is_none() {
            return Vec::new();
        }
        let left = self.game.player.as_ref().map(|p| {
            gui::debug::left_lines(&gui::debug::DebugInfo {
                fps_line: format!("{} fps (0 chunk updates) T: 120 fast", self.fps.1),
                provider: format!(
                    "MultiplayerChunkCache: {}, {}",
                    self.game.store.len(),
                    self.game.store.len()
                ),
                pos: p.pos,
                yaw: p.yaw,
                pitch: self.game.pitch,
                ..Default::default()
            })
        });
        let (w, h) = r.size();
        let mut g = gui::Gui::new(gui::GuiScale::new(w, h, 0), r.font(), 0);
        self.hud.draw(
            &mut g,
            &gui::Frame {
                partial_ticks: frac as f32,
                chat_input: self.input.chat_input(),
                debug: left
                    .as_deref()
                    .filter(|_| self.show_debug)
                    .map(|l| (l, &[][..])),
                fancy: true,
                brightness: 1.0,
                player_view: self.game.player.is_some(),
                ..Default::default()
            },
        );
        g.into_quads()
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Ferrocraft 1.8.9");
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        self.renderer = Some(Renderer::new(
            window.clone(),
            self.args.assets_dir.as_deref(),
        ));
        // A child process (loader, terminal) starts unfocused on macOS: no keys until focused.
        window.focus_window();
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        self.input.on_window_event(&event);
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.input.on_mouse_motion(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.pump_network();
        let (dx, dy) = self.input.take_look_delta();
        self.game.look(dx, dy);
        self.run_ticks();
        self.handle_actions();
        self.sync_cursor();
        if self.game.disconnected.is_some() || self.deadline.is_some_and(|d| Instant::now() >= d) {
            el.exit();
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn entity_draw(e: &Entity, partial: f64) -> EntityDraw {
    let class = match e.kind {
        Kind::Player => Class::Player {
            slim: e.uuid.is_some_and(|u| {
                (u as u32 ^ (u >> 32) as u32 ^ (u >> 64) as u32 ^ (u >> 96) as u32) & 1 == 1
            }),
        },
        Kind::Mob(id) => Class::Mob(id),
        Kind::Object(id) => Class::Object(id),
        Kind::ExperienceOrb => Class::Orb,
    };
    let p = partial as f32;
    EntityDraw {
        class,
        pos: e.lerp_pos(partial),
        yaw: e.yaw,
        head_yaw: e.head_yaw,
        pitch: e.pitch,
        limb_swing: e.limb_swing - e.limb_swing_amount * (1.0 - p),
        limb_swing_amount: e.prev_limb_swing_amount
            + (e.limb_swing_amount - e.prev_limb_swing_amount) * p,
        swing: e.swing_progress(p),
        age: e.age as f32 + p,
        sneaking: e.sneaking(),
        hurt: e.hurt_time > 0,
        death_time: if e.is_dead() {
            e.death_time as f32 + p
        } else {
            0.0
        },
        invisible: e.invisible(),
        size: e.kind.size(),
        held: e.equipment[0].map_or(0, |i| i.id),
        armor: [1, 2, 3, 4].map(|s| e.equipment[s].map_or(0, |i| i.id)),
        name: e.name.clone(),
    }
}

/// Lets the renderer read a `world` chunk.
struct View<'a>(&'a Chunk);

impl ChunkBlocks for View<'_> {
    fn chunk_x(&self) -> i32 {
        self.0.x
    }
    fn chunk_z(&self) -> i32 {
        self.0.z
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        self.0.block(x, y, z)
    }
}
