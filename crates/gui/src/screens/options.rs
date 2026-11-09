//! `GameSettings` with its `options.txt` format, and the option screens: `GuiOptions`,
//! `GuiVideoSettings`, `GuiControls`, `GuiScreenOptionsSounds`, `ScreenChatOptions`, `GuiCustomizeSkin`.

use super::{
    button, centered, default_background, draw_buttons, press, release_all, text, Button, Canvas,
    Ctx, Effect, Mods, Nav, Screen, ScrollList, Texts, WHITE,
};
use std::fmt::Write as _;
use std::io;
use std::path::PathBuf;

/// `SoundCategory`, in `options.txt` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCategory {
    Master,
    Music,
    Record,
    Weather,
    Block,
    Hostile,
    Neutral,
    Player,
    Ambient,
}

impl SoundCategory {
    pub const ALL: [SoundCategory; 9] = [
        Self::Master,
        Self::Music,
        Self::Record,
        Self::Weather,
        Self::Block,
        Self::Hostile,
        Self::Neutral,
        Self::Player,
        Self::Ambient,
    ];

    pub fn name(self) -> &'static str {
        [
            "master", "music", "record", "weather", "block", "hostile", "neutral", "player",
            "ambient",
        ][self as usize]
    }
}

/// `EnumPlayerModelParts` names; bit `1 << index` in `C15PacketClientSettings`.
pub const MODEL_PARTS: [&str; 7] = [
    "cape",
    "jacket",
    "left_sleeve",
    "right_sleeve",
    "left_pants_leg",
    "right_pants_leg",
    "hat",
];

/// `KeyBinding`; codes are LWJGL keys, or `button - 100` for mouse buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyBind {
    pub desc: &'static str,
    pub default: i32,
    pub category: &'static str,
    pub code: i32,
}

const KEYS: [(&str, i32, &str); 33] = [
    ("key.attack", -100, "key.categories.gameplay"),
    ("key.use", -99, "key.categories.gameplay"),
    ("key.forward", 17, "key.categories.movement"),
    ("key.left", 30, "key.categories.movement"),
    ("key.back", 31, "key.categories.movement"),
    ("key.right", 32, "key.categories.movement"),
    ("key.jump", 57, "key.categories.movement"),
    ("key.sneak", 42, "key.categories.movement"),
    ("key.sprint", 29, "key.categories.movement"),
    ("key.drop", 16, "key.categories.gameplay"),
    ("key.inventory", 18, "key.categories.inventory"),
    ("key.chat", 20, "key.categories.multiplayer"),
    ("key.playerlist", 15, "key.categories.multiplayer"),
    ("key.pickItem", -98, "key.categories.gameplay"),
    ("key.command", 53, "key.categories.multiplayer"),
    ("key.screenshot", 60, "key.categories.misc"),
    ("key.togglePerspective", 63, "key.categories.misc"),
    ("key.smoothCamera", 0, "key.categories.misc"),
    ("key.streamStartStop", 64, "key.categories.stream"),
    ("key.streamPauseUnpause", 65, "key.categories.stream"),
    ("key.streamCommercial", 0, "key.categories.stream"),
    ("key.streamToggleMic", 0, "key.categories.stream"),
    ("key.fullscreen", 87, "key.categories.misc"),
    ("key.spectatorOutlines", 0, "key.categories.misc"),
    ("key.hotbar.1", 2, "key.categories.inventory"),
    ("key.hotbar.2", 3, "key.categories.inventory"),
    ("key.hotbar.3", 4, "key.categories.inventory"),
    ("key.hotbar.4", 5, "key.categories.inventory"),
    ("key.hotbar.5", 6, "key.categories.inventory"),
    ("key.hotbar.6", 7, "key.categories.inventory"),
    ("key.hotbar.7", 8, "key.categories.inventory"),
    ("key.hotbar.8", 9, "key.categories.inventory"),
    ("key.hotbar.9", 10, "key.categories.inventory"),
];

/// `GameSettings.Options` that a screen shows, in vanilla order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opt {
    InvertMouse,
    Sensitivity,
    Fov,
    Gamma,
    RenderDistance,
    ViewBobbing,
    Anaglyph,
    FramerateLimit,
    FboEnable,
    RenderClouds,
    Graphics,
    Ao,
    GuiScale,
    Particles,
    ChatVisibility,
    ChatColor,
    ChatLinks,
    ChatOpacity,
    ChatLinksPrompt,
    Fullscreen,
    Vsync,
    Vbo,
    Touchscreen,
    ChatScale,
    ChatWidth,
    ChatHeightFocused,
    ChatHeightUnfocused,
    MipmapLevels,
    BlockAlternatives,
    ReducedDebugInfo,
    EntityShadows,
    RealmsNotifications,
}

impl Opt {
    const ALL: [Opt; 32] = [
        Opt::InvertMouse,
        Opt::Sensitivity,
        Opt::Fov,
        Opt::Gamma,
        Opt::RenderDistance,
        Opt::ViewBobbing,
        Opt::Anaglyph,
        Opt::FramerateLimit,
        Opt::FboEnable,
        Opt::RenderClouds,
        Opt::Graphics,
        Opt::Ao,
        Opt::GuiScale,
        Opt::Particles,
        Opt::ChatVisibility,
        Opt::ChatColor,
        Opt::ChatLinks,
        Opt::ChatOpacity,
        Opt::ChatLinksPrompt,
        Opt::Fullscreen,
        Opt::Vsync,
        Opt::Vbo,
        Opt::Touchscreen,
        Opt::ChatScale,
        Opt::ChatWidth,
        Opt::ChatHeightFocused,
        Opt::ChatHeightUnfocused,
        Opt::MipmapLevels,
        Opt::BlockAlternatives,
        Opt::ReducedDebugInfo,
        Opt::EntityShadows,
        Opt::RealmsNotifications,
    ];

    fn from_id(id: i32) -> Option<Opt> {
        Opt::ALL.get(usize::try_from(id).ok()?).copied()
    }

    pub fn key(self) -> &'static str {
        const KEYS: [&str; 32] = [
            "options.invertMouse",
            "options.sensitivity",
            "options.fov",
            "options.gamma",
            "options.renderDistance",
            "options.viewBobbing",
            "options.anaglyph",
            "options.framerateLimit",
            "options.fboEnable",
            "options.renderClouds",
            "options.graphics",
            "options.ao",
            "options.guiScale",
            "options.particles",
            "options.chat.visibility",
            "options.chat.color",
            "options.chat.links",
            "options.chat.opacity",
            "options.chat.links.prompt",
            "options.fullscreen",
            "options.vsync",
            "options.vbo",
            "options.touchscreen",
            "options.chat.scale",
            "options.chat.width",
            "options.chat.height.focused",
            "options.chat.height.unfocused",
            "options.mipmapLevels",
            "options.blockAlternatives",
            "options.reducedDebugInfo",
            "options.entityShadows",
            "options.realmsNotifications",
        ];
        KEYS[self as usize]
    }

    /// `(min, max, step)` for sliders, `None` for buttons.
    pub fn range(self) -> Option<(f32, f32, f32)> {
        use Opt::*;
        match self {
            Sensitivity | Gamma | ChatOpacity | ChatScale | ChatWidth | ChatHeightFocused
            | ChatHeightUnfocused => Some((0.0, 1.0, 0.0)),
            Fov => Some((30.0, 110.0, 1.0)),
            // 32 on a 64-bit JVM with at least 1 GB, as every current launcher gives.
            RenderDistance => Some((2.0, 32.0, 1.0)),
            FramerateLimit => Some((10.0, 260.0, 10.0)),
            MipmapLevels => Some((0.0, 4.0, 1.0)),
            _ => None,
        }
    }

    fn bool_opt(self) -> bool {
        use Opt::*;
        matches!(
            self,
            InvertMouse
                | ViewBobbing
                | Anaglyph
                | FboEnable
                | ChatColor
                | ChatLinks
                | ChatLinksPrompt
                | Fullscreen
                | Vsync
                | Vbo
                | Touchscreen
                | BlockAlternatives
                | ReducedDebugInfo
                | EntityShadows
                | RealmsNotifications
        )
    }

    /// `normalizeValue`.
    pub fn norm(self, v: f32) -> f32 {
        let (lo, hi, _) = self.range().unwrap_or((0.0, 1.0, 0.0));
        ((self.snap(v) - lo) / (hi - lo)).clamp(0.0, 1.0)
    }

    /// `denormalizeValue`.
    pub fn denorm(self, p: f32) -> f32 {
        let (lo, hi, _) = self.range().unwrap_or((0.0, 1.0, 0.0));
        self.snap(lo + (hi - lo) * p.clamp(0.0, 1.0))
    }

    /// `snapToStepClamp`.
    fn snap(self, v: f32) -> f32 {
        let (lo, hi, step) = self.range().unwrap_or((0.0, 1.0, 0.0));
        let v = if step > 0.0 {
            step * (v / step).round()
        } else {
            v
        };
        v.clamp(lo, hi)
    }
}

