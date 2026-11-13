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
mod pack;
mod play;
mod screen;

use crate::args::Args;
use crate::game::{eye_height, Game};
use crate::link::Link;
use crate::skins::PlayerSkins;
use crate::ui::{Fm, Ui};
use gui::inventory::icons::Icons;
use gui::inventory::ContainerScreen;
use gui::screens::{languages, Screen, Texts};
use gui::status::Status;
use input_audio::sound::index::AssetIndex;
use input_audio::sound::Sound;
use input_audio::InputState;
use render::{
    AssetSource, Camera, CapeSway, ChunkBlocks, Class, EntityDraw, Environment, Fluid, Hand,
    HeldKind, Look, Particles, Renderer, TeamTag, Thrower, UseAction, Using,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};
use world::chunk::Chunk;
use world::effects::id as Fx;
use world::entities::{flag, look_vector, Entity, Kind};

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

/// Item shown in the hand: id, damage, glint and leather `display.color`.
type HandItem = (i16, i16, bool, Option<i32>);

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
    /// The resource pack the server sent (S48): on top of the selected packs until the world is left.
    server_pack: Option<std::path::PathBuf>,
    /// `host:port` of the server last connected to, for the mod host's mirror.
    server: String,
    /// `ServerData.getResourceMode` of that server: `None` asks, `Some(true)` accepts, `Some(false)` declines.
    pack_mode: Option<bool>,
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
    /// `GameRenderer.shaderIndex` into `render::SHADERS`; its length means none.
    shader_index: usize,
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
    /// What the mods left of the world values at the last frame's start.
    mod_view: ferrocraft_modhost::Overrides,
    view_bob: play::ViewBob,
    /// When the last frame was shown, for the `maxFps` limit.
    last_frame: Instant,
    /// `ItemRenderer.equippedProgress` and its previous value, with the item shown.
    equip: (f32, f32, Option<HandItem>),
    /// `GuiDownloadingTerrain` stays until the first position packet.
    awaiting_terrain: bool,
    /// A server pack offer that arrived before that screen closed; the prompt opens once it has.
    pending_pack: Option<(String, String)>,
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

