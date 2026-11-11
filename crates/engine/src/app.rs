//! winit glue: window, event routing, the 20 TPS clock and the render call.

/// Font measurement over the renderer's font, for `gui` screens. A macro so the borrow stays
/// on `renderer` alone and `ui` can be borrowed mutably beside it.
macro_rules! metrics {
    ($s:expr) => {
        $crate::ui::Fm(gui::Metrics($s.renderer.as_ref().and_then(|r| r.font())))
    };
}

mod events;
mod input;
mod own_sounds;
mod play;
mod screen;

use crate::args::Args;
use crate::game::{eye_height, Game};
use crate::link::Link;
use crate::skins::PlayerSkins;
use crate::ui::{Fm, Ui};
use gui::inventory::icons::Icons;
use gui::inventory::ContainerScreen;
use gui::screens::{Screen, Texts};
use gui::status::Status;
use input_audio::sound::Sound;
use input_audio::InputState;
use render::{
    AssetSource, Camera, ChunkBlocks, Class, EntityDraw, Environment, Fluid, Hand, HeldKind,
    Particles, Renderer, UseAction, Using,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};
use world::chunk::Chunk;
use world::entities::{look_vector, Entity, Kind};

const TICK: Duration = Duration::from_millis(50);
/// Catch-up cap after a stall, like vanilla's `elapsedTicks` clamp.
const MAX_TICKS_PER_FRAME: u32 = 10;
/// `EntityRenderer.thirdPersonDistance`.
const THIRD_PERSON_DISTANCE: f64 = 4.0;

/// Quads, textures they registered and effects they raised.
type Overlay = (
    Vec<render::Quad>,
    Vec<(String, Vec<u8>)>,
    Vec<gui::screens::Effect>,
);

/// `EntityPlayer.limbSwing` for the local player, so the third-person model walks.
#[derive(Default)]
struct Limbs {
    swing: f32,
    amount: f32,
    prev_amount: f32,
}

pub struct App {
    args: Args,
    link: Link,
    game: Game,
    input: InputState,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    hud: gui::Hud,
    /// The Java 8 mod host (Forge mods), when `FERROCRAFT_MODHOST_*` is set.
    mods: Option<ferrocraft_modhost::ModHost>,
    ui: Ui,
    icons: Option<Icons>,
    container: Option<ContainerScreen>,
    particles: Particles,
    sound: Option<Sound>,
    /// Rendered frames in the last second, for the F3 `fps` line.
    fps: (u32, u32, Instant),
    next_tick: Instant,
    grabbed: bool,
    /// Left-click presses since the last tick that did not hit an entity.
    attack_presses: u32,
    show_debug: bool,
    hide_hud: bool,
    /// F5: 0 first person, 1 behind the player, 2 in front of the player.
    perspective: u8,
    /// The hotbar slot the server last heard of.
    synced_held: u8,
    /// `rightClickDelayTimer`.
    use_delay: i32,
    /// `EntityPlayer.itemInUseCount`: ticks left of an item use that a release (`C07` status 5)
    /// ends early; 0 while no item is in use.
    use_ticks: i32,
    sneak_sent: bool,
    sprint_sent: bool,
    /// Ticks the left button has dug at one block, for hit sounds and shards every 4 ticks.
    dig_ticks: u32,
    /// Whether the chat input was open last tick; closing it resets the chat scroll.
    chat_open: bool,
    /// `Entity.distanceWalkedOnStepModified` and `nextStepDistance` for footstep sounds.
    walked: f32,
    next_step: i32,
    own_sounds: own_sounds::State,
    limbs: Limbs,
    /// `EntityRenderer.fovModifierHand` and its previous tick value.
    fov_mod: (f32, f32),
    view_bob: play::ViewBob,
    /// When the last frame was shown, for the `maxFps` limit.
    last_frame: Instant,
    /// `ItemRenderer.equippedProgress` and its previous value, with the item shown.
    equip: (f32, f32, Option<(i16, i16)>),
    /// `GuiDownloadingTerrain` stays until the first position packet.
    awaiting_terrain: bool,
    cursor: (f64, f64),
    ctrl: bool,
    shift: bool,
    mouse_down: bool,
    /// Whether `input` was last told a screen is open.
    input_modal: bool,
    quit: bool,
    started: Instant,
    skins: PlayerSkins,
    /// `FERROCRAFT_WINDOW_SECS`: exit after this long, for smoke tests.
    deadline: Option<Instant>,
}