/// `GameSettings`: every field `options.txt` holds, with vanilla defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub path: PathBuf,
    pub invert_mouse: bool,
    pub mouse_sensitivity: f32,
    pub fov: f32,
    pub gamma: f32,
    pub saturation: f32,
    pub render_distance: i32,
    pub gui_scale: u8,
    pub particles: i32,
    pub view_bobbing: bool,
    pub anaglyph: bool,
    pub max_fps: i32,
    pub fbo: bool,
    pub difficulty: i32,
    pub fancy_graphics: bool,
    pub ao: i32,
    pub clouds: i32,
    /// Raw JSON lists, written back as read.
    pub resource_packs: String,
    pub incompatible_resource_packs: String,
    pub last_server: String,
    pub lang: String,
    pub chat_visibility: i32,
    pub chat_colors: bool,
    pub chat_links: bool,
    pub chat_links_prompt: bool,
    pub chat_opacity: f32,
    pub snooper: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub vbo: bool,
    pub hide_server_address: bool,
    pub advanced_item_tooltips: bool,
    pub pause_on_lost_focus: bool,
    pub touchscreen: bool,
    pub override_width: i32,
    pub override_height: i32,
    pub held_item_tooltips: bool,
    pub chat_height_focused: f32,
    pub chat_height_unfocused: f32,
    pub chat_scale: f32,
    pub chat_width: f32,
    pub show_inventory_achievement_hint: bool,
    pub mipmap_levels: i32,
    pub stream_bytes_per_pixel: f32,
    pub stream_mic_volume: f32,
    pub stream_system_volume: f32,
    pub stream_kbps: f32,
    pub stream_fps: f32,
    pub stream_compression: i32,
    pub stream_send_metadata: bool,
    pub stream_preferred_server: String,
    pub stream_chat_enabled: i32,
    pub stream_chat_user_filter: i32,
    pub stream_mic_toggle_behavior: i32,
    pub force_unicode: bool,
    pub block_alternatives: bool,
    pub reduced_debug_info: bool,
    pub use_native_transport: bool,
    pub entity_shadows: bool,
    pub realms_notifications: bool,
    pub keys: Vec<KeyBind>,
    /// Indexed by [`SoundCategory`].
    pub volumes: [f32; 9],
    /// Bit `1 << i` for [`MODEL_PARTS`]`[i]`.
    pub model_parts: u8,
}

type Field<T> = fn(&mut Settings) -> &mut T;

const BOOL_KEYS: &[(&str, Field<bool>)] = &[
    ("invertYMouse", |s| &mut s.invert_mouse),
    ("bobView", |s| &mut s.view_bobbing),
    ("anaglyph3d", |s| &mut s.anaglyph),
    ("fboEnable", |s| &mut s.fbo),
    ("fancyGraphics", |s| &mut s.fancy_graphics),
    ("chatColors", |s| &mut s.chat_colors),
    ("chatLinks", |s| &mut s.chat_links),
    ("chatLinksPrompt", |s| &mut s.chat_links_prompt),
    ("snooperEnabled", |s| &mut s.snooper),
    ("fullscreen", |s| &mut s.fullscreen),
    ("enableVsync", |s| &mut s.vsync),
    ("useVbo", |s| &mut s.vbo),
    ("hideServerAddress", |s| &mut s.hide_server_address),
    ("advancedItemTooltips", |s| &mut s.advanced_item_tooltips),
    ("pauseOnLostFocus", |s| &mut s.pause_on_lost_focus),
    ("touchscreen", |s| &mut s.touchscreen),
    ("heldItemTooltips", |s| &mut s.held_item_tooltips),
    ("showInventoryAchievementHint", |s| {
        &mut s.show_inventory_achievement_hint
    }),
    ("streamSendMetadata", |s| &mut s.stream_send_metadata),
    ("forceUnicodeFont", |s| &mut s.force_unicode),
    ("allowBlockAlternatives", |s| &mut s.block_alternatives),
    ("reducedDebugInfo", |s| &mut s.reduced_debug_info),
    ("useNativeTransport", |s| &mut s.use_native_transport),
    ("entityShadows", |s| &mut s.entity_shadows),
    ("realmsNotifications", |s| &mut s.realms_notifications),
];

const F32_KEYS: &[(&str, Field<f32>)] = &[
    ("mouseSensitivity", |s| &mut s.mouse_sensitivity),
    ("gamma", |s| &mut s.gamma),
    ("saturation", |s| &mut s.saturation),
    ("chatOpacity", |s| &mut s.chat_opacity),
    ("chatHeightFocused", |s| &mut s.chat_height_focused),
    ("chatHeightUnfocused", |s| &mut s.chat_height_unfocused),
    ("chatScale", |s| &mut s.chat_scale),
    ("chatWidth", |s| &mut s.chat_width),
    ("streamBytesPerPixel", |s| &mut s.stream_bytes_per_pixel),
    ("streamMicVolume", |s| &mut s.stream_mic_volume),
    ("streamSystemVolume", |s| &mut s.stream_system_volume),
    ("streamKbps", |s| &mut s.stream_kbps),
    ("streamFps", |s| &mut s.stream_fps),
];

const I32_KEYS: &[(&str, Field<i32>)] = &[
    ("renderDistance", |s| &mut s.render_distance),
    ("particles", |s| &mut s.particles),
    ("maxFps", |s| &mut s.max_fps),
    ("overrideHeight", |s| &mut s.override_height),
    ("overrideWidth", |s| &mut s.override_width),
    ("mipmapLevels", |s| &mut s.mipmap_levels),
    ("streamCompression", |s| &mut s.stream_compression),
    ("streamChatEnabled", |s| &mut s.stream_chat_enabled),
    ("streamChatUserFilter", |s| &mut s.stream_chat_user_filter),
    ("streamMicToggleBehavior", |s| {
        &mut s.stream_mic_toggle_behavior
    }),
];

/// Key, whether an empty value is stored, field.
const STRING_KEYS: &[(&str, bool, Field<String>)] = &[
    ("resourcePacks", true, |s| &mut s.resource_packs),
    ("incompatibleResourcePacks", true, |s| {
        &mut s.incompatible_resource_packs
    }),
    ("lastServer", false, |s| &mut s.last_server),
    ("streamPreferredServer", false, |s| {
        &mut s.stream_preferred_server
    }),
];

type Flag = (Opt, fn(&Settings) -> bool, Field<bool>);

