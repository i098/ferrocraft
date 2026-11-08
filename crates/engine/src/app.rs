//! winit glue: window, event routing, the 20 TPS clock and the render call.

use crate::args::Args;
use crate::game::{Effect, Game, EYE_HEIGHT};
use ferrocraft_net_client::Session;
use input_audio::InputState;
use render::{Camera, Hud, Renderer};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorGrabMode, Window, WindowId};

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
    next_tick: Instant,
    grabbed: bool,
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
            next_tick: Instant::now(),
            grabbed: false,
        }
    }

    pub fn exit_reason(&self) -> Option<&str> {
        self.game.disconnected.as_deref()
    }

    fn pump_network(&mut self) {
        for ev in self.session.poll() {
            let effect = self.game.apply(ev);
            let (Some(r), Some(effect)) = (&mut self.renderer, effect) else {
                continue;
            };
            match effect {
                Effect::Upload(x, z) => {
                    if let Some(c) = self.game.store.get(x, z) {
                        r.upload_chunk(c);
                    }
                }
                Effect::Remove(x, z) => r.remove_chunk(x, z),
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
                let r = self.session.send_chat(&line);
                self.send_result(r);
            }
            let (dx, dy) = self.input.take_look_delta();
            self.game.tick(self.input.movement());
            if let Some(p) = &self.game.player {
                let [x, y, z] = p.pos;
                let r =
                    self.session
                        .send_position_look(x, y, z, p.yaw, self.game.pitch, p.on_ground);
                self.send_result(r);
            }
        }
        if ran == MAX_TICKS_PER_FRAME {
            self.next_tick = now + TICK;
        }
    }

    fn send_result(&mut self, r: std::io::Result<()>) {
        if let Err(e) = r {
            self.game
                .disconnected
                .get_or_insert(format!("send failed: {e}"));
        }
    }

    fn sync_cursor(&mut self) {
        let want = self.input.wants_cursor_grab();
        if want == self.grabbed {
            return;
        }
        let Some(w) = &self.window else { return };
        if want {
            let _ = w
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined));
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
        }
        w.set_cursor_visible(!want);
        self.grabbed = want;
    }

    fn draw(&mut self) {
        let (Some(r), Some(p)) = (&mut self.renderer, &self.game.player) else {
            if let Some(r) = &mut self.renderer {
                r.render(
                    &Camera {
                        pos: [0.0; 3],
                        yaw: 0.0,
                        pitch: 0.0,
                        fov_deg: 70.0,
                    },
                    &Hud {
                        chat: self.game.chat.clone(),
                        chat_input: self.input.chat_input().map(String::from),
                        crosshair: false,
                        debug: vec!["waiting for position".into()],
                    },
                );
            }
            return;
        };
        let cam = Camera {
            pos: [p.pos[0], p.pos[1] + EYE_HEIGHT, p.pos[2]],
            yaw: p.yaw,
            pitch: self.game.pitch,
            fov_deg: 70.0,
        };
        let hud = Hud {
            chat: self.game.chat.clone(),
            chat_input: self.input.chat_input().map(String::from),
            crosshair: true,
            debug: vec![
                format!("XYZ: {:.3} / {:.5} / {:.3}", p.pos[0], p.pos[1], p.pos[2]),
                format!("chunks: {}", self.game.store.len()),
            ],
        };
        r.render(&cam, &hud);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Ferrocraft 1.8.9");
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        let assets = self.args.assets_dir.as_deref().unwrap_or(Path::new(""));
        self.renderer = Some(Renderer::new(window.clone(), assets));
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
        self.run_ticks();
        self.sync_cursor();
        if self.game.disconnected.is_some() {
            el.exit();
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