impl App {
    pub fn new(args: Args) -> App {
        let ui = Ui::new(args.game_dir.clone(), Texts::default());
        let mut app = App {
            skins: PlayerSkins::new(&args.game_dir),
            input: InputState::new(ui.settings.mouse_sensitivity),
            link: Link::Offline,
            game: Game::new(),
            window: None,
            renderer: None,
            hud: gui::Hud::default(),
            mods: None,
            ui,
            icons: None,
            container: None,
            particles: Particles::new(),
            sound: None,
            fps: (0, 0, Instant::now()),
            next_tick: Instant::now(),
            grabbed: false,
            attack_presses: 0,
            show_debug: false,
            hide_hud: false,
            perspective: 0,
            synced_held: 0,
            use_delay: 0,
            use_ticks: 0,
            sneak_sent: false,
            sprint_sent: false,
            dig_ticks: 0,
            chat_open: false,
            walked: 0.0,
            next_step: 0,
            own_sounds: own_sounds::State::default(),
            limbs: Limbs::default(),
            fov_mod: (1.0, 1.0),
            view_bob: play::ViewBob::default(),
            last_frame: Instant::now(),
            equip: (1.0, 1.0, None),
            awaiting_terrain: false,
            cursor: (0.0, 0.0),
            ctrl: false,
            shift: false,
            mouse_down: false,
            input_modal: false,
            quit: false,
            started: Instant::now(),
            deadline: std::env::var("FERROCRAFT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(|s| Instant::now() + Duration::from_secs(s)),
            args,
        };
        app.apply_settings();
        app
    }

    /// Loads language texts, item icons and sounds once the window and its assets exist.
    fn load_assets(&mut self) {
        let Some(mut src) = self.args.assets_dir.as_deref().and_then(AssetSource::open) else {
            return;
        };
        let mut text = |name: &str| {
            src.read(&format!("assets/minecraft/{name}"))
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default()
        };
        let lang = text(&format!("lang/{}.lang", self.ui.settings.lang));
        let lang = if lang.is_empty() {
            text("lang/en_US.lang")
        } else {
            lang
        };
        self.ui.texts = Texts::parse(&lang, &text("texts/splashes.txt"));
        self.icons = Some(Icons::new(|name| {
            src.read(&format!("assets/minecraft/{name}"))
        }));
    }

    fn load_sound(&mut self) {
        let dirs = [
            Some(self.args.game_dir.clone()),
            self.args.game_dir.parent().map(|p| p.to_path_buf()),
        ];
        for dir in dirs.into_iter().flatten() {
            if let Ok(s) = Sound::new(&dir, "1.8.9") {
                self.sound = Some(s);
                self.apply_settings();
                return;
            }
        }
        crate::log::line("no sound: assets index or output device not found");
    }

    /// Pushes `options.txt` values into the parts that read them.
    fn apply_settings(&mut self) {
        let s = &self.ui.settings;
        self.input.set_sensitivity(s.mouse_sensitivity);
        self.input.set_pause_on_lost_focus(s.pause_on_lost_focus);
        self.input
            .set_bindings(input_audio::KeyBindings::from_options_txt(&s.to_text()));
        if let Some(sound) = &mut self.sound {
            for (cat, v) in input_audio::sound::index::Category::ALL
                .iter()
                .zip(s.volumes)
            {
                sound.set_volume(*cat, v);
            }
        }
        let chat_settings = gui::chat::ChatSettings {
            visibility: match s.chat_visibility {
                0 => gui::chat::Visibility::Full,
                1 => gui::chat::Visibility::System,
                _ => gui::chat::Visibility::Hidden,
            },
            colors: s.chat_colors,
            opacity: s.chat_opacity,
            scale: s.chat_scale,
            width: s.chat_width,
            height_focused: s.chat_height_focused,
            height_unfocused: s.chat_height_unfocused,
        };
        let (fullscreen, vsync, force_unicode) = (s.fullscreen, s.vsync, s.force_unicode);
        let vsync = std::env::var("FERROCRAFT_VSYNC").map_or(vsync, |v| v != "0");
        let unicode = self
            .renderer
            .as_mut()
            .is_some_and(|r| r.set_force_unicode(force_unicode));
        if unicode || self.hud.chat.settings != chat_settings {
            self.hud.chat.settings = chat_settings;
            let m = gui::Metrics(self.renderer.as_ref().and_then(Renderer::font));
            self.hud.chat.refresh(m);
        }
        if let Some(w) = &self.window {
            if w.fullscreen().is_some() != fullscreen {
                w.set_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None)));
            }
        }
        if let Some(r) = &mut self.renderer {
            r.set_vsync(vsync);
        }
        self.send_client_settings();
    }

    /// `GameSettings.sendSettingsToServer`: C15 with the locale, view distance, chat and skin parts.
    fn send_client_settings(&mut self) {
        let s = &self.ui.settings;
        let (lang, distance, chat, colors, parts) = (
            s.lang.clone(),
            s.render_distance as i8,
            s.chat_visibility as i8,
            s.chat_colors,
            s.model_parts,
        );
        if let Some(sess) = self.link.session() {
            let r = sess.client_settings(&lang, distance, chat, colors, parts);
            self.send_result(r);
        }
    }

    /// `Minecraft.getLimitFramerate`: 30 fps in menus without a world, else `maxFps` (260 is
    /// unlimited).
    fn pace_frame(&mut self, el: &ActiveEventLoop) -> bool {
        let limit = if !self.ui.in_world && self.ui.is_open() {
            30
        } else {
            self.ui.settings.max_fps
        };
        if (1..260).contains(&limit) {
            let due = self.last_frame + Duration::from_secs_f64(1.0 / f64::from(limit));
            if due > Instant::now() {
                el.set_control_flow(ControlFlow::WaitUntil(due));
                return false;
            }
        }
        el.set_control_flow(ControlFlow::Poll);
        self.last_frame = Instant::now();
        true
    }

    /// `ScreenShotHelper.saveScreenshot`: `screenshots/<date>.png` under the game dir, and a chat line.
    fn save_screenshot(&mut self, (w, h, rgba): (u32, u32, Vec<u8>)) {
        let dir = self.args.game_dir.join("screenshots");
        let stamp = screenshot_stamp();
        let mut path = dir.join(format!("{stamp}.png"));
        for n in 2.. {
            if !path.exists() {
                break;
            }
            path = dir.join(format!("{stamp}_{n}.png"));
        }
        let saved =
            std::fs::create_dir_all(&dir).and_then(|()| render::encode_png(&path, w, h, &rgba));
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let text = match saved {
            Ok(()) => self.ui.texts.fmt("screenshot.success", &[&name]),
            Err(e) => self.ui.texts.fmt("screenshot.failure", &[&e.to_string()]),
        };
        self.chat_line(vec![ferrocraft_net_client::Styled {
            format: String::new(),
            text,
        }]);
    }

    fn send_result(&mut self, r: std::io::Result<()>) {
        if let Err(e) = r {
            self.game
                .disconnected
                .get_or_insert(format!("send failed: {e}"));
        }
    }

    /// The local player's eye position, lerped between ticks.
    fn eye(&self, frac: f64) -> Option<[f64; 3]> {
        let p = self.game.player.as_ref()?;
        let lerp = |prev: f64, now: f64| prev + (now - prev) * frac;
        let [px, py, pz] = p.pos;
        let [qx, qy, qz] = self.game.prev_pos;
        let height = eye_height(self.input.sneaking());
        Some([lerp(qx, px), lerp(qy, py) + height, lerp(qz, pz)])
    }

    /// `Minecraft.isReducedDebug`: the player's game rule flag or the option.
    fn reduced_debug(&self) -> bool {
        self.game.reduced_debug || self.ui.settings.reduced_debug_info
    }

    fn camera(&self, frac: f64) -> Camera {
        let mut cam = self.view_camera(frac);
        cam.hurt_roll = self.hurt_roll(frac as f32);
        // `getFOVModifier`: dying zooms out, being in water zooms in.
        let mut zoom = 1.0;
        if self.game.health <= 0.0 {
            let t = f32::from(self.game.death_ticks) + frac as f32;
            zoom /= (1.0 - 500.0 / (t + 500.0)) * 2.0 + 1.0;
        }
        if self.fluid_at(cam.pos) == Some(Fluid::Water) {
            zoom *= 60.0 / 70.0;
        }
        cam.fov_deg *= zoom;
        cam.hand_fov_deg *= zoom;
        if self.ui.settings.view_bobbing && self.game.player.is_some() {
            cam.bob = self.view_bob.offsets(frac as f32);
        }
        cam
    }

    /// `EntityRenderer.applyHurtCam`; the hit direction is always 0 on the client, so the
    /// yaw rotations around the roll cancel.
    fn hurt_roll(&self, frac: f32) -> f32 {
        let g = &self.game;
        let mut roll = 0.0;
        if g.health <= 0.0 {
            roll += 40.0 - 8000.0 / (f32::from(g.death_ticks) + frac + 200.0);
        }
        let t = f32::from(g.hurt_timer) - frac;
        if t >= 0.0 {
            let t = t / 10.0;
            roll -= (t * t * t * t * std::f32::consts::PI).sin() * 14.0;
        }
        roll
    }

    /// The pose of a timed use of the held item (`EnumAction`), for the first-person hand.
    fn using(&self, frac: f32) -> Option<Using> {
        if self.use_ticks <= 0 {
            return None;
        }
        let id = self.game.inv.held_stack().as_ref()?.id;
        let action = match id {
            261 => UseAction::Bow,
            267 | 268 | 272 | 276 | 283 => UseAction::Block,
            _ => UseAction::Consume,
        };
        Some(Using {
            action,
            timer: self.use_ticks as f32 - frac + 1.0,
        })
    }

    fn view_camera(&self, frac: f64) -> Camera {
        let fov = self.ui.settings.fov
            * (self.fov_mod.1 + (self.fov_mod.0 - self.fov_mod.1) * frac as f32);
        let (Some(p), Some(eye)) = (&self.game.player, self.eye(frac)) else {
            return Camera {
                pos: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                fov_deg: fov,
                ..Camera::default()
            };
        };
        if self.perspective == 0 {
            return Camera {
                pos: eye,
                yaw: p.yaw,
                pitch: self.game.pitch,
                fov_deg: fov,
                ..Camera::default()
            };
        }
        let (yaw, pitch) = if self.perspective == 2 {
            (p.yaw + 180.0, -self.game.pitch)
        } else {
            (p.yaw, self.game.pitch)
        };
        let dir = look_vector(yaw, pitch);
        let mut dist = 0.0;
        while dist < THIRD_PERSON_DISTANCE {
            let next = dist + 0.1;
            let at = [0, 1, 2].map(|i| (eye[i] - dir[i] * next).floor() as i32);
            if render::is_solid(self.game.store.get_block(at[0], at[1], at[2])) {
                break;
            }
            dist = next;
        }
        Camera {
            pos: [0, 1, 2].map(|i| eye[i] - dir[i] * dist),
            yaw,
            pitch,
            fov_deg: fov,
            ..Camera::default()
        }
    }

    /// The local player as a model, for F5 views.
    fn self_draw(&self, frac: f64) -> Option<EntityDraw> {
        let p = self.game.player.as_ref()?;
        let f = frac as f32;
        let lerp = |prev: f64, now: f64| prev + (now - prev) * frac;
        let slot = |i: usize| self.game.inv.player[i].as_ref().map_or(0, |s| s.id);
        Some(EntityDraw {
            class: Class::Player { slim: false },
            pos: [
                lerp(self.game.prev_pos[0], p.pos[0]),
                lerp(self.game.prev_pos[1], p.pos[1]),
                lerp(self.game.prev_pos[2], p.pos[2]),
            ],
            yaw: p.yaw,
            head_yaw: p.yaw,
            pitch: self.game.pitch,
            limb_swing: self.limbs.swing - self.limbs.amount * (1.0 - f),
            limb_swing_amount: self.limbs.prev_amount
                + (self.limbs.amount - self.limbs.prev_amount) * f,
            swing: self.game.swing.progress(f),
            age: self.game.ticks as f32 + f,
            sneaking: self.input.sneaking(),
            hurt: false,
            death_time: 0.0,
            invisible: false,
            burning: self.game.burning(),
            size: (0.6, 1.8),
            held: self.game.inv.held_stack().as_ref().map_or(0, |s| s.id),
            armor: [slot(8), slot(7), slot(6), slot(5)],
            name: None,
            skin: None,
            eye_height: eye_height(self.input.sneaking()) as f32,
            always_name: false,
            pointed: false,
            ridden: false,
            child: false,
        })
    }

    fn draw(&mut self) {
        let until = self.next_tick.saturating_duration_since(Instant::now());
        let frac = (1.0 - until.as_secs_f64() / TICK.as_secs_f64()).clamp(0.0, 1.0);
        let cam = self.camera(frac);
        let eye = self.eye(frac);
        let self_model = (self.perspective != 0)
            .then(|| self.self_draw(frac))
            .flatten();
        let pointed = self.entity_target().map(|(id, _)| id);
        let skins = &mut self.skins;
        let mut renderer = self.renderer.as_mut();
        let ridden: std::collections::HashSet<i32> = self
            .game
            .entities
            .iter()
            .filter_map(|e| e.vehicle)
            .chain(self.game.vehicle)
            .collect();
        let mut draws: Vec<EntityDraw> = self
            .game
            .entities
            .iter()
            .map(|e| {
                let skin = e.uuid.filter(|_| e.kind == Kind::Player).and_then(|u| {
                    renderer
                        .as_deref_mut()
                        .and_then(|r| skins.show(r, u, e.textures.as_deref()))
                });
                let slim = skin.and_then(|u| skins.slim(u));
                entity_draw(
                    e,
                    frac,
                    skin,
                    slim,
                    pointed == Some(e.id),
                    ridden.contains(&e.id),
                )
            })
            .collect();
        draws.extend(self_model);
        let (quads, textures, effects) = self.overlay(frac, eye, &cam, &draws);
        self.run_effects(effects);
        let held = self
            .game
            .inv
            .held_stack()
            .as_ref()
            .map(|s| (s.id, s.damage));
        let lowered = 1.0 - (self.equip.1 + (self.equip.0 - self.equip.1) * frac as f32);
        let swing = self.game.swing.progress(frac as f32);
        let using = self.using(frac as f32);
        let fluid = self.fluid_at(cam.pos);
        let Some(r) = &mut self.renderer else { return };
        for (name, png) in textures {
            r.register_texture(&name, &png);
        }
        self.fps.0 += 1;
        if self.fps.2.elapsed() >= Duration::from_secs(1) {
            self.fps = (0, self.fps.0, Instant::now());
        }
        let (rain, thunder) = self.game.weather.strengths(frac as f32);
        r.set_environment(Environment {
            world_time: self.game.world_time,
            ticks: self.game.ticks,
            partial: frac as f32,
            rain,
            thunder,
            flash: self.game.bolts.flash(frac as f32),
            render_distance_chunks: self.ui.settings.render_distance as u32,
            gamma: self.ui.settings.gamma,
            fluid,
        });
        r.set_lightning(self.game.bolts.shapes());
        let hand_item = self.equip.2.or(held);
        r.set_hand(
            (!self.hide_hud && self.perspective == 0 && self.game.player.is_some()).then(|| Hand {
                held: hand_item.map(|(id, damage)| HeldKind::Item { id, damage }),
                swing,
                lowered,
                using,
            }),
        );
        r.set_on_fire(self.perspective == 0 && self.game.player.is_some() && self.game.burning());
        r.set_entities(&draws);
        r.set_selection(eye.and_then(|e| self.game.selection(e)));
        r.set_cracks(self.game.cracks());
        let verts = self
            .particles
            .vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, frac as f32);
        r.set_particles(verts);
        r.render(&cam, &quads);
        if let Some(shot) = r.take_screenshot() {
            self.save_screenshot(shot);
        }
    }

    /// A screen a mod opened is current in the JVM: it owns the keys, the mouse and the cursor. Only in a world,
    /// where the mod frame is drawn.
    fn mod_screen(&self) -> bool {
        self.in_world() && self.mods.as_ref().is_some_and(|m| m.screen_open())
    }

    fn in_world(&self) -> bool {
        self.ui.in_world && self.game.player.is_some()
    }

    /// A menu, the container or a mod screen is open: input goes to it, and the cursor is free.
    fn modal(&self) -> bool {
        self.ui.is_open() || self.container.is_some() || self.mod_screen()
    }

    /// A menu, the container, a mod screen or chat is open: vanilla routes input to it, not to the game.
    fn screen_open(&self) -> bool {
        self.modal() || self.input.chat_input().is_some()
    }

    /// The F3 left column, `None` without a player.
    fn debug_lines(&self, eye: Option<[f64; 3]>) -> Option<Vec<String>> {
        self.game.player.as_ref().map(|p| {
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
                looking_at: eye.and_then(|e| self.game.target(e)).map(|h| h.pos),
                reduced: self.reduced_debug(),
                ..Default::default()
            })
        })
    }

    /// The HUD, the open container and the menu screens as quads, plus the textures they
    /// registered and the effects they raised.
    fn overlay(
        &mut self,
        frac: f64,
        eye: Option<[f64; 3]>,
        cam: &Camera,
        draws: &[EntityDraw],
    ) -> Overlay {
        let Some(r) = &self.renderer else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let (w, h) = r.size();
        let scale = gui::GuiScale::new(
            w,
            h,
            u32::from(self.ui.settings.gui_scale),
            self.ui.settings.force_unicode,
        );
        let factor = scale.factor.max(1) as f64;
        let (mx, my) = (
            (self.cursor.0 / factor) as i32,
            (self.cursor.1 / factor) as i32,
        );
        let mut g = gui::Gui::new(scale, r.font(), 0);
        self.draw_screen_effects(&mut g, (w, h), frac as f32);
        let m = Fm(g.metrics());
        let screen_open = self.modal();
        let in_world = self.in_world();
        let show_hud =
            in_world && !(self.hide_hud && !screen_open && self.input.chat_input().is_none());
        if show_hud {
            draw_name_tags(&mut g, &self.game, cam, draws, (w, h), factor as f32);
            let debug = self.debug_lines(eye);
            let status = status_of(&self.game, self.input.sneaking());
            self.hud.selected_slot = i32::from(self.game.inv.held);
            let inv = &self.game.inv;
            let hotbar = self.icons.as_ref().map(|icons| gui::HotbarItems {
                slots: std::array::from_fn(|i| &inv.player[36 + i]),
                icons,
                bob: self.game.bob,
            });
            self.hud.draw(
                &mut g,
                &gui::Frame {
                    partial_ticks: frac as f32,
                    chat_input: self.input.chat_input(),
                    debug: debug
                        .as_deref()
                        .filter(|_| self.show_debug)
                        .map(|l| (l, &[][..])),
                    reduced_debug: self.reduced_debug(),
                    fancy: self.ui.settings.fancy_graphics,
                    brightness: 1.0,
                    player_view: self.game.gamemode != 3,
                    status: Some(&status),
                    hotbar,
                    player_list: self.input.player_list_held(),
                    held_item_tooltips: self.ui.settings.held_item_tooltips,
                },
            );
        }
        let mut mod_textures = Vec::new();
        if let Some(mods) = &self.mods {
            mods.set_view(
                w as i32,
                h as i32,
                i32::from(self.ui.settings.gui_scale),
                self.ui.settings.force_unicode,
                self.screen_open(),
                (self.cursor.0 as i32, self.cursor.1 as i32),
            );
            if show_hud {
                mods.draw(&mut g, &mut mod_textures);
            }
        }
        let mut c = gui::GuiCanvas::new(&mut g);
        if let (Some(cs), Some(icons)) = (&mut self.container, &self.icons) {
            cs.draw(
                &mut c,
                &self.game.inv,
                icons,
                mx,
                my,
                self.ui.settings.advanced_item_tooltips,
            );
        }
        let effects = self.ui.draw(&m, &mut c, mx, my);
        let mut textures = std::mem::take(&mut c.pending_textures);
        textures.extend(mod_textures);
        (g.into_quads(), textures, effects)
    }
}