/// Button options backed by one `bool` field: getter and toggle target.
const FLAGS: &[Flag] = &[
    (
        Opt::InvertMouse,
        |s| s.invert_mouse,
        |s| &mut s.invert_mouse,
    ),
    (
        Opt::ViewBobbing,
        |s| s.view_bobbing,
        |s| &mut s.view_bobbing,
    ),
    (Opt::Anaglyph, |s| s.anaglyph, |s| &mut s.anaglyph),
    (Opt::FboEnable, |s| s.fbo, |s| &mut s.fbo),
    (Opt::ChatColor, |s| s.chat_colors, |s| &mut s.chat_colors),
    (Opt::ChatLinks, |s| s.chat_links, |s| &mut s.chat_links),
    (
        Opt::ChatLinksPrompt,
        |s| s.chat_links_prompt,
        |s| &mut s.chat_links_prompt,
    ),
    (Opt::Fullscreen, |s| s.fullscreen, |s| &mut s.fullscreen),
    (Opt::Vsync, |s| s.vsync, |s| &mut s.vsync),
    (Opt::Vbo, |s| s.vbo, |s| &mut s.vbo),
    (Opt::Touchscreen, |s| s.touchscreen, |s| &mut s.touchscreen),
    (
        Opt::BlockAlternatives,
        |s| s.block_alternatives,
        |s| &mut s.block_alternatives,
    ),
    (
        Opt::ReducedDebugInfo,
        |s| s.reduced_debug_info,
        |s| &mut s.reduced_debug_info,
    ),
    (
        Opt::EntityShadows,
        |s| s.entity_shadows,
        |s| &mut s.entity_shadows,
    ),
    (
        Opt::RealmsNotifications,
        |s| s.realms_notifications,
        |s| &mut s.realms_notifications,
    ),
];

fn parse_f(v: &str) -> Option<f32> {
    match v {
        "true" => Some(1.0),
        "false" => Some(0.0),
        _ => v.parse().ok(),
    }
}

fn parse_ao(v: &str) -> Option<i32> {
    match v {
        "true" => Some(2),
        "false" => Some(0),
        _ => v.parse().ok(),
    }
}

impl Settings {
    /// Vanilla defaults for `<game_dir>/options.txt`.
    pub fn new(game_dir: PathBuf) -> Settings {
        Settings {
            path: game_dir.join("options.txt"),
            invert_mouse: false,
            mouse_sensitivity: 0.5,
            fov: 70.0,
            gamma: 0.0,
            saturation: 0.0,
            render_distance: 12,
            gui_scale: 0,
            particles: 0,
            view_bobbing: true,
            anaglyph: false,
            max_fps: 120,
            fbo: true,
            difficulty: 2,
            fancy_graphics: true,
            ao: 2,
            clouds: 2,
            resource_packs: "[]".into(),
            incompatible_resource_packs: "[]".into(),
            last_server: String::new(),
            lang: "en_US".into(),
            chat_visibility: 0,
            chat_colors: true,
            chat_links: true,
            chat_links_prompt: true,
            chat_opacity: 1.0,
            snooper: true,
            fullscreen: false,
            vsync: true,
            vbo: false,
            hide_server_address: false,
            advanced_item_tooltips: false,
            pause_on_lost_focus: true,
            touchscreen: false,
            override_width: 0,
            override_height: 0,
            held_item_tooltips: true,
            chat_height_focused: 1.0,
            chat_height_unfocused: 0.443_661_96,
            chat_scale: 1.0,
            chat_width: 1.0,
            show_inventory_achievement_hint: true,
            mipmap_levels: 4,
            stream_bytes_per_pixel: 0.5,
            stream_mic_volume: 1.0,
            stream_system_volume: 1.0,
            stream_kbps: 0.541_284_4,
            stream_fps: 0.316_901_42,
            stream_compression: 1,
            stream_send_metadata: true,
            stream_preferred_server: String::new(),
            stream_chat_enabled: 0,
            stream_chat_user_filter: 0,
            stream_mic_toggle_behavior: 0,
            force_unicode: false,
            block_alternatives: true,
            reduced_debug_info: false,
            use_native_transport: true,
            entity_shadows: true,
            realms_notifications: true,
            keys: KEYS
                .iter()
                .map(|&(desc, default, category)| KeyBind {
                    desc,
                    default,
                    category,
                    code: default,
                })
                .collect(),
            volumes: [1.0; 9],
            model_parts: 0x7f,
        }
    }

    /// `GameSettings.loadOptions`: defaults, then every line of `options.txt` that parses.
    pub fn load(game_dir: PathBuf) -> Settings {
        let mut s = Settings::new(game_dir);
        if let Ok(text) = std::fs::read_to_string(&s.path) {
            for line in text.lines() {
                if s.apply(line).is_none() {
                    eprintln!("Skipping bad option: {line}");
                }
            }
        }
        s
    }

    fn apply(&mut self, line: &str) -> Option<()> {
        let (k, v) = line.split_once(':').unwrap_or((line, ""));
        if let Some((_, g)) = BOOL_KEYS.iter().find(|e| e.0 == k) {
            *g(self) = v == "true";
        } else if let Some((_, g)) = F32_KEYS.iter().find(|e| e.0 == k) {
            *g(self) = parse_f(v)?;
        } else if let Some((_, g)) = I32_KEYS.iter().find(|e| e.0 == k) {
            *g(self) = v.parse().ok()?;
        } else if let Some((_, keep_empty, g)) = STRING_KEYS.iter().find(|e| e.0 == k) {
            if *keep_empty || !v.is_empty() {
                *g(self) = v.to_owned();
            }
        } else {
            self.apply_special(k, v)?;
        }
        Some(())
    }

    fn apply_special(&mut self, k: &str, v: &str) -> Option<()> {
        let i = || v.parse::<i32>().ok();
        match k {
            "fov" => self.fov = parse_f(v)? * 40.0 + 70.0,
            "guiScale" => self.gui_scale = i()?.clamp(0, 255) as u8,
            "difficulty" => self.difficulty = i()?.rem_euclid(4),
            "chatVisibility" => self.chat_visibility = i()?.rem_euclid(3),
            "ao" => self.ao = parse_ao(v)?,
            "renderClouds" => {
                let c = ["false", "fast", "true"].iter().position(|x| *x == v);
                self.clouds = c.map_or(self.clouds, |c| c as i32);
            }
            "lang" => {
                if !v.is_empty() {
                    self.lang = v.split(':').next().unwrap_or(v).to_owned();
                }
            }
            _ => self.apply_prefixed(k, v)?,
        }
        Some(())
    }

    fn apply_prefixed(&mut self, k: &str, v: &str) -> Option<()> {
        if let Some(name) = k.strip_prefix("key_") {
            let code = v.parse::<i32>().ok()?;
            self.keys
                .iter_mut()
                .filter(|kb| kb.desc == name)
                .for_each(|kb| kb.code = code);
        } else if let Some(name) = k.strip_prefix("soundCategory_") {
            let vol = parse_f(v)?;
            if let Some(c) = SoundCategory::ALL.iter().find(|c| c.name() == name) {
                self.volumes[*c as usize] = vol;
            }
        } else if let Some(name) = k.strip_prefix("modelPart_") {
            if let Some(bit) = MODEL_PARTS.iter().position(|p| *p == name) {
                self.set_part(bit, v == "true");
            }
        }
        Some(())
    }

    pub fn set_part(&mut self, bit: usize, on: bool) {
        if on {
            self.model_parts |= 1 << bit;
        } else {
            self.model_parts &= !(1 << bit);
        }
    }