/// `view_ray`'s eye, yaw, pitch, viewer id and its vehicle.
type ViewRay = ([f64; 3], f32, f32, i32, Option<i32>);

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
            server_pack: None,
            server: String::new(),
            pack_mode: None,
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
            shader_index: render::SHADERS.len(),
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
            mod_view: ferrocraft_modhost::Overrides::default(),
            view_bob: play::ViewBob::default(),
            last_frame: Instant::now(),
            equip: (1.0, 1.0, None),
            awaiting_terrain: false,
            pending_pack: None,
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
        self.load_texts();
        let Some(mut src) = self.asset_source() else {
            return;
        };
        self.icons = Some(Icons::new(|name| {
            src.read(&format!("assets/minecraft/{name}"))
        }));
    }

    /// `LanguageManager.onResourceManagerReload`: `en_US` with the chosen language over it, the
    /// splashes and the `pack.mcmeta` language list. Only `en_US` is in the client jar; the other
    /// languages and `pack.mcmeta` come from the launcher's asset index.
    fn load_texts(&mut self) {
        let mut src = self.asset_source();
        let index = self.asset_index();
        let file = |name: &str| {
            index
                .as_ref()?
                .path(name)
                .and_then(|p| std::fs::read(p).ok())
        };
        let mut lang = merged_lang(&file, src.as_mut(), "en_US");
        if self.ui.settings.lang != "en_US" {
            lang.push('\n');
            lang += &merged_lang(&file, src.as_mut(), &self.ui.settings.lang);
        }
        let splashes = src
            .as_mut()
            .and_then(|s| s.read("assets/minecraft/texts/splashes.txt"))
            .or_else(|| file("minecraft/texts/splashes.txt"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let mut texts = Texts::parse(&lang, &splashes);
        let meta = file("pack.mcmeta");
        texts.languages = meta.as_deref().map_or_else(Vec::new, languages);
        // `DefaultResourcePack`: `pack.png` from the jar, the description from the asset index.
        let desc = meta.map_or_else(String::new, |b| gui::screens::default_description(&b));
        let jar = self.args.assets_dir.as_deref().and_then(AssetSource::open);
        texts.default_pack = (jar.and_then(|mut j| j.read("pack.png")), desc);
        self.ui.texts = texts;
    }

    fn asset_index(&self) -> Option<AssetIndex> {
        [
            Some(self.args.game_dir.as_path()),
            self.args.game_dir.parent(),
        ]
        .into_iter()
        .flatten()
        .find_map(|d| AssetIndex::load(d, "1.8.9").ok())
    }

    /// `Minecraft.refreshResources`'s resource manager: the selected packs, top first, over the jar.
    pub(super) fn asset_source(&self) -> Option<AssetSource> {
        // `ResourcePackRepository.setServerResourcePack`: the server pack is the top layer.
        let packs: Vec<_> = self
            .server_pack
            .iter()
            .cloned()
            .chain(gui::screens::selected_paths(
                &self.args.game_dir,
                &self.ui.settings,
            ))
            .collect();
        AssetSource::layered(&packs, self.args.assets_dir.as_deref()?)
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
        let (fullscreen, vsync, unicode) = (s.fullscreen, s.vsync, self.ui.unicode());
        let vsync = std::env::var("FERROCRAFT_VSYNC").map_or(vsync, |v| v != "0");
        let unicode = self
            .renderer
            .as_mut()
            .is_some_and(|r| r.set_force_unicode(unicode));
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
            ..Default::default()
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

    /// `renderViewEntity`'s eyes and look, which `getMouseOver` casts from: the camera entity,
    /// else the local player. Returns the eye, yaw, pitch, viewer id and its vehicle.
    pub(super) fn view_ray(&self, frac: f64) -> Option<ViewRay> {
        let Some(e) = self.game.camera.and_then(|id| self.game.entities.get(id)) else {
            let p = self.game.player.as_ref()?;
            let (id, vehicle) = (self.game.entity_id, self.game.vehicle);
            return Some((self.eye(frac)?, p.yaw, self.game.pitch, id, vehicle));
        };
        let [x, y, z] = e.lerp_pos(frac);
        let (yaw, pitch) = e.lerp_look(frac as f32);
        let eye = [x, y + f64::from(e.eye_height()), z];
        Some((eye, yaw, pitch, e.id, e.vehicle))
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
        Some(Using {
            action: use_action(id),
            timer: self.use_ticks as f32 - frac + 1.0,
        })
    }

    /// A remote player's held model (cast rod) and a bobber's line to its thrower's rod tip.
    fn held_and_line(&self, e: &Entity, d: &mut EntityDraw, frac: f64) {
        if e.kind == Kind::Player {
            let cast = self.casting(e.id);
            let drawn = e.use_ticks.map_or(0, |t| t as i32);
            d.held_damage =
                world::inventory::items::held_variant(d.held, d.held_damage, drawn, cast);
        }
        if e.kind == Kind::Object(90) {
            d.line = self.thrower(e.owner, frac).map(|t| t.rod_tip());
        }
    }

    /// `PlayerEntity.fishingBobber != null`: a bobber of this player is out.
    fn casting(&self, id: i32) -> bool {
        self.game
            .entities
            .iter()
            .any(|e| e.kind == Kind::Object(90) && e.owner == id)
    }

    /// The `damage` the held item model is chosen by: bow draw stage and cast rod
    /// (`ItemRenderer.renderItemInHand`).
    fn held_variant(&self, id: i16, damage: i16) -> i16 {
        let drawn = if self.use_ticks > 0 {
            72_000 - self.use_ticks
        } else {
            0
        };
        world::inventory::items::held_variant(id, damage, drawn, self.casting(self.game.entity_id))
    }

    /// How far into the current tick the frame is, as vanilla's partial tick.
    fn tick_frac(&self) -> f64 {
        let until = self.next_tick.saturating_duration_since(Instant::now());
        (1.0 - until.as_secs_f64() / TICK.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// Ask the mods once per drawn frame for the values they change (kept in `mod_view` for the frame to draw)
    /// and for what their look hook leaves of this frame's mouse look, in degrees. Vanilla look without mods
    /// or when the JVM is late.
    fn mod_look(&mut self, dx: f32, dy: f32) -> (f32, f32) {
        self.mod_view = ferrocraft_modhost::Overrides::default();
        let (Some(mods), Some(p)) = (&self.mods, &self.game.player) else {
            return (dx, dy);
        };
        let frac = self.tick_frac();
        let (rain, thunder) = self.game.weather.strengths(frac as f32);
        let state = ferrocraft_modhost::WorldState {
            partial: frac as f32,
            yaw: p.yaw,
            pitch: self.game.pitch,
            look: (dx, dy),
            perspective: self.perspective,
            world_time: self.game.world_time,
            rain,
            thunder,
            fov: self.ui.settings.fov,
            fov_mod: (self.fov_mod.1, self.fov_mod.0),
            night_vision: self.game.effects.get(Fx::NIGHT_VISION).map(|e| e.duration),
        };
        self.mod_view = mods.overrides(state).unwrap_or_default();
        self.mod_view.look.unwrap_or((dx, dy))
    }

    fn view_camera(&self, frac: f64) -> Camera {
        let fov = self.mod_view.fov.unwrap_or(
            self.ui.settings.fov
                * (self.fov_mod.1 + (self.fov_mod.0 - self.fov_mod.1) * frac as f32),
        );
        let ray = self.view_ray(frac).filter(|_| self.game.player.is_some());
        let Some((eye, yaw, pitch, ..)) = ray else {
            return Camera {
                pos: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                fov_deg: fov,
                ..Camera::default()
            };
        };
        let looked = self.mod_view.camera.filter(|_| self.game.camera.is_none());
        let (yaw, pitch) = looked.map_or((yaw, pitch), |c| (c.yaw, c.pitch));
        let max_dist = looked
            .filter(|c| c.distance > 0.0)
            .map_or(THIRD_PERSON_DISTANCE, |c| f64::from(c.distance));
        if self.perspective == 0 {
            return Camera {
                pos: eye,
                yaw,
                pitch,
                fov_deg: fov,
                ..Camera::default()
            };
        }
        let (yaw, pitch) = if self.perspective == 2 {
            (yaw + 180.0, -pitch)
        } else {
            (yaw, pitch)
        };
        let dir = look_vector(yaw, pitch);
        let mut dist = 0.0;
        while dist < max_dist {
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

    /// The scoreboard's say on a remote player's tag: the team-formatted name (`getDisplayName`), the team's
    /// visibility for this viewer and the below-name line.
    fn team_tag(&self, e: &Entity, d: &mut EntityDraw) {
        let (Kind::Player, Some(name), sb) = (e.kind, &e.name, &self.hud.scoreboard) else {
            return;
        };
        d.name = Some(sb.format_name(name));
        d.team_tag = TeamTag {
            shown: sb.name_tag_shown(name, &self.args.username),
            below: sb.below_name(name),
        };
    }

    /// The local player as a model, for F5 views.
    /// The player who cast a fishing rod, as `FishingBobberRenderer` reads it.
    fn thrower(&self, id: i32, frac: f64) -> Option<Thrower> {
        let f = frac as f32;
        if id == self.game.entity_id {
            let p = self.game.player.as_ref()?;
            let lerp = |a: f64, b: f64| a + (b - a) * frac;
            return Some(Thrower {
                pos: [
                    lerp(self.game.prev_pos[0], p.pos[0]),
                    lerp(self.game.prev_pos[1], p.pos[1]),
                    lerp(self.game.prev_pos[2], p.pos[2]),
                ],
                yaw: p.yaw,
                pitch: self.game.pitch,
                body_yaw: p.yaw,
                eye_height: eye_height(self.input.sneaking()) as f32,
                sneaking: self.input.sneaking(),
                swing: self.game.swing.progress(f),
                first_person: self.perspective == 0,
            });
        }
        let e = self.game.entities.get(id)?;
        Some(Thrower {
            pos: e.lerp_pos(frac),
            yaw: e.yaw,
            pitch: e.pitch,
            body_yaw: e.yaw,
            eye_height: e.eye_height(),
            sneaking: e.sneaking(),
            swing: e.swing_progress(f),
            first_person: false,
        })
    }

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
            hurt: self.game.hurt_timer > 0 || self.game.health <= 0.0,
            death_time: 0.0,
            invisible: self.game.effects.has(Fx::INVISIBILITY),
            burning: self.game.burning(),
            size: (0.6, 1.8),
            held: self.game.inv.held_stack().as_ref().map_or(0, |s| s.id),
            armor: [slot(8), slot(7), slot(6), slot(5)],
            armor_dye: [8, 7, 6, 5].map(|i| {
                self.game.inv.player[i]
                    .as_ref()
                    .and_then(|s| s.nbt.as_ref()?.dye())
            }),
            armor_glint: [8, 7, 6, 5].map(|i| {
                self.game.inv.player[i]
                    .as_ref()
                    .is_some_and(world::inventory::glint)
            }),
            held_glint: self
                .game
                .inv
                .held_stack()
                .as_ref()
                .is_some_and(world::inventory::glint),
            held_damage: self
                .game
                .inv
                .held_stack()
                .as_ref()
                .map_or(0, |s| self.held_variant(s.id, s.damage)),
            held_dye: self
                .game
                .inv
                .held_stack()
                .as_ref()
                .and_then(|s| s.nbt.as_ref()?.dye()),
            stack: None,
            hover_start: 0.0,
            hang: Default::default(),
            name: None,
            skin: None,
            eye_height: eye_height(self.input.sneaking()) as f32,
            always_name: false,
            team_tag: TeamTag::default(),
            pointed: false,
            ridden: false,
            child: false,
            using: self.using(f).map(|u| u.action),
            view: [0.0; 2],
            line: None,
            look: Look::default(),
        })
    }

    fn draw(&mut self) {
        let frac = self.tick_frac();
        let cam = self.camera(frac);
        let spectated = self.game.camera.and_then(|id| self.game.entities.get(id));
        // `RenderPlayer.doRender`: the user player draws only as `renderViewEntity`.
        let self_model = (self.perspective != 0 && spectated.is_none())
            .then(|| self.self_draw(frac))
            .flatten();
        // `isDrawBlockOutline`: only a player view draws the outline and the highlight.
        let player_view = spectated.is_none_or(|e| e.kind == Kind::Player);
        let ray = self.view_ray(frac).filter(|_| player_view);
        let pointed = ray.and_then(|(eye, yaw, pitch, id, vehicle)| {
            self.entity_pick(eye, yaw, pitch, id, vehicle)
                .map(|(id, _)| id)
        });
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
        for (e, d) in self.game.entities.iter().zip(&mut draws) {
            d.view = [cam.yaw, cam.pitch];
            self.team_tag(e, d);
            self.held_and_line(e, d, frac);
        }
        draws.extend(self_model);
        // `RenderGlobal`: the spectated entity is not drawn from its own eyes.
        if let (0, Some(id)) = (self.perspective, self.game.camera) {
            if let Some(i) = self.game.entities.iter().position(|e| e.id == id) {
                draws.remove(i);
            }
        }
        self.tab_heads();
        let (quads, textures, effects) = self.overlay(frac, &cam, &draws);
        self.run_effects(effects);
        let held = self.game.inv.held_stack().as_ref().map(|s| {
            let dye = s.nbt.as_ref().and_then(|n| n.dye());
            (
                s.id,
                self.held_variant(s.id, s.damage),
                world::inventory::glint(s),
                dye,
            )
        });
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
        let (rain, thunder) = self.mod_view.weather.unwrap_or_else(|| {
            let (rain, thunder) = self.game.weather.strengths(frac as f32);
            (rain, thunder * rain)
        });
        r.set_environment(Environment {
            world_time: self.mod_view.time.unwrap_or(self.game.world_time),
            ticks: self.game.ticks,
            partial: frac as f32,
            rain,
            thunder,
            flash: self.game.bolts.flash(frac as f32),
            render_distance_chunks: self.ui.settings.render_distance as u32,
            gamma: self.ui.settings.gamma,
            fluid,
            clouds: self.ui.settings.clouds as u8,
            blindness: self.game.effects.get(Fx::BLINDNESS).map(|e| e.duration),
            night_vision: self.mod_view.night_vision.unwrap_or_else(|| {
                self.game.effects.get(Fx::NIGHT_VISION).map_or(0.0, |e| {
                    world::effects::night_vision_brightness(e.duration, frac as f32)
                })
            }),
            view_y: self.game.player.as_ref().map_or(64.0, |p| p.pos[1] as f32),
        });
        r.set_lightning(self.game.bolts.shapes());
        r.set_border(self.game.player.as_ref().map(|p| {
            let (border, now) = (&self.game.border, crate::game::now_ms());
            render::Border {
                walls: border.bounds(now),
                viewer: p.pos,
                range: f64::from(self.ui.settings.render_distance) * 16.0,
                closest: border.closest_distance(p.pos[0], p.pos[2], now),
                rgb: border.status().color(),
                phase_ms: (now % 3000) as u32,
            }
        }));
        let hand_item = self.equip.2.or(held);
        r.set_hand(
            (!self.hide_hud
                && self.perspective == 0
                && self.game.player.is_some()
                && self.game.gamemode != 3)
                .then(|| Hand {
                    held: hand_item.map(|(id, damage, _, dye)| HeldKind::Item { id, damage, dye }),
                    swing,
                    lowered,
                    using,
                    glint: hand_item.is_some_and(|h| h.2),
                }),
        );
        r.set_on_fire(
            self.perspective == 0
                && self.game.player.is_some()
                && self.game.gamemode != 3
                && self.game.burning(),
        );
        r.set_entities(&draws);
        r.set_block_entities(cam.pos, self.game.world_time, frac as f32);
        r.set_selection(ray.and_then(|(eye, yaw, pitch, ..)| self.game.selection(eye, yaw, pitch)));
        r.set_cracks(self.game.cracks());
        let verts =
            self.particles
                .vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, frac as f32, |p| {
                    r.light_at(p)
                });
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

    /// `GuiChat.drawScreen`: the tooltip of the hover event under the mouse.
    fn chat_hover(&self, m: &Fm) -> Option<Vec<String>> {
        if self.input.chat_input().is_none() || self.modal() {
            return None;
        }
        let hover = self.chat_piece()?.hover.as_ref()?;
        let advanced = self.ui.settings.advanced_item_tooltips;
        Some(gui::hover::lines(hover, &self.ui.texts, advanced, m))
    }

    /// A menu, the container, a mod screen or chat is open: vanilla routes input to it, not to the game.
    fn screen_open(&self) -> bool {
        self.modal() || self.input.chat_input().is_some()
    }

    /// What HUD mods read from `Minecraft` and `EntityPlayerSP`, as the engine has it now. `None` without a player.
    fn mod_mirror(&self) -> Option<ferrocraft_modhost::Mirror> {
        use ferrocraft_modhost::mirror::{Effect, Stack, SLOTS};
        let (p, g) = (self.game.player.as_ref()?, &self.game);
        let stack = |i: usize| {
            g.inv.player.get(i)?.as_ref().map(|s| Stack {
                id: s.id,
                count: s.count,
                damage: s.damage,
            })
        };
        // mainInventory: the hotbar is window slots 36..45 and comes first; armorInventory runs boots to helmet.
        let slots: [Option<Stack>; SLOTS] = std::array::from_fn(|i| match i {
            0..9 => stack(36 + i),
            9..36 => stack(i),
            _ => stack(44 - i),
        });
        let (x, z) = (p.pos[0].floor() as i32, p.pos[2].floor() as i32);
        let biome = g.store.get(x >> 4, z >> 4).map_or(1, |c| {
            c.biome(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
        });
        Some(ferrocraft_modhost::Mirror {
            fps: self.fps.1 as i32,
            ping: self
                .hud
                .tablist
                .entries
                .iter()
                .find(|e| e.name == self.args.username)
                .map_or(0, |e| e.ping),
            pos: p.pos,
            yaw: p.yaw,
            pitch: g.pitch,
            biome,
            health: g.health,
            held: g.inv.held,
            effects: g
                .effects
                .iter()
                .map(|e| Effect {
                    id: e.id,
                    amplifier: e.amplifier,
                    duration: e.duration,
                })
                .collect(),
            slots,
            server: self.server.clone(),
        })
    }

    /// The F3 left column, `None` without a player.
    fn debug_lines(&self, frac: f64) -> Option<Vec<String>> {
        let ray = self.view_ray(frac);
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
                looking_at: ray
                    .and_then(|(eye, yaw, pitch, ..)| self.game.ray_dist(eye, yaw, pitch))
                    .map(|(h, _)| h.pos),
                reduced: self.reduced_debug(),
                ..Default::default()
            })
        })
    }

    /// `renderPlayerlist` head inputs: uploads the skins that finished downloading and tells each
    /// entry which skin, hat and flip to draw. Only an online connection draws heads.
    fn tab_heads(&mut self) {
        if !self.hud.tablist.encrypted || !self.input.player_list_held() {
            return;
        }
        let Some(r) = self.renderer.as_mut() else {
            return;
        };
        let own_parts = self.ui.settings.model_parts;
        for e in &mut self.hud.tablist.entries {
            if let Some(img) = self.skins.tab_head(e.uuid, e.textures.as_deref()) {
                r.register_image(&crate::skins::tab_texture(e.uuid), &img);
            }
            let tex = crate::skins::tab_texture(e.uuid);
            let parts = if e.name == self.args.username {
                Some(own_parts)
            } else {
                self.game
                    .entities
                    .iter()
                    .find(|p| p.kind == Kind::Player && p.uuid == Some(e.uuid))
                    .map(|p| p.meta.stand.unwrap_or(0x7F))
            };
            e.head = gui::tablist::Head {
                skin: self.skins.has_tab(e.uuid).then_some(tex),
                hat: parts.is_some_and(|p| p & 0x40 != 0),
                upside_down: parts.is_some_and(|p| p & 1 != 0)
                    && matches!(e.name.as_str(), "Dinnerbone" | "Grumm"),
            };
        }
    }

    /// The HUD, the open container and the menu screens as quads, plus the textures they
    /// registered and the effects they raised.
    fn overlay(&mut self, frac: f64, cam: &Camera, draws: &[EntityDraw]) -> Overlay {
        let Some(r) = &self.renderer else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let (w, h) = r.size();
        let scale = gui::GuiScale::new(
            w,
            h,
            u32::from(self.ui.settings.gui_scale),
            self.ui.unicode(),
        );
        let factor = scale.factor.max(1) as f64;
        let (mx, my) = (
            (self.cursor.0 / factor) as i32,
            (self.cursor.1 / factor) as i32,
        );
        self.input.set_gui_size(scale.width, scale.height);
        let mut g = gui::Gui::new(scale, r.font(), 0);
        g.set_bidi(self.ui.bidirectional());
        self.draw_screen_effects(&mut g, (w, h), frac as f32);
        let m = Fm(g.metrics());
        let screen_open = self.modal();
        let in_world = self.in_world();
        let show_hud =
            in_world && !(self.hide_hud && !screen_open && self.input.chat_input().is_none());
        if show_hud {
            draw_name_tags(&mut g, &self.game, cam, draws, (w, h), factor as f32);
            let debug = self.debug_lines(frac);
            let status = status_of(&self.game, self.hud.update_counter);
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
                    chat_input: self.input.chat_field(),
                    debug: debug
                        .as_deref()
                        .filter(|_| self.show_debug)
                        .map(|l| (l, &[][..])),
                    reduced_debug: self.reduced_debug(),
                    fancy: self.ui.settings.fancy_graphics,
                    brightness: 1.0,
                    border_warning: self.game.player.as_ref().map_or(0.0, |p| {
                        let now = crate::game::now_ms();
                        self.game.border.warning(p.pos[0], p.pos[2], now)
                    }),
                    player_view: self.game.gamemode != 3,
                    status: Some(&status),
                    hotbar,
                    player_list: self.input.player_list_held(),
                    now_ms: crate::game::now_ms(),
                    local_player: &self.args.username,
                    held_item_tooltips: self.ui.settings.held_item_tooltips,
                },
            );
        }
        let mut mod_textures = Vec::new();
        if let (Some(mods), Some(m)) = (&self.mods, self.mod_mirror()) {
            mods.set_mirror(m);
        }
        if let Some(mods) = &self.mods {
            mods.set_view(
                w as i32,
                h as i32,
                i32::from(self.ui.settings.gui_scale),
                self.ui.unicode(),
                self.screen_open(),
                (self.cursor.0 as i32, self.cursor.1 as i32),
            );
            if show_hud {
                mods.draw(&mut g, &mut mod_textures);
            }
        }
        let hover = self.chat_hover(&m);
        let mut c = gui::GuiCanvas::new(&mut g);
        if let (Some(cs), Some(icons)) = (&mut self.container, &self.icons) {
            let view = gui::inventory::InventoryView {
                inv: &self.game.inv,
                icons,
                advanced_tooltips: self.ui.settings.advanced_item_tooltips,
                effects: &self.game.effects,
            };
            cs.draw(&mut c, &view, mx, my);
        }
        let effects = self.ui.draw(&m, &mut c, mx, my);
        if let Some(lines) = hover {
            gui::screens::tooltip(&mut c, &lines, mx, my, scale.width, scale.height);
        }
        let mut textures = std::mem::take(&mut c.pending_textures);
        textures.extend(mod_textures);
        (g.into_quads(), textures, effects)
    }
}