/// `Render.renderLivingLabel`: a 25% black plate and the name at vanilla's world scale. Vanilla draws the text twice,
/// faint without the depth test and solid with it; the overlay has no depth buffer, so a block on the line of
/// sight to the tag stands in for the depth test.
fn draw_name_tags(
    g: &mut gui::Gui,
    game: &Game,
    cam: &Camera,
    draws: &[EntityDraw],
    (w, h): (u32, u32),
    factor: f32,
) {
    let Some(player) = &game.player else { return };
    let vp = render::view_proj(cam, w as f32 / h as f32);
    for tag in render::nametags(draws, player.pos, &vp, cam.fov_deg, w as f32, h as f32) {
        let half = g.width(&tag.text) / 2;
        g.push();
        g.translate(tag.screen[0] / factor, tag.screen[1] / factor);
        g.scale(tag.scale / factor, tag.scale / factor);
        g.fill(-half - 1, -1, half + 1, 8, 0x4000_0000);
        g.text_plain(&tag.text, -half as f32, 0.0, 0x20FF_FFFF);
        if !tag.sneaking && !game.line_blocked(cam.pos, tag.anchor) {
            g.text_plain(&tag.text, -half as f32, 0.0, 0xFFFF_FFFF);
        }
        g.pop();
    }
}