    /// `options.txt` text, line for line as `GameSettings.saveOptions` writes it.
    pub fn to_text(&self) -> String {
        let mut o = String::new();
        let mut w = |k: &str, v: &dyn std::fmt::Display| {
            let _ = writeln!(o, "{k}:{v}");
        };
        let jf = JavaFloat;
        w("invertYMouse", &self.invert_mouse);
        w("mouseSensitivity", &jf.of(self.mouse_sensitivity));
        w("fov", &jf.of((self.fov - 70.0) / 40.0));
        w("gamma", &jf.of(self.gamma));
        w("saturation", &jf.of(self.saturation));
        w("renderDistance", &self.render_distance);
        w("guiScale", &self.gui_scale);
        w("particles", &self.particles);
        w("bobView", &self.view_bobbing);
        w("anaglyph3d", &self.anaglyph);
        w("maxFps", &self.max_fps);
        w("fboEnable", &self.fbo);
        w("difficulty", &self.difficulty);
        w("fancyGraphics", &self.fancy_graphics);
        w("ao", &self.ao);
        match self.clouds {
            0 => w("renderClouds", &"false"),
            1 => w("renderClouds", &"fast"),
            2 => w("renderClouds", &"true"),
            _ => {}
        }
        w("resourcePacks", &self.resource_packs);
        w(
            "incompatibleResourcePacks",
            &self.incompatible_resource_packs,
        );
        w("lastServer", &self.last_server);
        w("lang", &self.lang);
        w("chatVisibility", &self.chat_visibility);
        w("chatColors", &self.chat_colors);
        w("chatLinks", &self.chat_links);
        w("chatLinksPrompt", &self.chat_links_prompt);
        w("chatOpacity", &jf.of(self.chat_opacity));
        w("snooperEnabled", &self.snooper);
        w("fullscreen", &self.fullscreen);
        w("enableVsync", &self.vsync);
        w("useVbo", &self.vbo);
        w("hideServerAddress", &self.hide_server_address);
        w("advancedItemTooltips", &self.advanced_item_tooltips);
        w("pauseOnLostFocus", &self.pause_on_lost_focus);
        w("touchscreen", &self.touchscreen);
        w("overrideWidth", &self.override_width);
        w("overrideHeight", &self.override_height);
        w("heldItemTooltips", &self.held_item_tooltips);
        w("chatHeightFocused", &jf.of(self.chat_height_focused));
        w("chatHeightUnfocused", &jf.of(self.chat_height_unfocused));
        w("chatScale", &jf.of(self.chat_scale));
        w("chatWidth", &jf.of(self.chat_width));
        w(
            "showInventoryAchievementHint",
            &self.show_inventory_achievement_hint,
        );
        w("mipmapLevels", &self.mipmap_levels);
        w("streamBytesPerPixel", &jf.of(self.stream_bytes_per_pixel));
        w("streamMicVolume", &jf.of(self.stream_mic_volume));
        w("streamSystemVolume", &jf.of(self.stream_system_volume));
        w("streamKbps", &jf.of(self.stream_kbps));
        w("streamFps", &jf.of(self.stream_fps));
        w("streamCompression", &self.stream_compression);
        w("streamSendMetadata", &self.stream_send_metadata);
        w("streamPreferredServer", &self.stream_preferred_server);
        w("streamChatEnabled", &self.stream_chat_enabled);
        w("streamChatUserFilter", &self.stream_chat_user_filter);
        w("streamMicToggleBehavior", &self.stream_mic_toggle_behavior);
        w("forceUnicodeFont", &self.force_unicode);
        w("allowBlockAlternatives", &self.block_alternatives);
        w("reducedDebugInfo", &self.reduced_debug_info);
        w("useNativeTransport", &self.use_native_transport);
        w("entityShadows", &self.entity_shadows);
        w("realmsNotifications", &self.realms_notifications);
        for k in &self.keys {
            w(&format!("key_{}", k.desc), &k.code);
        }
        for c in SoundCategory::ALL {
            w(
                &format!("soundCategory_{}", c.name()),
                &jf.of(self.volumes[c as usize]),
            );
        }
        for (i, p) in MODEL_PARTS.iter().enumerate() {
            w(&format!("modelPart_{p}"), &(self.model_parts >> i & 1 == 1));
        }
        o
    }

    pub fn save(&self) -> io::Result<()> {
        std::fs::write(&self.path, self.to_text())
    }

    /// `getOptionFloatValue`.
    pub fn float(&self, o: Opt) -> f32 {
        use Opt::*;
        match o {
            Fov => self.fov,
            Gamma => self.gamma,
            Sensitivity => self.mouse_sensitivity,
            ChatOpacity => self.chat_opacity,
            ChatHeightFocused => self.chat_height_focused,
            ChatHeightUnfocused => self.chat_height_unfocused,
            ChatScale => self.chat_scale,
            ChatWidth => self.chat_width,
            FramerateLimit => self.max_fps as f32,
            MipmapLevels => self.mipmap_levels as f32,
            RenderDistance => self.render_distance as f32,
            _ => 0.0,
        }
    }

    /// `setOptionFloatValue`.
    pub fn set_float(&mut self, o: Opt, v: f32) {
        use Opt::*;
        match o {
            Sensitivity => self.mouse_sensitivity = v,
            Fov => self.fov = v,
            Gamma => self.gamma = v,
            FramerateLimit => self.max_fps = v as i32,
            ChatOpacity => self.chat_opacity = v,
            ChatHeightFocused => self.chat_height_focused = v,
            ChatHeightUnfocused => self.chat_height_unfocused = v,
            ChatWidth => self.chat_width = v,
            ChatScale => self.chat_scale = v,
            MipmapLevels => self.mipmap_levels = v as i32,
            RenderDistance => self.render_distance = v as i32,
            _ => {}
        }
    }

    /// `getOptionOrdinalValue`.
    pub fn flag(&self, o: Opt) -> bool {
        FLAGS.iter().find(|e| e.0 == o).is_some_and(|e| (e.1)(self))
    }

    /// `setOptionValue(option, 1)`: toggle or step a button option.
    pub fn cycle(&mut self, o: Opt) {
        use Opt::*;
        if let Some(e) = FLAGS.iter().find(|e| e.0 == o) {
            *(e.2)(self) ^= true;
            return;
        }
        match o {
            GuiScale => self.gui_scale = self.gui_scale.wrapping_add(1) & 3,
            Particles => self.particles = (self.particles + 1) % 3,
            RenderClouds => self.clouds = (self.clouds + 1) % 3,
            Graphics => self.fancy_graphics ^= true,
            Ao => self.ao = (self.ao + 1) % 3,
            ChatVisibility => self.chat_visibility = (self.chat_visibility + 1) % 3,
            _ => {}
        }
    }

    /// `getKeyBinding`: the button label, such as `FOV: Quake Pro`.
    pub fn label(&self, o: Opt, t: &Texts) -> String {
        let head = format!("{}: ", t.tr(o.key()));
        let tail = if o.range().is_some() {
            self.float_label(o, t)
        } else if o.bool_opt() {
            t.tr(if self.flag(o) {
                "options.on"
            } else {
                "options.off"
            })
        } else {
            self.choice_label(o, t)
        };
        head + &tail
    }