/// One language's `.lang` text: the asset index copy, then every pack's copy over it
/// (`getAllResources`).
fn merged_lang(
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
    src: Option<&mut AssetSource>,
    code: &str,
) -> String {
    let base = file(&format!("minecraft/lang/{code}.lang"));
    let packs = src.map(|s| s.read_all(&format!("assets/minecraft/lang/{code}.lang")));
    let parts: Vec<_> = base
        .into_iter()
        .chain(packs.into_iter().flatten())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect();
    parts.join("\n")
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
fn status_of(game: &Game, update_counter: i32) -> Status {
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
        poisoned: game.effects.has(Fx::POISON),
        withered: game.effects.has(Fx::WITHER),
        hunger: game.effects.has(Fx::HUNGER),
        regen_tick: if game.effects.has(Fx::REGENERATION) {
            update_counter % (Status::default().max_health + 5.0).ceil() as i32
        } else {
            -1
        },
        horse_jump: game.riding_saddled_horse().then_some(game.horse_jump.power),
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
        self.renderer = Some(Renderer::new(window.clone(), self.asset_source()));
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
        let due = self.pace_frame(el);
        if due {
            let (dx, dy) = self.input.take_look_delta();
            let dy = if self.ui.settings.invert_mouse {
                -dy
            } else {
                dy
            };
            let (dx, dy) = self.mod_look(dx, dy);
            self.game.look(dx, dy);
        }
        self.handle_actions();
        self.run_ticks();
        self.sync_cursor();
        if self.quit || self.deadline.is_some_and(|d| Instant::now() >= d) {
            el.exit();
        }
        if due {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

/// `Item.getItemUseAction` of an item id, as far as the poses differ.
fn use_action(id: i16) -> UseAction {
    match id {
        261 => UseAction::Bow,
        267 | 268 | 272 | 276 | 283 => UseAction::Block,
        _ => UseAction::Consume,
    }
}

/// Action of the timed item use a remote entity has in progress, from its `USING_ITEM` flag.
fn remote_using(e: &Entity) -> Option<UseAction> {
    (e.flags & flag::USING_ITEM != 0).then(|| use_action(e.equipment[0].map_or(0, |i| i.id)))
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
        Kind::Painting => Class::Painting,
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
        armor_dye: [1, 2, 3, 4].map(|s| e.equipment[s].and_then(|i| i.dye)),
        armor_glint: [1, 2, 3, 4].map(|s| e.equipment[s].is_some_and(|i| i.glint)),
        held_glint: e.equipment[0].is_some_and(|i| i.glint),
        held_damage: e.equipment[0].map_or(0, |i| i.damage),
        held_dye: e.equipment[0].and_then(|i| i.dye),
        stack: e.stack.map(|i| render::item::Stack {
            id: i.id,
            damage: i.damage,
            count: i.count,
            glint: i.glint,
            dye: i.dye,
        }),
        hover_start: (e.id as f32 * 0.618) % std::f32::consts::TAU,
        hang: render::hanging::Hang {
            facing: e.facing,
            title: e.title.clone(),
            rotation: e.meta.rotation.unwrap_or(0),
        },
        name: e.name.clone(),
        skin,
        burning: e.burning(),
        eye_height: e.eye_height(),
        always_name: e.always_name(),
        team_tag: TeamTag::default(),
        pointed,
        ridden,
        child: e.is_child(),
        using: remote_using(e),
        view: [0.0; 2],
        line: None,
        look: Look {
            // Metadata 10 of a player is the skin parts byte; a sheep keeps its fleece in byte 16 and a
            // villager its profession in int 16. A server that never sent the byte shows every part.
            skin_parts: e.meta.stand.unwrap_or(0x7F),
            variant: match e.kind {
                Kind::Mob(91) => e.meta.slime.unwrap_or(0),
                Kind::Mob(120) => e.meta.guardian.map_or(0, |p| p as u8),
                _ => 0,
            },
            cape: Some(CapeSway::default()),
            ..Look::default()
        },
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
    fn biome(&self, x: usize, z: usize) -> u8 {
        self.0.biome(x, z)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regeneration_bounce_wraps_on_max_health_even_at_zero_health() {
        let mut game = Game::new();
        game.effects.add(world::effects::Effect {
            id: Fx::REGENERATION,
            amplifier: 0,
            duration: 100,
            permanent: false,
            ambient: false,
            particles: true,
        });
        // Vanilla: updateCounter % ceil(maxHealth + 5) = % 25, independent of current health.
        for health in [1.0, 0.0, -5.0, -20.0] {
            game.health = health;
            assert_eq!(status_of(&game, 24).regen_tick, 24);
            assert_eq!(status_of(&game, 25).regen_tick, 0);
        }
    }
}