/// `GuiIngame.renderPlayerStats` inputs from the local player.
fn status_of(game: &Game, _sneaking: bool) -> Status {
    let armor_points: i32 = (5..9)
        .filter_map(|i| game.inv.player[i].as_ref())
        .map(|s| armor_points(s.id))
        .sum();
    Status {
        health: game.health,
        food: game.food,
        saturation: game.saturation,
        armor: armor_points,
        xp_bar: game.xp_bar,
        xp_level: game.xp_level,
        hardcore: game.hardcore,
        creative: game.gamemode == 1,
        spectator: game.gamemode == 3,
        ..Status::default()
    }
}

/// Armor bar points of an armor item (`ItemArmor.damageReduceAmount`); 0 for anything else.
/// Ids 298 to 317 are leather, chain, iron, diamond and gold: helmet, chestplate, leggings, boots.
fn armor_points(id: i16) -> i32 {
    const POINTS: [i32; 20] = [1, 3, 2, 1, 2, 5, 4, 1, 2, 6, 5, 2, 3, 8, 6, 3, 2, 5, 3, 1];
    usize::try_from(id - 298)
        .ok()
        .and_then(|i| POINTS.get(i).copied())
        .unwrap_or(0)
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Minecraft 1.8.9");
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        self.renderer = Some(Renderer::new(
            window.clone(),
            self.args.assets_dir.as_deref(),
        ));
        self.load_assets();
        self.load_sound();
        if let Some(cfg) = ferrocraft_modhost::Config::from_env() {
            match ferrocraft_modhost::ModHost::start(&cfg) {
                Ok(host) => self.mods = Some(host),
                Err(e) => crate::log::line(&format!("mod host did not start: {e}")),
            }
        }
        // A child process (loader, terminal) starts unfocused on macOS: no keys until focused.
        window.focus_window();
        self.window = Some(window);
        self.apply_settings();
        self.start_screen();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let screen = self.screen_open();
        self.route_window_event(&event);
        if let Some(mods) = self
            .mods
            .as_ref()
            .filter(|m| self.in_world() || !m.screen_open())
        {
            mods.window_event(&event, screen);
        }
        match event {
            WindowEvent::CloseRequested => {
                crate::log::line("window close requested");
                el.exit();
            }
            WindowEvent::Resized(s) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(s.width, s.height);
                }
                self.resize_screens();
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
        let dy = if self.ui.settings.invert_mouse {
            -dy
        } else {
            dy
        };
        self.game.look(dx, dy);
        self.handle_actions();
        self.run_ticks();
        self.sync_cursor();
        if self.quit || self.deadline.is_some_and(|d| Instant::now() >= d) {
            el.exit();
        }
        if self.pace_frame(el) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

/// `skin` and `slim` come from a downloaded skin; without one the player has the default skin
/// that `DefaultPlayerSkin` picks from the uuid.
fn entity_draw(
    e: &Entity,
    partial: f64,
    skin: Option<u128>,
    slim: Option<bool>,
    pointed: bool,
    ridden: bool,
) -> EntityDraw {
    let class = match e.kind {
        Kind::Player => Class::Player {
            slim: slim.unwrap_or_else(|| {
                e.uuid.is_some_and(|u| {
                    (u as u32 ^ (u >> 32) as u32 ^ (u >> 64) as u32 ^ (u >> 96) as u32) & 1 == 1
                })
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
        size: e.size(),
        held: e.equipment[0].map_or(0, |i| i.id),
        armor: [1, 2, 3, 4].map(|s| e.equipment[s].map_or(0, |i| i.id)),
        name: e.name.clone(),
        skin,
        burning: e.burning(),
        eye_height: e.eye_height(),
        always_name: e.always_name(),
        pointed,
        ridden,
        child: e.is_child(),
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

/// Builds a screen with no arguments, for effects that open one.
fn boxed<S: Screen + 'static>(s: S) -> Box<dyn Screen> {
    Box::new(s)
}

/// `yyyy-MM-dd_HH.mm.ss` in UTC, the vanilla screenshot file name.
fn screenshot_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}.{:02}.{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