    fn float_label(&self, o: Opt, t: &Texts) -> String {
        use Opt::*;
        let v = self.float(o);
        let n = o.norm(v);
        let ends = |key: &str, at_min: bool, at_max: bool, other: String| {
            [(at_min, "min"), (at_max, "max")]
                .into_iter()
                .find(|e| e.0)
                .map_or(other, |(_, end)| t.tr(&format!("{key}.{end}")))
        };
        let or_off = |off: bool, other: String| {
            ends("options", off, false, other).replace("options.min", "options.off")
        };
        match o {
            Sensitivity => ends(
                "options.sensitivity",
                n == 0.0,
                n == 1.0,
                format!("{}%", (n * 200.0) as i32),
            ),
            Fov => ends("options.fov", v == 70.0, v == 110.0, (v as i32).to_string()),
            FramerateLimit => ends(
                "options.framerateLimit",
                false,
                v == 260.0,
                format!("{} fps", v as i32),
            ),
            Gamma => ends(
                "options.gamma",
                n == 0.0,
                n == 1.0,
                format!("+{}%", (n * 100.0) as i32),
            ),
            ChatOpacity => format!("{}%", (n * 90.0 + 10.0) as i32),
            ChatHeightUnfocused | ChatHeightFocused => {
                format!("{}px", (n * 160.0 + 20.0).floor() as i32)
            }
            ChatWidth => format!("{}px", (n * 280.0 + 40.0).floor() as i32),
            RenderDistance => format!("{} chunks", v as i32),
            MipmapLevels => or_off(v == 0.0, (v as i32).to_string()),
            _ => or_off(n == 0.0, format!("{}%", (n * 100.0) as i32)),
        }
    }

    fn choice_label(&self, o: Opt, t: &Texts) -> String {
        use Opt::*;
        let pick = |keys: &[&str], i: i32| t.tr(keys.get(i as usize).unwrap_or(&keys[0]));
        match o {
            GuiScale => pick(
                &[
                    "options.guiScale.auto",
                    "options.guiScale.small",
                    "options.guiScale.normal",
                    "options.guiScale.large",
                ],
                self.gui_scale as i32,
            ),
            ChatVisibility => pick(
                &[
                    "options.chat.visibility.full",
                    "options.chat.visibility.system",
                    "options.chat.visibility.hidden",
                ],
                self.chat_visibility,
            ),
            Particles => pick(
                &[
                    "options.particles.all",
                    "options.particles.decreased",
                    "options.particles.minimal",
                ],
                self.particles,
            ),
            Ao => pick(
                &["options.ao.off", "options.ao.min", "options.ao.max"],
                self.ao,
            ),
            RenderClouds => pick(
                &[
                    "options.off",
                    "options.graphics.fast",
                    "options.graphics.fancy",
                ],
                self.clouds,
            ),
            Graphics => t.tr(if self.fancy_graphics {
                "options.graphics.fancy"
            } else {
                "options.graphics.fast"
            }),
            _ => String::new(),
        }
    }

    /// `KeyBinding.compareTo` order for the controls list: category, then name, both translated.
    fn sorted_keys(&self, t: &Texts) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.keys.len()).collect();
        idx.sort_by_key(|&i| (t.tr(self.keys[i].category), t.tr(self.keys[i].desc)));
        idx
    }
}

/// Java's `Float.toString`, which `options.txt` floats use.
struct JavaFloat;

impl JavaFloat {
    fn of(&self, v: f32) -> String {
        let a = v.abs();
        if v == 0.0 || (1e-3..1e7).contains(&a) {
            // Debug prints the shortest round-trip digits with a `.0` on whole numbers, as Java does.
            format!("{v:?}")
        } else {
            let s = format!("{v:e}");
            let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
            let m = if m.contains('.') {
                m.to_owned()
            } else {
                format!("{m}.0")
            };
            format!("{m}E{e}")
        }
    }
}

/// `GameSettings.getKeyDisplayString`.
pub fn key_name(code: i32, t: &Texts) -> String {
    if code < 0 {
        return t.fmt("key.mouseButton", &[&(code + 101).to_string()]);
    }
    if code >= 256 {
        return char::from_u32((code - 256) as u32)
            .map_or(String::new(), |c| c.to_uppercase().collect());
    }
    lwjgl_key_name(code).unwrap_or("null").to_owned()
}

/// `org.lwjgl.input.Keyboard.getKeyName`.
fn lwjgl_key_name(code: i32) -> Option<&'static str> {
    const LOW: [&str; 89] = [
        "NONE",
        "ESCAPE",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
        "0",
        "MINUS",
        "EQUALS",
        "BACK",
        "TAB",
        "Q",
        "W",
        "E",
        "R",
        "T",
        "Y",
        "U",
        "I",
        "O",
        "P",
        "LBRACKET",
        "RBRACKET",
        "RETURN",
        "LCONTROL",
        "A",
        "S",
        "D",
        "F",
        "G",
        "H",
        "J",
        "K",
        "L",
        "SEMICOLON",
        "APOSTROPHE",
        "GRAVE",
        "LSHIFT",
        "BACKSLASH",
        "Z",
        "X",
        "C",
        "V",
        "B",
        "N",
        "M",
        "COMMA",
        "PERIOD",
        "SLASH",
        "RSHIFT",
        "MULTIPLY",
        "LMENU",
        "SPACE",
        "CAPITAL",
        "F1",
        "F2",
        "F3",
        "F4",
        "F5",
        "F6",
        "F7",
        "F8",
        "F9",
        "F10",
        "NUMLOCK",
        "SCROLL",
        "NUMPAD7",
        "NUMPAD8",
        "NUMPAD9",
        "SUBTRACT",
        "NUMPAD4",
        "NUMPAD5",
        "NUMPAD6",
        "ADD",
        "NUMPAD1",
        "NUMPAD2",
        "NUMPAD3",
        "NUMPAD0",
        "DECIMAL",
        "",
        "",
        "",
        "F11",
        "F12",
    ];
    const HIGH: [(i32, &str); 45] = [
        (100, "F13"),
        (101, "F14"),
        (102, "F15"),
        (103, "F16"),
        (104, "F17"),
        (105, "F18"),
        (112, "KANA"),
        (113, "F19"),
        (121, "CONVERT"),
        (123, "NOCONVERT"),
        (125, "YEN"),
        (141, "NUMPADEQUALS"),
        (144, "CIRCUMFLEX"),
        (145, "AT"),
        (146, "COLON"),
        (147, "UNDERLINE"),
        (148, "KANJI"),
        (149, "STOP"),
        (150, "AX"),
        (151, "UNLABELED"),
        (156, "NUMPADENTER"),
        (157, "RCONTROL"),
        (167, "SECTION"),
        (179, "NUMPADCOMMA"),
        (181, "DIVIDE"),
        (183, "SYSRQ"),
        (184, "RMENU"),
        (196, "FUNCTION"),
        (197, "PAUSE"),
        (199, "HOME"),
        (200, "UP"),
        (201, "PRIOR"),
        (203, "LEFT"),
        (205, "RIGHT"),
        (207, "END"),
        (208, "DOWN"),
        (209, "NEXT"),
        (210, "INSERT"),
        (211, "DELETE"),
        (218, "CLEAR"),
        (219, "LMETA"),
        (220, "RMETA"),
        (221, "APPS"),
        (222, "POWER"),
        (223, "SLEEP"),
    ];
    let low = LOW.get(code as usize).filter(|n| !n.is_empty()).copied();
    low.or_else(|| HIGH.iter().find(|(c, _)| *c == code).map(|(_, n)| *n))
}

/// A 150-wide option button: a slider or a cycling button, id = option ordinal.
fn opt_button(o: Opt, x: i32, y: i32, ctx: &Ctx) -> Button {
    let label = ctx.settings.label(o, ctx.texts);
    match o.range() {
        Some(_) => Button::slider(o as i32, x, y, 150, o.norm(ctx.settings.float(o)), label),
        None => Button::new(o as i32, x, y, 150, 20, label),
    }
}

/// Apply a pressed or dragged option button to the settings (`GuiOptionSlider`, `GuiOptionButton`).
fn opt_pressed(b: &mut Button, ctx: &mut Ctx) {
    let Some(o) = Opt::from_id(b.id) else { return };
    match b.slider {
        Some(s) => {
            let v = o.denorm(s.pos);
            ctx.settings.set_float(o, v);
            b.slider = Some(super::Slider {
                pos: o.norm(v),
                dragging: s.dragging,
            });
        }
        None => {
            ctx.settings.cycle(o);
            ctx.save_settings();
        }
    }
    b.label = ctx.settings.label(o, ctx.texts);
}

/// Dragged option sliders follow the mouse while drawing, as `GuiOptionSlider.mouseDragged`.
fn opt_drag(buttons: &mut [Button], mx: i32, ctx: &mut Ctx) {
    for b in buttons.iter_mut() {
        if b.drag(mx).is_some() && Opt::from_id(b.id).is_some() {
            opt_pressed(b, ctx);
        }
    }
}

fn release_sliders(buttons: &mut [Button], ctx: &mut Ctx) {
    if buttons.iter().any(|b| b.slider.is_some_and(|s| s.dragging)) {
        ctx.save_settings();
    }
    release_all(buttons);
}

/// Grid slot `i` of the two-column option layouts.
fn grid(ctx: &Ctx, i: i32, top: i32) -> (i32, i32) {
    (ctx.width / 2 - 155 + i % 2 * 160, top + 24 * (i >> 1))
}

/// A two-column option page with a Done button: `ScreenChatOptions`, `GuiCustomizeSkin`, `GuiScreenOptionsSounds`.
pub struct OptionPage {
    parent: Option<Box<dyn Screen>>,
    kind: Page,
    buttons: Vec<Button>,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Chat,
    Skin,
    Sounds,
}

const SKIN_PART: i32 = 1000;
const SOUND: i32 = 2000;
const DONE: i32 = 200;

impl OptionPage {
    fn new(parent: Box<dyn Screen>, kind: Page) -> OptionPage {
        OptionPage {
            parent: Some(parent),
            kind,
            buttons: Vec::new(),
        }
    }

    fn sound_label(c: SoundCategory, ctx: &Ctx) -> String {
        let v = ctx.settings.volumes[c as usize];
        let tail = if v == 0.0 {
            ctx.tr("options.off")
        } else {
            format!("{}%", (v * 100.0) as i32)
        };
        format!("{}: {tail}", ctx.tr(&format!("soundCategory.{}", c.name())))
    }

    fn skin_label(bit: usize, ctx: &Ctx) -> String {
        let on = ctx.settings.model_parts >> bit & 1 == 1;
        let name = ctx.tr(&format!("options.modelPart.{}", MODEL_PARTS[bit]));
        format!(
            "{name}: {}",
            ctx.tr(if on { "options.on" } else { "options.off" })
        )
    }

    fn back(&mut self) -> Option<Nav> {
        self.parent.take().map(Nav::Open)
    }
}

impl Screen for OptionPage {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons.clear();
        match self.kind {
            Page::Chat => {
                use Opt::*;
                let opts = [
                    ChatVisibility,
                    ChatColor,
                    ChatLinks,
                    ChatOpacity,
                    ChatLinksPrompt,
                    ChatScale,
                    ChatHeightFocused,
                    ChatHeightUnfocused,
                    ChatWidth,
                    ReducedDebugInfo,
                ];
                for (i, o) in opts.iter().enumerate() {
                    let (x, y) = grid(ctx, i as i32, h / 6);
                    self.buttons.push(opt_button(*o, x, y, ctx));
                }
                self.buttons.push(Button::wide(
                    DONE,
                    w / 2 - 100,
                    h / 6 + 120,
                    ctx.tr("gui.done"),
                ));
            }
            Page::Skin => {
                for bit in 0..MODEL_PARTS.len() {
                    let (x, y) = grid(ctx, bit as i32, h / 6);
                    self.buttons.push(Button::new(
                        SKIN_PART + bit as i32,
                        x,
                        y,
                        150,
                        20,
                        Self::skin_label(bit, ctx),
                    ));
                }
                let n = MODEL_PARTS.len() as i32 + 1;
                self.buttons.push(Button::wide(
                    DONE,
                    w / 2 - 100,
                    h / 6 + 24 * (n >> 1),
                    ctx.tr("gui.done"),
                ));
            }
            Page::Sounds => {
                let mut i = 0;
                for c in SoundCategory::ALL {
                    let (x, y) = grid(ctx, i, h / 6 - 12);
                    let wide = c == SoundCategory::Master;
                    let pos = ctx.settings.volumes[c as usize];
                    let label = Self::sound_label(c, ctx);
                    self.buttons.push(Button::slider(
                        SOUND + c as i32,
                        x,
                        y,
                        if wide { 310 } else { 150 },
                        pos,
                        label,
                    ));
                    i += if wide { 2 } else { 1 };
                }
                self.buttons.push(Button::wide(
                    DONE,
                    w / 2 - 100,
                    h / 6 + 168,
                    ctx.tr("gui.done"),
                ));
            }
        }
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        opt_drag(&mut self.buttons, mx, ctx);
        for b in self
            .buttons
            .iter_mut()
            .filter(|b| (SOUND..SOUND + 9).contains(&b.id))
        {
            if let Some(p) = b.drag(mx) {
                let cat = SoundCategory::ALL[(b.id - SOUND) as usize];
                ctx.settings.volumes[cat as usize] = p;
                b.label = Self::sound_label(cat, ctx);
            }
        }
        default_background(c, ctx);
        let (title, y) = match self.kind {
            Page::Chat => ("options.chat.title", 20),
            Page::Skin => ("options.skinCustomisation.title", 20),
            Page::Sounds => ("options.sounds.title", 15),
        };
        centered(c, &ctx.tr(title), ctx.width / 2, y, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav> {
        let id = press(&mut self.buttons, x, y, button, ctx)?;
        match id {
            DONE => {
                ctx.save_settings();
                return self.back();
            }
            _ if (SKIN_PART..SKIN_PART + 7).contains(&id) => {
                let bit = (id - SKIN_PART) as usize;
                let on = ctx.settings.model_parts >> bit & 1 == 0;
                ctx.settings.set_part(bit, on);
                ctx.effects.push(Effect::SettingsSaved);
                super::button(&mut self.buttons, id).label = Self::skin_label(bit, ctx);
            }
            _ if (SOUND..SOUND + 9).contains(&id) => {
                if matches!(ctx.effects.last(), Some(Effect::Sound("gui.button.press"))) {
                    ctx.effects.pop();
                }
                let b = super::button(&mut self.buttons, id);
                let cat = SoundCategory::ALL[(id - SOUND) as usize];
                ctx.settings.volumes[cat as usize] = b.slider.map_or(1.0, |s| s.pos);
                b.label = Self::sound_label(cat, ctx);
            }
            _ => opt_pressed(super::button(&mut self.buttons, id), ctx),
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _button: i32, ctx: &mut Ctx) {
        // `GuiScreenOptionsSounds.Button.mouseReleased` plays the click when a volume drag ends.
        if self
            .buttons
            .iter()
            .any(|b| (SOUND..SOUND + 9).contains(&b.id) && b.slider.is_some_and(|s| s.dragging))
        {
            ctx.effects.push(Effect::Sound("gui.button.press"));
        }
        release_sliders(&mut self.buttons, ctx);
    }
}

const DIFFICULTY: i32 = 108;
const SKIN: i32 = 110;
const SUPER_SECRET: i32 = 8675309;
const SOUNDS: i32 = 106;
const STREAM: i32 = 107;
const VIDEO: i32 = 101;
const CONTROLS: i32 = 100;
const LANGUAGE: i32 = 102;
const CHAT: i32 = 103;
const RESOURCE_PACKS: i32 = 105;
const SNOOPER: i32 = 104;

/// `GuiOptions`.
pub struct Options {
    parent: Option<Box<dyn Screen>>,
    buttons: Vec<Button>,
}

impl Options {
    pub fn new(parent: Box<dyn Screen>) -> Options {
        Options {
            parent: Some(parent),
            buttons: Vec::new(),
        }
    }

    fn difficulty_label(d: u8, ctx: &Ctx) -> String {
        let names = ["peaceful", "easy", "normal", "hard"];
        let name = ctx.tr(&format!("options.difficulty.{}", names[d as usize & 3]));
        format!("{}: {name}", ctx.tr("options.difficulty"))
    }

    /// Open a sub-screen with this one as its parent.
    fn child(id: i32) -> Nav {
        Nav::Child(Box::new(move |me| -> Box<dyn Screen> {
            match id {
                VIDEO => Box::new(Video::new(me)),
                CONTROLS => Box::new(Controls::new(me)),
                CHAT => Box::new(OptionPage::new(me, Page::Chat)),
                SKIN => Box::new(OptionPage::new(me, Page::Skin)),
                _ => Box::new(OptionPage::new(me, Page::Sounds)),
            }
        }))
    }
}

impl Screen for Options {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let t = |k: &str| ctx.tr(k);
        let col = |i: i32| w / 2 - 155 + i % 2 * 160;
        let mut b = vec![opt_button(Opt::Fov, col(0), h / 6 - 12, ctx)];
        match ctx.difficulty {
            Some(d) => {
                // Only the integrated server can change it; on a server the button is disabled.
                let mut d = Button::new(
                    DIFFICULTY,
                    col(1),
                    h / 6 - 12,
                    150,
                    20,
                    Self::difficulty_label(d, ctx),
                );
                d.enabled = false;
                b.push(d);
            }
            None => b.push(opt_button(
                Opt::RealmsNotifications,
                col(1),
                h / 6 - 12,
                ctx,
            )),
        }
        let row = |id: i32, right: bool, dy: i32, key: &str| {
            let x = if right { w / 2 + 5 } else { w / 2 - 155 };
            Button::new(id, x, h / 6 + dy - 6, 150, 20, t(key))
        };
        b.push(row(SKIN, false, 48, "options.skinCustomisation"));
        b.push(Button::new(
            SUPER_SECRET,
            w / 2 + 5,
            h / 6 + 48 - 6,
            150,
            20,
            "Super Secret Settings...",
        ));
        b.push(row(SOUNDS, false, 72, "options.sounds"));
        b.push(row(STREAM, true, 72, "options.stream"));
        b.push(row(VIDEO, false, 96, "options.video"));
        b.push(row(CONTROLS, true, 96, "options.controls"));
        b.push(row(LANGUAGE, false, 120, "options.language"));
        b.push(row(CHAT, true, 120, "options.chat.title"));
        b.push(row(RESOURCE_PACKS, false, 144, "options.resourcepack"));
        b.push(row(SNOOPER, true, 144, "options.snooper.view"));
        b.push(Button::wide(DONE, w / 2 - 100, h / 6 + 168, t("gui.done")));
        // shortcut: no shaders, Twitch, language, resource pack or snooper screens yet; their buttons stay disabled.
        for id in [SUPER_SECRET, STREAM, LANGUAGE, RESOURCE_PACKS, SNOOPER] {
            button(&mut b, id).enabled = false;
        }
        self.buttons = b;
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        opt_drag(&mut self.buttons, mx, ctx);
        default_background(c, ctx);
        centered(c, &ctx.tr("options.title"), ctx.width / 2, 15, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        let id = press(&mut self.buttons, x, y, mb, ctx)?;
        match id {
            DONE => {
                ctx.save_settings();
                self.parent.take().map(Nav::Open)
            }
            VIDEO | CONTROLS | CHAT | SKIN | SOUNDS => {
                ctx.save_settings();
                Some(Self::child(id))
            }
            _ => {
                opt_pressed(button(&mut self.buttons, id), ctx);
                None
            }
        }
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, ctx: &mut Ctx) {
        release_sliders(&mut self.buttons, ctx);
    }
}

/// `GuiVideoSettings` options, in vanilla order.
const VIDEO_OPTS: [Opt; 16] = [
    Opt::Graphics,
    Opt::RenderDistance,
    Opt::Ao,
    Opt::FramerateLimit,
    Opt::Anaglyph,
    Opt::ViewBobbing,
    Opt::GuiScale,
    Opt::Gamma,
    Opt::RenderClouds,
    Opt::Particles,
    Opt::Fullscreen,
    Opt::Vsync,
    Opt::MipmapLevels,
    Opt::BlockAlternatives,
    Opt::Vbo,
    Opt::EntityShadows,
];

/// `GuiVideoSettings` with its `GuiOptionsRowList`.
pub struct Video {
    parent: Option<Box<dyn Screen>>,
    list: ScrollList,
    /// Two option buttons per row; row `i` holds `rows[2i]` and `rows[2i + 1]`.
    rows: Vec<Button>,
    buttons: Vec<Button>,
    held: bool,
}

impl Video {
    pub fn new(parent: Box<dyn Screen>) -> Video {
        Video {
            parent: Some(parent),
            list: ScrollList::new(0, 0, 0, 0, 25),
            rows: Vec::new(),
            buttons: Vec::new(),
            held: false,
        }
    }

    fn row_count(&self) -> usize {
        self.rows.len().div_ceil(2)
    }
}

impl Screen for Video {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let scroll = self.list.scroll;
        self.list = ScrollList::new(w, h, 32, h - 32, 25);
        self.list.row_w = 400;
        self.list.bar_x = w / 2 + 124 + 32;
        self.list.scroll = scroll;
        self.rows = VIDEO_OPTS
            .iter()
            .enumerate()
            .map(|(i, o)| opt_button(*o, w / 2 - 155 + i as i32 % 2 * 160, 0, ctx))
            .collect();
        self.buttons = vec![Button::wide(DONE, w / 2 - 100, h - 27, ctx.tr("gui.done"))];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let n = self.row_count();
        if self.held {
            self.list.drag(my, n);
        }
        opt_drag(&mut self.rows, mx, ctx);
        default_background(c, ctx);
        self.list.draw_back(c, n);
        for (i, b) in self.rows.iter_mut().enumerate() {
            b.y = self.list.row_y(i / 2);
            if self.list.row_visible(i / 2) {
                b.draw(c, mx, my);
            }
        }
        self.list.draw_front(c, n);
        centered(c, &ctx.tr("options.videoTitle"), ctx.width / 2, 5, WHITE);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        if press(&mut self.buttons, x, y, mb, ctx) == Some(DONE) {
            ctx.save_settings();
            return self.parent.take().map(Nav::Open);
        }
        let n = self.row_count();
        if mb == 0 {
            self.held = true;
            self.list.press(x, y, n);
        }
        // `GuiListExtended.mouseClicked`: only the row under the mouse takes the click.
        let row = (y >= self.list.top && y <= self.list.bottom)
            .then(|| self.list.row_at(x, y, n))
            .flatten()?;
        let end = (row * 2 + 2).min(self.rows.len());
        let pair = &mut self.rows[row * 2..end];
        if let Some(id) = press(pair, x, y, mb, ctx) {
            opt_pressed(button(pair, id), ctx);
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, ctx: &mut Ctx) {
        self.held = false;
        self.list.release();
        release_sliders(&mut self.rows, ctx);
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        let n = self.row_count();
        self.list.wheel(dir, n);
    }
}

const RESET_ALL: i32 = 201;

/// One `GuiKeyBindingList` row.
enum KeyRow {
    Category(&'static str),
    /// Index into [`Settings::keys`], with its change and reset buttons.
    Key(usize, Button, Button),
}

/// `GuiControls` with its `GuiKeyBindingList`.
pub struct Controls {
    parent: Option<Box<dyn Screen>>,
    list: ScrollList,
    rows: Vec<KeyRow>,
    buttons: Vec<Button>,
    /// `buttonId`: the binding that waits for a key.
    waiting: Option<usize>,
    label_w: i32,
    held: bool,
}

impl Controls {
    pub fn new(parent: Box<dyn Screen>) -> Controls {
        Controls {
            parent: Some(parent),
            list: ScrollList::new(0, 0, 0, 0, 20),
            rows: Vec::new(),
            buttons: Vec::new(),
            waiting: None,
            label_w: 0,
            held: false,
        }
    }

    /// `GameSettings.setOptionKeyBinding`: set the code and save.
    fn bind(&mut self, k: usize, code: i32, ctx: &mut Ctx) {
        ctx.settings.keys[k].code = code;
        ctx.save_settings();
        self.waiting = None;
    }
}

impl Screen for Controls {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let scroll = self.list.scroll;
        self.list = ScrollList::new(w, h, 63, h - 32, 20);
        self.list.row_w = 220 + 32;
        self.list.bar_x = w / 2 + 124 + 15;
        self.list.scroll = scroll;
        self.rows.clear();
        let mut last = "";
        for k in ctx.settings.sorted_keys(ctx.texts) {
            let kb = &ctx.settings.keys[k];
            if kb.category != last {
                last = kb.category;
                self.rows.push(KeyRow::Category(last));
            }
            let change = Button::new(0, 0, 0, 75, 20, "");
            let reset = Button::new(1, 0, 0, 50, 20, ctx.tr("controls.reset"));
            self.rows.push(KeyRow::Key(k, change, reset));
        }
        self.buttons = vec![
            Button::new(DONE, w / 2 - 155, h - 29, 150, 20, ctx.tr("gui.done")),
            Button::new(
                RESET_ALL,
                w / 2 - 155 + 160,
                h - 29,
                150,
                20,
                ctx.tr("controls.resetAll"),
            ),
        ];
        for (i, o) in [Opt::InvertMouse, Opt::Sensitivity, Opt::Touchscreen]
            .iter()
            .enumerate()
        {
            let (x, y) = (w / 2 - 155 + i as i32 % 2 * 160, 18 + 24 * (i as i32 >> 1));
            self.buttons.push(opt_button(*o, x, y, ctx));
        }
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if self.label_w == 0 {
            self.label_w = ctx
                .settings
                .keys
                .iter()
                .map(|k| c.width(&ctx.tr(k.desc)))
                .max()
                .unwrap_or(0);
        }
        let n = self.rows.len();
        if self.held {
            self.list.drag(my, n);
        }
        opt_drag(&mut self.buttons, mx, ctx);
        default_background(c, ctx);
        self.list.draw_back(c, n);
        let x = self.list.row_x();
        let keys = &ctx.settings.keys;
        for (i, row) in self.rows.iter_mut().enumerate() {
            let y = self.list.row_y(i);
            if !self.list.row_visible(i) {
                continue;
            }
            match row {
                KeyRow::Category(cat) => {
                    let s = ctx.texts.tr(cat);
                    let sw = c.width(&s);
                    text(c, &s, ctx.width / 2 - sw / 2, y + 20 - 9 - 1, WHITE);
                }
                KeyRow::Key(k, change, reset) => {
                    let kb = &keys[*k];
                    text(
                        c,
                        &ctx.texts.tr(kb.desc),
                        x + 90 - self.label_w,
                        y + 20 / 2 - 9 / 2,
                        WHITE,
                    );
                    (reset.x, reset.y, reset.enabled) = (x + 190, y, kb.code != kb.default);
                    reset.draw(c, mx, my);
                    let name = key_name(kb.code, ctx.texts);
                    let clash = kb.code != 0
                        && keys
                            .iter()
                            .enumerate()
                            .any(|(j, o)| j != *k && o.code == kb.code);
                    change.label = if self.waiting == Some(*k) {
                        format!("§f> §e{name}§f <")
                    } else if clash {
                        format!("§c{name}")
                    } else {
                        name
                    };
                    (change.x, change.y) = (x + 105, y);
                    change.draw(c, mx, my);
                }
            }
        }
        self.list.draw_front(c, n);
        centered(c, &ctx.tr("controls.title"), ctx.width / 2, 8, WHITE);
        let changed = keys.iter().any(|k| k.code != k.default);
        button(&mut self.buttons, RESET_ALL).enabled = changed;
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        if let Some(k) = self.waiting {
            self.bind(k, -100 + mb, ctx);
            return None;
        }
        let n = self.rows.len();
        if mb == 0 {
            self.held = true;
            self.list.press(x, y, n);
            let in_list = y >= self.list.top && y <= self.list.bottom;
            if let Some(KeyRow::Key(k, change, reset)) = in_list
                .then(|| self.list.row_at(x, y, n))
                .flatten()
                .map(|i| &mut self.rows[i])
            {
                let k = *k;
                if change.pressed(x, y) {
                    ctx.effects.push(Effect::Sound("gui.button.press"));
                    self.waiting = Some(k);
                    return None;
                }
                if reset.pressed(x, y) {
                    ctx.effects.push(Effect::Sound("gui.button.press"));
                    let default = ctx.settings.keys[k].default;
                    self.bind(k, default, ctx);
                    return None;
                }
            }
        }
        match press(&mut self.buttons, x, y, mb, ctx)? {
            DONE => return self.parent.take().map(Nav::Open),
            RESET_ALL => {
                ctx.settings
                    .keys
                    .iter_mut()
                    .for_each(|k| k.code = k.default);
                ctx.save_settings();
            }
            id => opt_pressed(button(&mut self.buttons, id), ctx),
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, ctx: &mut Ctx) {
        self.held = false;
        self.list.release();
        release_sliders(&mut self.buttons, ctx);
    }

    fn key(&mut self, code: i32, ch: char, _mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        let Some(k) = self.waiting else {
            return (code == super::key::ESCAPE).then_some(Nav::Game);
        };
        let new = if code == super::key::ESCAPE {
            0
        } else if code != 0 {
            code
        } else if ch != '\0' {
            ch as i32 + 256
        } else {
            ctx.settings.keys[k].code
        };
        self.bind(k, new, ctx);
        None
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        let n = self.rows.len();
        self.list.wheel(dir, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_txt_round_trips() {
        let mut s = Settings::new(PathBuf::from("/tmp"));
        s.fov = 90.0;
        s.gui_scale = 2;
        s.clouds = 1;
        s.keys[2].code = 200;
        s.volumes[SoundCategory::Music as usize] = 0.25;
        s.set_part(6, false);
        let text = s.to_text();
        assert!(text.contains("fov:0.5\n"));
        assert!(text.contains("renderClouds:fast\n"));
        assert!(text.contains("key_key.forward:200\n"));
        assert!(text.contains("soundCategory_music:0.25\n"));
        assert!(text.contains("modelPart_hat:false\n"));
        let mut back = Settings::new(PathBuf::from("/tmp"));
        for line in text.lines() {
            assert!(back.apply(line).is_some(), "{line}");
        }
        assert_eq!(back, s);
    }

    #[test]
    fn java_float_format() {
        let f = |v| JavaFloat.of(v);
        assert_eq!(f(1.0), "1.0");
        assert_eq!(f(0.0), "0.0");
        assert_eq!(f(0.44366196), "0.44366196");
        assert_eq!(f(1.0e-4), "1.0E-4");
    }

    #[test]
    fn slider_snaps_to_step() {
        assert_eq!(Opt::FramerateLimit.denorm(0.5), 140.0);
        assert_eq!(Opt::Fov.denorm(1.0), 110.0);
        assert_eq!(Opt::RenderDistance.norm(17.0), 0.5);
    }
}
