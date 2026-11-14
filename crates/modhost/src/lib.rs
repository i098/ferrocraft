//! Rust side of the mod host. See the README of this crate.

pub mod anchor;
pub mod chat;
pub mod events;
pub mod hooks;
pub mod hud;
pub mod hud_lists;
pub mod keys;
pub mod mirror;
pub mod overrides;
pub mod probe;
pub mod provision;
pub mod provision_fabric;
pub mod wire;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::PhysicalKey;

pub use chat::{Kind as ChatKind, Verdict as ChatVerdict};
pub use events::{Event as ModEvent, Spawn};
pub use hud_lists::Lists as HudLists;
pub use mirror::Mirror;
pub use overrides::{Camera, Overrides, WorldState};
pub use wire::{DrawCmd, Event, Input, PlayerInput, PlayerState};

/// A random secret for one JVM: only the process that got it can say hello.
fn token() -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};
    (0..2)
        .map(|_| format!("{:016x}", RandomState::new().build_hasher().finish()))
        .collect()
}

/// The mod loader the JVM runs.
pub enum Loader {
    /// FML on LaunchWrapper: the host tweakers register the transformers.
    Forge,
    /// Fabric Loader's Knot, with the host jar (`java/modhost-fabric`) as its Java agent.
    Fabric {
        /// The host jar; it is on the class path too.
        agent: PathBuf,
        /// Tiny v2 file from Forge's MCP names to intermediary names, see `provision_fabric`.
        names: PathBuf,
    },
}

/// What the JVM needs to start.
pub struct Config {
    pub loader: Loader,
    /// `bin/java` of a Java 8 runtime.
    pub java: PathBuf,
    /// Class path: for Forge LaunchWrapper, Forge, the stand-in Minecraft jar, the host classes and the libraries; for Fabric
    /// the host jar, LaunchWrapper's classes, Knot and its libraries and the game jar.
    pub class_path: Vec<PathBuf>,
    /// Vanilla game directory; mods are read from `mods/` below it.
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
}

#[derive(PartialEq)]
enum Phase {
    /// The JVM has not said hello yet; input is queued and sent with the first frame.
    Starting,
    Live,
    /// Stopped, or the JVM is gone.
    Done,
}

struct Shared {
    phase: Phase,
    /// A view arrived since the last frame was sent.
    fresh: bool,
    input: Input,
    /// The HUD state for the next frame, sent once.
    mirror: Option<Mirror>,
    /// The HUD lists for the next frame, sent once after the mirror.
    lists: Option<HudLists>,
    commands: Vec<DrawCmd>,
    /// A screen a mod opened is current in the JVM.
    screen: bool,
    /// The files the JVM defined, by name.
    sources: HashMap<String, Vec<u8>>,
    /// Texture names handed to the renderer, and whether the file decoded.
    textures: HashMap<String, bool>,
    /// A player tick the game thread waits on, and the JVM's answer.
    player: Option<PlayerState>,
    answer: Option<PlayerInput>,
    /// A world values request the game thread waits on, and the JVM's answer.
    world: Option<WorldState>,
    world_answer: Option<Overrides>,
    /// A world draws request and the JVM's answer, see `anchor.rs`.
    anchor: anchor::Slot,
    /// A chat line the receiving thread waits on, and the JVM's verdict.
    chat: Option<(ChatKind, String)>,
    chat_answer: Option<ChatVerdict>,
    /// The last chat reply said Java keeps the chat: a timed-out line may already be on its screen.
    chat_taken: bool,
    /// A render probe the game thread waits on, and the JVM's answer.
    probe: Option<probe::Request>,
    probe_answer: Option<probe::Answer>,
    probe_gate: probe::Gate,
    /// World and entity events for the next frame, oldest first.
    events: Vec<events::Event>,
    /// A player tick, a world values request or a chat line got no answer in time: no more are asked until the pump has finished the round trip it is in,
    /// and the late answer is dropped.
    stalled: bool,
    /// `ClientBrandRetriever.getClientModName()` from the hello; `None` until the JVM connected.
    brand: Option<String>,
    /// The vanilla methods that a mod changed, from the hello; empty until the JVM connected.
    hooks: hooks::Hooks,
    /// The HUD elements that Java owns in the last frame.
    hud: hud::Owned,
}

struct Link {
    state: Mutex<Shared>,
    view: Condvar,
    answered: Condvar,
}

/// How long the game tick waits for the JVM's answer to a player tick.
const PLAYER_TICK_WAIT: Duration = Duration::from_millis(100);

fn prefixed(prefix: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(prefix);
    arg.push(path);
    arg
}

/// A running JVM and the last frame of draw commands it sent.
pub struct ModHost {
    child: Arc<Mutex<Child>>,
    link: Arc<Link>,
}

impl ModHost {
    /// Start the JVM and return at once. A worker thread waits for it to connect, which is after FML
    /// has loaded the mods, and gives up when the process exits first.
    pub fn start(cfg: &Config) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let sep = if cfg!(windows) { ";" } else { ":" };
        let cp = cfg
            .class_path
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(sep);
        let token = token();
        let mut cmd = Command::new(&cfg.java);
        cmd.env("FERROCRAFT_BRIDGE_TOKEN", &token)
            .arg(format!("-Dferro.bridge=127.0.0.1:{port}"));
        match &cfg.loader {
            Loader::Forge => {
                cmd.args(["-cp", &cp, "net.minecraft.launchwrapper.Launch"])
                    .args([
                        "--tweakClass",
                        "net.minecraftforge.fml.common.launcher.FMLTweaker",
                    ])
                    .args(["--tweakClass", "net.ferrocraft.tweak.HostTweaker"]);
            }
            Loader::Fabric { agent, names } => {
                cmd.arg(prefixed("-javaagent:", agent))
                    .arg(prefixed("-Dferro.tiny=", names))
                    .arg(prefixed("-Dferro.gameDir=", &cfg.game_dir))
                    .args([
                        "-cp",
                        &cp,
                        "net.fabricmc.loader.impl.launch.knot.KnotClient",
                    ]);
            }
        }
        let child = cmd
            .args(["--gameDir".as_ref(), cfg.game_dir.as_os_str()])
            .args(["--assetsDir".as_ref(), cfg.assets_dir.as_os_str()])
            .args(["--version", "1.8.9", "--accessToken", "x"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        let host = Self {
            child: Arc::new(Mutex::new(child)),
            link: Arc::new(Link {
                state: Mutex::new(Shared {
                    phase: Phase::Starting,
                    fresh: false,
                    input: Input::default(),
                    mirror: None,
                    lists: None,
                    commands: Vec::new(),
                    screen: false,
                    sources: HashMap::new(),
                    textures: HashMap::new(),
                    player: None,
                    answer: None,
                    world: None,
                    world_answer: None,
                    anchor: anchor::Slot::default(),
                    chat: None,
                    chat_answer: None,
                    chat_taken: false,
                    probe: None,
                    probe_answer: None,
                    probe_gate: probe::Gate::default(),
                    events: Vec::new(),
                    stalled: false,
                    brand: None,
                    hooks: hooks::Hooks::default(),
                    hud: hud::Owned::default(),
                }),
                view: Condvar::new(),
                answered: Condvar::new(),
            }),
        };
        let (child, link) = (Arc::clone(&host.child), Arc::clone(&host.link));
        thread::spawn(move || run(&listener, &child, &link, &token));
        Ok(host)
    }

    /// Set the window size in pixels, the GUI scale setting, the unicode font flag, whether a screen
    /// is open and the mouse in pixels for the next frame, and ask for that frame. The JVM runs one
    /// frame per call.
    pub fn set_view(
        &self,
        width: i32,
        height: i32,
        gui_scale: i32,
        unicode: bool,
        screen: bool,
        mouse: (i32, i32),
    ) {
        let mut s = self.link.state.lock().unwrap();
        s.input.width = width;
        s.input.height = height;
        s.input.gui_scale = gui_scale;
        s.input.unicode = unicode;
        s.input.screen = screen;
        s.input.mouse = mouse;
        s.fresh = true;
        self.link.view.notify_one();
    }

    /// Set the engine state that HUD mods read; it goes to the JVM with the next frame.
    pub fn set_mirror(&self, mirror: Mirror) {
        self.link.state.lock().unwrap().mirror = Some(mirror);
    }

    /// Set the tab list, scoreboard, boss bar, title and action bar that the mods read; they go to the JVM with the next frame.
    pub fn set_lists(&self, lists: HudLists) {
        self.link.state.lock().unwrap().lists = Some(lists);
    }

    /// Queue a world or entity event for the mods' Forge handlers. It goes to the JVM before the next frame, in order.
    /// Events wait while the JVM loads the mods; they are dropped once it has stopped, or when a stalled JVM lets
    /// 16384 pile up.
    pub fn post_event(&self, event: events::Event) {
        const MAX_EVENTS: usize = 1 << 14;
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Done && s.events.len() < MAX_EVENTS {
            s.events.push(event);
        }
    }

    /// Queue key, mouse button and wheel transitions from a winit window event for the next frame.
    /// `screen` is whether a screen was open when the event arrived: the game does not see those.
    pub fn window_event(&self, event: &WindowEvent, screen: bool) {
        if let Some(e) = transition(event) {
            self.queue(e, screen);
        }
    }

    fn queue(&self, e: Event, screen: bool) {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Done {
            s.input.events.push((e, screen));
        }
    }

    /// Draw the last finished frame into the HUD's `Gui`, over the vanilla HUD; nothing once the JVM is gone.
    /// Images the renderer has not seen yet are appended to `textures` as PNG files under the name quads use.
    pub fn draw(&self, g: &mut gui::Gui, textures: &mut Vec<(String, Vec<u8>)>) {
        let mut s = self.link.state.lock().unwrap();
        let Shared {
            commands,
            sources,
            textures: known,
            ..
        } = &mut *s;
        let factor = g.scale.factor.max(1);
        for c in commands.iter() {
            match c {
                DrawCmd::Rect {
                    rect: [x0, y0, x1, y1],
                    argb,
                } => g.fill(
                    x0.round() as i32,
                    y0.round() as i32,
                    x1.round() as i32,
                    y1.round() as i32,
                    *argb,
                ),
                DrawCmd::Gradient {
                    rect: [x0, y0, x1, y1],
                    top,
                    bottom,
                } => g.gradient(
                    x0.round() as i32,
                    y0.round() as i32,
                    x1.round() as i32,
                    y1.round() as i32,
                    *top,
                    *bottom,
                ),
                DrawCmd::Text {
                    text,
                    pos: [x, y],
                    argb,
                    shadow,
                    translate,
                    scale,
                } => {
                    g.push();
                    g.translate(translate[0], translate[1]);
                    g.scale(*scale, *scale);
                    g.draw_string(text, *x, *y, *argb, *shadow);
                    g.pop();
                }
                DrawCmd::Image {
                    name,
                    rect: [x0, y0, x1, y1],
                    argb,
                    recolor,
                } => {
                    let (w, h) = (x1 - x0, y1 - y0);
                    if let (Some(data), true) = (sources.get(name), w > 0.0 && h > 0.0) {
                        let px = |v: f32| {
                            ((v * factor as f32).round().clamp(1.0, MAX_SVG_PX) as u32)
                                .next_multiple_of(16)
                        };
                        if let Some((tex, tint)) = texture(
                            name,
                            data,
                            (px(w), px(h)),
                            (*argb, *recolor),
                            known,
                            textures,
                        ) {
                            g.blit_tinted(tex, *x0, *y0, w, h, [0.0, 0.0, 1.0, 1.0], tint);
                        }
                    }
                }
                DrawCmd::Round { rect, radii, argb } => {
                    let k = factor as f32;
                    for (r, cover) in round_spans(rect.map(|v| v * k), radii.map(|v| v * k)) {
                        let [x0, y0, x1, y1] = r.map(|v| v / k);
                        let c = with_cover(*argb, cover);
                        g.quad(
                            "",
                            [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
                            [[0.0; 2]; 4],
                            [c; 4],
                        );
                    }
                }
                DrawCmd::Line {
                    rect: [x0, y0, x1, y1],
                    width,
                    argb,
                } => {
                    let (dx, dy) = (x1 - x0, y1 - y0);
                    let len = dx.hypot(dy);
                    if len > 0.0 {
                        // Half the width, at least half a pixel, to the left of the direction.
                        let half = width.max(1.0 / factor as f32) / 2.0;
                        let (nx, ny) = (-dy / len * half, dx / len * half);
                        g.quad(
                            "",
                            [
                                [x0 + nx, y0 + ny],
                                [x1 + nx, y1 + ny],
                                [x1 - nx, y1 - ny],
                                [x0 - nx, y0 - ny],
                            ],
                            [[0.0; 2]; 4],
                            [*argb; 4],
                        );
                    }
                }
                DrawCmd::Clip(rect) => g.set_clip(*rect),
                DrawCmd::Define { .. } | DrawCmd::Hud(_) => {}
            }
        }
        g.set_clip(None);
    }

    /// One tick of the stand-in `EntityPlayerSP.onLivingUpdate` in the JVM, which the mods hook, and the sprint
    /// and sneak it leaves. `None` while the JVM is not live, or when it does not answer in time: the engine then
    /// uses its own input.
    pub fn player_tick(&self, state: PlayerState) -> Option<PlayerInput> {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Live || s.stalled {
            return None;
        }
        s.player = Some(state);
        s.answer = None;
        self.link.view.notify_one();
        let (mut s, _) = self
            .link
            .answered
            .wait_timeout_while(s, PLAYER_TICK_WAIT, |s| {
                s.answer.is_none() && s.phase == Phase::Live
            })
            .unwrap();
        s.player = None;
        let answer = s.answer.take();
        s.stalled = answer.is_none() && s.phase == Phase::Live;
        answer
    }

    /// The world values the mods change (time, weather, camera, FOV, night vision) and what their look hook does to
    /// the player, once per frame. `None` while the JVM is not live or when it does not answer in time: the engine
    /// then uses the vanilla values. A late answer stalls like one of `player_tick`.
    pub fn overrides(&self, state: WorldState) -> Option<Overrides> {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Live || s.stalled {
            return None;
        }
        s.world = Some(state);
        s.world_answer = None;
        self.link.view.notify_one();
        let (mut s, _) = self
            .link
            .answered
            .wait_timeout_while(s, PLAYER_TICK_WAIT, |s| {
                s.world_answer.is_none() && s.phase == Phase::Live
            })
            .unwrap();
        s.world = None;
        let answer = s.world_answer.take();
        s.stalled = answer.is_none() && s.phase == Phase::Live;
        answer
    }

    /// The render probes of one frame (`probe.rs`): the hurt tint, the item glint color and the entities that a render
    /// handler cancelled. `None` while the JVM is not live, when no mod changed what the frame draws, or when the JVM does not
    /// answer in time: the renderer then uses the vanilla values. A late answer stalls like one of `player_tick`.
    pub fn probe(&self, frame: &probe::Frame) -> Option<probe::Reply> {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Live || s.stalled {
            return None;
        }
        let Shared {
            probe_gate, hooks, ..
        } = &mut *s;
        let request = probe_gate.request(hooks, frame)?;
        s.probe = Some(request);
        s.probe_answer = None;
        self.link.view.notify_one();
        let (mut s, _) = self
            .link
            .answered
            .wait_timeout_while(s, PLAYER_TICK_WAIT, |s| {
                s.probe_answer.is_none() && s.phase == Phase::Live
            })
            .unwrap();
        s.probe = None;
        let answer = s.probe_answer.take();
        s.stalled = answer.is_none() && s.phase == Phase::Live;
        if let Some(listening) = answer.as_ref().and_then(|a| a.listening) {
            s.probe_gate.learn(listening);
        }
        answer.map(|a| a.reply)
    }

    /// Post a received chat line, with `§` format codes, to the mods' `ClientChatReceivedEvent` and return what
    /// they did with it. `Keep` while the JVM is not live or does not answer in time.
    pub fn chat(&self, kind: ChatKind, text: &str) -> ChatVerdict {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Live || s.stalled || s.chat.is_some() || text.len() > chat::MAX_TEXT {
            return ChatVerdict::Keep;
        }
        s.chat = Some((kind, text.to_owned()));
        s.chat_answer = None;
        self.link.view.notify_one();
        let (mut s, _) = self
            .link
            .answered
            .wait_timeout_while(s, PLAYER_TICK_WAIT, |s| {
                s.chat_answer.is_none() && s.phase == Phase::Live
            })
            .unwrap();
        // Sent but unanswered while Java keeps the chat: it may still print the line, so Rust must not.
        let sent = s.chat.is_none();
        s.chat = None;
        let answer = s.chat_answer.take();
        s.stalled = answer.is_none() && s.phase == Phase::Live;
        match answer {
            Some(a) => {
                s.chat_taken = a == ChatVerdict::Taken;
                a
            }
            None if sent && s.chat_taken && s.phase == Phase::Live => ChatVerdict::Cancel,
            None => ChatVerdict::Keep,
        }
    }

    /// The brand the hosted `ClientBrandRetriever.getClientModName()` returned (`fml,forge`), once the JVM is
    /// live. The client sends it as `MC|Brand`; `None` means no mod host answers, so the brand stays `vanilla`.
    pub fn client_brand(&self) -> Option<String> {
        let s = self.link.state.lock().unwrap();
        s.brand.clone().filter(|_| s.phase == Phase::Live)
    }

    /// The vanilla methods that a mod changed, once the JVM is live; empty before that and with no mod.
    pub fn hooks(&self) -> hooks::Hooks {
        let s = self.link.state.lock().unwrap();
        match s.phase {
            Phase::Live => s.hooks.clone(),
            _ => hooks::Hooks::default(),
        }
    }

    /// The vanilla HUD elements that Java owns in the last frame, once the JVM is live: Rust does not draw them. Nothing
    /// is owned before that, so Rust draws the whole HUD.
    pub fn hud_owned(&self) -> hud::Owned {
        self.link.state.lock().unwrap().hud
    }

    /// Whether a screen a mod opened (OneConfig's GUI) is open: it owns the keys, the mouse and the cursor.
    pub fn screen_open(&self) -> bool {
        self.link.state.lock().unwrap().screen
    }

    pub fn stop(&mut self) {
        {
            let mut s = self.link.state.lock().unwrap();
            s.phase = Phase::Done;
            s.commands.clear();
            s.hud = hud::Owned::default();
            s.screen = false;
            self.link.view.notify_all();
            self.link.answered.notify_all();
        }
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for ModHost {
    fn drop(&mut self) {
        self.stop();
    }
}

fn transition(event: &WindowEvent) -> Option<Event> {
    match event {
        WindowEvent::KeyboardInput { event: k, .. } => {
            let PhysicalKey::Code(code) = k.physical_key else {
                return None;
            };
            let pressed = k.state == ElementState::Pressed;
            Some(Event::Key {
                code: keys::lwjgl(code)?,
                pressed,
                repeat: k.repeat,
                ch: k
                    .text
                    .as_deref()
                    .and_then(|t| t.chars().next())
                    .filter(|_| pressed)
                    .and_then(|c| u16::try_from(c).ok())
                    .unwrap_or(0),
            })
        }
        WindowEvent::MouseInput { state, button, .. } => Some(Event::Button {
            button: match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                _ => return None,
            },
            pressed: *state == ElementState::Pressed,
        }),
        // LWJGL reports 120 per notch. Pixel deltas (trackpads) arrive in many small steps and are dropped.
        WindowEvent::MouseWheel {
            delta: MouseScrollDelta::LineDelta(_, y),
            ..
        } => {
            let delta = (y * 120.0) as i32;
            (delta != 0).then_some(Event::Wheel(delta))
        }
        _ => None,
    }
}

/// Connect, then serve frames until the JVM goes away; then leave no stale commands behind.
fn run(listener: &TcpListener, child: &Mutex<Child>, link: &Link, token: &str) {
    let result = connect(listener, child, link, token).and_then(|stream| pump(stream, link));
    let mut s = link.state.lock().unwrap();
    if let (Err(e), false) = (result, s.phase == Phase::Done) {
        eprintln!("mod host stopped: {e}");
    }
    s.phase = Phase::Done;
    s.commands.clear();
    s.hud = hud::Owned::default();
    s.screen = false;
    link.answered.notify_all();
}

/// Accept connections until one sends the hello with the token; the others are closed. Gives up when the JVM exits.
fn connect(
    listener: &TcpListener,
    child: &Mutex<Child>,
    link: &Link,
    token: &str,
) -> io::Result<TcpStream> {
    let (stream, brand, hooks) = loop {
        match listener.accept() {
            Ok((stream, _)) => match hello(stream, token) {
                Ok(hello) => break hello,
                Err(e) => eprintln!("mod host: connection refused: {e}"),
            },
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if let Some(status) = child.lock().unwrap().try_wait()? {
                    return Err(io::Error::other(format!(
                        "the JVM exited with {status} before it connected"
                    )));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    };
    let mut s = link.state.lock().unwrap();
    if s.phase != Phase::Starting {
        return Err(io::Error::other("stopped"));
    }
    s.phase = Phase::Live;
    s.brand = Some(brand);
    s.hooks = hooks;
    Ok(stream)
}

/// Read `'H'`, the token the JVM got through its environment, the brand (`writeUTF`: u16 length, bytes) and the hook table.
fn hello(mut stream: TcpStream, token: &str) -> io::Result<(TcpStream, String, hooks::Hooks)> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut got = vec![0; 1 + token.len()];
    stream.read_exact(&mut got)?;
    if got[0] != b'H' || &got[1..] != token.as_bytes() {
        return Err(io::Error::other("bad hello"));
    }
    let mut len = [0; 2];
    stream.read_exact(&mut len)?;
    let mut brand = vec![0; usize::from(u16::from_be_bytes(len))];
    stream.read_exact(&mut brand)?;
    let brand = String::from_utf8(brand).map_err(|_| io::Error::other("bad brand"))?;
    let hooks = hooks::Hooks::read(&mut stream)?;
    stream.set_read_timeout(None)?;
    Ok((stream, brand, hooks))
}

/// One frame per `set_view`: send the input and the events since the last frame, read the commands.
fn pump(mut stream: TcpStream, link: &Link) -> io::Result<()> {
    loop {
        let (input, mirror, lists, events) = {
            let mut s = link.state.lock().unwrap();
            while s.phase == Phase::Live
                && !s.fresh
                && s.player.is_none()
                && s.world.is_none()
                && s.anchor.request.is_none()
                && s.chat.is_none()
                && s.probe.is_none()
            {
                s = link.view.wait(s).unwrap();
            }
            if s.phase != Phase::Live {
                return Ok(());
            }
            if let Some(p) = s.player.take() {
                drop(s);
                round_trip(
                    &mut stream,
                    link,
                    |st| wire::write_player(st, &p),
                    wire::read_player,
                    |s, a| s.answer = Some(a),
                )?;
                continue;
            }
            if let Some(w) = s.world.take() {
                drop(s);
                round_trip(
                    &mut stream,
                    link,
                    |st| overrides::write_state(st, &w),
                    overrides::read_overrides,
                    |s, a| s.world_answer = Some(a),
                )?;
                continue;
            }
            if let Some(a) = s.anchor.request.take() {
                drop(s);
                round_trip(
                    &mut stream,
                    link,
                    |st| anchor::write_request(st, &a),
                    anchor::read_world,
                    |s, w| s.anchor.answer = Some(w),
                )?;
                continue;
            }
            if let Some(q) = s.probe.take() {
                drop(s);
                round_trip(
                    &mut stream,
                    link,
                    |st| probe::write_request(st, &q),
                    |st| probe::read_reply(st, &q),
                    |s, a| s.probe_answer = Some(a),
                )?;
                continue;
            }
            if let Some((kind, text)) = s.chat.take() {
                drop(s);
                round_trip(
                    &mut stream,
                    link,
                    |st| chat::write_line(st, kind, &text),
                    chat::read_verdict,
                    |s, a| s.chat_answer = Some(a),
                )?;
                continue;
            }
            s.fresh = false;
            let input = s.input.clone();
            s.input.events.clear();
            (
                input,
                s.mirror.take(),
                s.lists.take(),
                std::mem::take(&mut s.events),
            )
        };
        events::write_all(&mut stream, &events)?;
        write_hud(&mut stream, mirror.as_ref(), lists.as_ref())?;
        wire::write_input(&mut stream, &input)?;
        let (screen, cmds) = wire::read_frame(&mut stream)?;
        apply_frame(&mut link.state.lock().unwrap(), screen, cmds);
    }
}

/// Stores one frame of the JVM: the draw commands, the resources, the HUD elements it owns and the screen flag.
fn apply_frame(s: &mut Shared, screen: bool, cmds: Vec<DrawCmd>) {
    s.commands = Vec::with_capacity(cmds.len());
    s.hud = hud::Owned::default();
    for c in cmds {
        match c {
            DrawCmd::Define { name, data } => {
                s.sources.insert(name, data);
            }
            DrawCmd::Hud(bits) => s.hud = s.hud.with(bits),
            c => s.commands.push(c),
        }
    }
    s.screen = screen;
    s.stalled = false;
}

/// Sends one request and records the JVM's answer; a stalled client keeps the answer out.
fn round_trip<T>(
    stream: &mut TcpStream,
    link: &Link,
    write: impl FnOnce(&mut TcpStream) -> io::Result<()>,
    read: impl FnOnce(&mut TcpStream) -> io::Result<T>,
    set: impl FnOnce(&mut Shared, T),
) -> io::Result<()> {
    write(stream)?;
    let answer = read(stream)?;
    let mut s = link.state.lock().unwrap();
    if !s.stalled {
        set(&mut s, answer);
    }
    s.stalled = false;
    link.answered.notify_all();
    Ok(())
}

impl Config {
    /// Read `FERROCRAFT_MODHOST_JAVA`, `FERROCRAFT_MODHOST_CP` (a platform path list) and
    /// `FERROCRAFT_MODHOST_GAMEDIR`. `None` when any of the three is unset: no mods then.
    pub fn from_env() -> Option<Self> {
        let java = PathBuf::from(std::env::var_os("FERROCRAFT_MODHOST_JAVA")?);
        let class_path =
            std::env::split_paths(&std::env::var_os("FERROCRAFT_MODHOST_CP")?).collect();
        let game_dir = PathBuf::from(std::env::var_os("FERROCRAFT_MODHOST_GAMEDIR")?);
        Some(Self {
            loader: Loader::Forge,
            java,
            class_path,
            assets_dir: game_dir.join("assets"),
            game_dir,
        })
    }
}

/// Largest side of a rasterized SVG, in window pixels.
const MAX_SVG_PX: f32 = 2048.0;

/// `argb` with its alpha scaled by `cover`, for the pixels that a shape edge covers in part.
fn with_cover(argb: u32, cover: f32) -> u32 {
    let a = ((argb >> 24) as f32 * cover).round() as u32;
    (argb & 0x00FF_FFFF) | (a.min(255) << 24)
}

/// The boxes (in pixels, each with the share of it that the shape covers) that fill a rectangle with rounded corners.
/// The middle is one box; the rows of the corner bands are one box each, with the pixels at the curve in part.
fn round_spans(rect: [f32; 4], radii: [f32; 4]) -> Vec<([f32; 4], f32)> {
    let [x0, y0, x1, y1] = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let [tl, tr, br, bl] = radii.map(|r| r.max(0.0));
    // Radii that do not fit shrink together, as in CSS.
    let fit = [(tl + bl, h), (tr + br, h), (tl + tr, w), (bl + br, w)]
        .into_iter()
        .filter(|(sum, _)| *sum > 0.0)
        .fold(1.0f32, |k, (sum, room)| k.min(room / sum));
    let [tl, tr, br, bl] = [tl, tr, br, bl].map(|r| r * fit);
    // How far the curve of a corner of radius `r` with its center `d` from the row cuts into the box.
    let inset = |r: f32, d: f32| {
        if d <= 0.0 {
            0.0
        } else {
            r - (r * r - d * d).max(0.0).sqrt()
        }
    };
    let (top, bottom) = (tl.max(tr), bl.max(br));
    let mut out = Vec::new();
    let (mid0, mid1) = ((y0 + top).ceil(), (y1 - bottom).floor());
    if mid0 < mid1 {
        out.push(([x0, mid0, x1, mid1], 1.0));
    }
    let mut row = y0.floor();
    while row < y1 {
        let (a, b) = (row.max(y0), (row + 1.0).min(y1));
        row += 1.0;
        if a >= b || (a >= mid0 && b <= mid1) {
            continue;
        }
        let cy = (a + b) / 2.0;
        let left = x0 + inset(tl, y0 + tl - cy).max(inset(bl, cy - (y1 - bl)));
        let right = x1 - inset(tr, y0 + tr - cy).max(inset(br, cy - (y1 - br)));
        if left >= right {
            continue;
        }
        let (first, last) = (left.ceil(), right.floor());
        if first > last {
            out.push(([left.floor(), a, left.floor() + 1.0, b], right - left));
            continue;
        }
        if first < last {
            out.push(([first, a, last, b], 1.0));
        }
        if left < first {
            out.push(([first - 1.0, a, first, b], first - left));
        }
        if right > last {
            out.push(([last, a, last + 1.0, b], right - last));
        }
    }
    out
}

/// The texture name and the tint to blit an image with, registering the texture on first use. An SVG is rasterized
/// at `size` pixels, in white when `recolor` so that the tint gives its color. `None` when the file does not
/// decode (logged once).
fn texture(
    name: &str,
    data: &[u8],
    size: (u32, u32),
    (argb, recolor): (u32, bool),
    known: &mut HashMap<String, bool>,
    textures: &mut Vec<(String, Vec<u8>)>,
) -> Option<(String, u32)> {
    let svg = data.trim_ascii_start().starts_with(b"<");
    let key = match (svg, recolor) {
        (false, _) => format!("modhost/{name}"),
        (true, false) => format!("modhost/{name}@{}x{}", size.0, size.1),
        (true, true) => format!("modhost/{name}@{}x{}#white", size.0, size.1),
    };
    let ok = *known.entry(key.clone()).or_insert_with(|| {
        let png = if svg {
            rasterize(data, size, recolor)
        } else {
            Ok(data.to_vec())
        };
        match png {
            Ok(png) => {
                textures.push((key.clone(), png));
                true
            }
            Err(e) => {
                eprintln!("mod host: {name}: {e}");
                false
            }
        }
    });
    ok.then_some((key, argb))
}

/// An SVG file as a PNG of `w` x `h` pixels; when `white` every pixel is white and keeps its alpha.
fn rasterize(svg: &[u8], (w, h): (u32, u32), white: bool) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("empty image")?;
    let size = tree.size();
    let fit =
        resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, fit, &mut pixmap.as_mut());
    let mut rgba = pixmap.take();
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if white {
            p[..3].fill(255);
        } else {
            // tiny-skia stores premultiplied colors; PNG wants straight ones
            for v in &mut p[..3] {
                *v = (u32::from(*v) * 255 + a / 2)
                    .checked_div(a)
                    .map_or(0, |x| x.min(255)) as u8;
            }
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgba))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// The HUD state of a frame: the mirror, then the lists, each only when it changed hands.
fn write_hud(
    stream: &mut TcpStream,
    mirror: Option<&Mirror>,
    lists: Option<&HudLists>,
) -> io::Result<()> {
    if let Some(m) = mirror {
        mirror::write(stream, m)?;
    }
    if let Some(l) = lists {
        hud_lists::write(stream, l)?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Instant;

    fn area(spans: &[([f32; 4], f32)]) -> f32 {
        spans
            .iter()
            .map(|([x0, y0, x1, y1], c)| (x1 - x0) * (y1 - y0) * c)
            .sum()
    }

    #[test]
    fn rounded_rect_covers_the_area_of_its_shape() {
        let (w, h, r) = (100.0f32, 60.0f32, 12.0f32);
        let want = w * h - (4.0 - std::f32::consts::PI) * r * r;
        let got = area(&round_spans([3.3, 5.6, 3.3 + w, 5.6 + h], [r; 4]));
        assert!((got - want).abs() / want < 0.005, "{got} vs {want}");
        // One radius: only that corner is cut.
        let want = w * h - (1.0 - std::f32::consts::FRAC_PI_4) * r * r;
        let got = area(&round_spans([0.0, 0.0, w, h], [r, 0.0, 0.0, 0.0]));
        assert!((got - want).abs() / want < 0.005, "{got} vs {want}");
        // No radius: the rectangle itself, in one box.
        assert_eq!(round_spans([0.0, 0.0, w, h], [0.0; 4]).len(), 1);
        // A radius past half the box is a pill, with no overlap.
        let got = area(&round_spans([0.0, 0.0, 40.0, 20.0], [99.0; 4]));
        let want = 20.0 * 20.0 + std::f32::consts::PI * 100.0;
        assert!((got - want).abs() / want < 0.01, "{got} vs {want}");
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="red"/></svg>"#;

    fn link() -> Arc<Link> {
        Arc::new(Link {
            state: Mutex::new(Shared {
                phase: Phase::Starting,
                fresh: false,
                input: Input::default(),
                mirror: None,
                lists: None,
                commands: Vec::new(),
                screen: false,
                sources: HashMap::new(),
                textures: HashMap::new(),
                player: None,
                answer: None,
                world: None,
                world_answer: None,
                anchor: anchor::Slot::default(),
                probe: None,
                probe_answer: None,
                probe_gate: probe::Gate::default(),
                chat: None,
                chat_answer: None,
                chat_taken: false,
                events: Vec::new(),
                stalled: false,
                brand: None,
                hooks: hooks::Hooks::default(),
                hud: hud::Owned::default(),
            }),
            view: Condvar::new(),
            answered: Condvar::new(),
        })
    }

    fn listener() -> (TcpListener, u16) {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.set_nonblocking(true).unwrap();
        let port = l.local_addr().unwrap().port();
        (l, port)
    }

    fn jvm(port: u16, token: &str) -> TcpStream {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"H").unwrap();
        s.write_all(token.as_bytes()).unwrap();
        s.write_all(&[0, 9]).unwrap();
        s.write_all(b"fml,forge").unwrap();
        s.write_all(&[0, 0, 0, 1]).unwrap();
        for part in [
            "net/minecraft/client/gui/GuiIngame",
            "func_175180_a",
            "(F)V",
        ] {
            s.write_all(&(part.len() as u16).to_be_bytes()).unwrap();
            s.write_all(part.as_bytes()).unwrap();
        }
        s
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "timed out: {what}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn decode(png: &[u8]) -> Vec<u8> {
        let mut reader = png::Decoder::new(png).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        buf
    }

    #[test]
    fn input_queued_while_starting_goes_out_in_order_and_the_reply_is_kept() {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let key = |pressed| Event::Key {
            code: 54,
            pressed,
            repeat: false,
            ch: if pressed { 0x41 } else { 0 },
        };
        host.queue(key(true), false);
        host.queue(key(false), false);
        host.set_view(854, 480, 2, false, false, (5, 6));
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };

        let mut stream = jvm(port, TOKEN);
        let mut frame = [0; 25 + 2 * 8];
        stream.read_exact(&mut frame).unwrap();
        assert_eq!(frame[0], b'F');
        assert_eq!(frame[23..25], [0, 2]);
        assert_eq!(frame[25..33], [b'K', 0, 0, 0, 54, 0b01, 0, 0x41]);
        assert_eq!(frame[33..41], [b'K', 0, 0, 0, 54, 0, 0, 0]);

        let mut reply = vec![
            1, 0, 0, 0, 2, b'D', 0, 0, 0, 1, b'a', 0, 0, 0, 2, 7, 8, b'R',
        ];
        for v in [1.0f32, 2.0, 3.0, 4.0] {
            reply.extend(v.to_be_bytes());
        }
        reply.extend(0xFF00_FF00u32.to_be_bytes());
        stream.write_all(&reply).unwrap();
        wait_until("the reply", || host.screen_open());
        {
            let s = link.state.lock().unwrap();
            assert_eq!(
                s.commands,
                vec![DrawCmd::Rect {
                    rect: [1.0, 2.0, 3.0, 4.0],
                    argb: 0xFF00_FF00
                }]
            );
            assert_eq!(s.sources["a"], [7, 8]);
        }

        drop(stream);
        host.set_view(854, 480, 2, false, false, (5, 6));
        worker.join().unwrap();
        let s = link.state.lock().unwrap();
        assert!(s.phase == Phase::Done && s.commands.is_empty() && !s.screen);
        drop(s);
        host.queue(key(true), false);
        assert!(link.state.lock().unwrap().input.events.is_empty());
        let _ = child.lock().unwrap().kill();
    }

    #[test]
    fn a_chat_line_goes_to_the_jvm_and_its_verdict_comes_back() {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let mut host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };
        let mut stream = jvm(port, TOKEN);
        wait_until("live", || link.state.lock().unwrap().phase == Phase::Live);

        let verdict = thread::scope(|scope| {
            let sender = scope.spawn(|| host.chat(ChatKind::Chat, "spam"));
            let mut request = [0; 10];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, &[b'C', 0, 0, 0, 0, 4, b's', b'p', b'a', b'm']);
            stream.write_all(&[1]).unwrap();
            sender.join().unwrap()
        });
        assert_eq!(verdict, ChatVerdict::Cancel);

        host.stop();
        worker.join().unwrap();
        assert_eq!(host.chat(ChatKind::Chat, "x"), ChatVerdict::Keep);
    }

    fn chat_after_replies(replies: &[u8]) -> ChatVerdict {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let mut host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };
        let mut stream = jvm(port, TOKEN);
        wait_until("live", || link.state.lock().unwrap().phase == Phase::Live);
        for r in replies {
            thread::scope(|scope| {
                let sender = scope.spawn(|| host.chat(ChatKind::InputOpen, ""));
                let mut request = [0; 6];
                stream.read_exact(&mut request).unwrap();
                stream.write_all(&[*r]).unwrap();
                sender.join().unwrap();
            });
        }
        let verdict = thread::scope(|scope| {
            let sender = scope.spawn(|| host.chat(ChatKind::Chat, "late"));
            let mut request = [0; 10];
            stream.read_exact(&mut request).unwrap();
            sender.join().unwrap() // Java never answers
        });
        drop(stream);
        host.stop();
        worker.join().unwrap();
        verdict
    }

    #[test]
    fn a_timed_out_chat_line_is_kept_unless_java_keeps_the_chat() {
        assert_eq!(chat_after_replies(&[]), ChatVerdict::Keep);
        assert_eq!(chat_after_replies(&[3, 0]), ChatVerdict::Keep);
    }

    #[test]
    fn a_timed_out_chat_line_is_not_printed_twice_when_java_keeps_the_chat() {
        assert_eq!(chat_after_replies(&[3]), ChatVerdict::Cancel);
    }

    #[test]
    fn a_chat_line_over_the_java_cap_is_kept_unsent_and_the_next_line_is_still_served() {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let mut host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };
        let mut stream = jvm(port, TOKEN);
        wait_until("live", || link.state.lock().unwrap().phase == Phase::Live);

        // Chat.java throws above 64 KiB, which would end the host: a server line this long must never be sent.
        let big = "§".repeat(33_000);
        assert!(big.len() > 1 << 16);
        assert_eq!(host.chat(ChatKind::Chat, &big), ChatVerdict::Keep);

        let verdict = thread::scope(|scope| {
            let sender = scope.spawn(|| host.chat(ChatKind::Chat, "ok"));
            let mut request = [0; 8];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, &[b'C', 0, 0, 0, 0, 2, b'o', b'k']);
            stream.write_all(&[1]).unwrap();
            sender.join().unwrap()
        });
        assert_eq!(verdict, ChatVerdict::Cancel);

        host.stop();
        worker.join().unwrap();
    }

    #[test]
    fn a_player_tick_without_an_answer_stalls_until_the_late_answer_and_that_answer_is_dropped() {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let mut host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };
        let mut stream = jvm(port, TOKEN);
        wait_until("live", || link.state.lock().unwrap().phase == Phase::Live);
        let state = PlayerState::default();

        assert_eq!(host.player_tick(state), None);
        let start = Instant::now();
        assert_eq!(host.player_tick(state), None);
        assert!(
            start.elapsed() < PLAYER_TICK_WAIT / 2,
            "a stalled tick must not wait"
        );
        let mut request = [0; 8];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(request[0], b'P');

        stream.write_all(&[1, 0]).unwrap();
        wait_until("the late answer", || !link.state.lock().unwrap().stalled);
        let answer = thread::scope(|scope| {
            let ticker = scope.spawn(|| host.player_tick(state));
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request[0], b'P');
            stream.write_all(&[0, 1]).unwrap();
            ticker.join().unwrap()
        });
        assert_eq!(
            answer,
            Some(PlayerInput {
                sprinting: false,
                sneaking: true
            })
        );

        host.stop();
        worker.join().unwrap();
    }

    #[test]
    fn a_world_values_request_returns_what_the_jvm_answers() {
        let (l, port) = listener();
        let link = link();
        let child = Arc::new(Mutex::new(Command::new("sleep").arg("10").spawn().unwrap()));
        let mut host = ModHost {
            child: Arc::clone(&child),
            link: Arc::clone(&link),
        };
        let worker = {
            let (child, link) = (Arc::clone(&child), Arc::clone(&link));
            thread::spawn(move || run(&l, &child, &link, TOKEN))
        };
        let mut stream = jvm(port, TOKEN);
        wait_until("live", || link.state.lock().unwrap().phase == Phase::Live);
        let state = WorldState {
            world_time: 18000,
            ..WorldState::default()
        };

        let answer = thread::scope(|scope| {
            let asker = scope.spawn(|| host.overrides(state));
            let mut request = [0; 54];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request[0], b'O');
            assert_eq!(request[22..30], 18000i64.to_be_bytes());
            // PolyTime at noon: only the time is valid.
            let mut reply = vec![1];
            reply.extend(6000i64.to_be_bytes());
            reply.extend([0; 36]);
            stream.write_all(&reply).unwrap();
            asker.join().unwrap()
        });
        assert_eq!(
            answer,
            Some(Overrides {
                time: Some(6000),
                ..Overrides::default()
            })
        );

        host.stop();
        worker.join().unwrap();
    }

    #[test]
    fn connect_gives_up_when_the_jvm_exits_first() {
        let (l, _) = listener();
        let child = Mutex::new(Command::new("true").spawn().unwrap());
        let e = connect(&l, &child, &link(), TOKEN).unwrap_err();
        assert!(e.to_string().contains("exited"), "{e}");
    }

    #[test]
    fn connect_closes_a_hello_with_the_wrong_token_and_waits_for_the_right_one() {
        let (l, port) = listener();
        let link = link();
        let child = Mutex::new(Command::new("sleep").arg("10").spawn().unwrap());
        let client = thread::spawn(move || {
            let mut bad = jvm(port, &"x".repeat(TOKEN.len()));
            let closed = matches!(bad.read(&mut [0; 1]), Ok(0) | Err(_));
            (closed, jvm(port, TOKEN))
        });
        let stream = connect(&l, &child, &link, TOKEN);
        let (closed, _good) = client.join().unwrap();
        let _ = child.lock().unwrap().kill();
        assert!(stream.is_ok() && closed);
        let s = link.state.lock().unwrap();
        assert!(s.phase == Phase::Live);
        assert_eq!(s.brand.as_deref(), Some("fml,forge"));
        assert!(s.hooks.changed(
            "net/minecraft/client/gui/GuiIngame",
            "func_175180_a",
            "(F)V"
        ));
    }

    #[test]
    fn a_recolored_svg_is_one_white_texture_and_the_tint_carries_the_color() {
        let (mut known, mut textures) = (HashMap::new(), Vec::new());
        let mut get = |argb, recolor, size| {
            texture("i", SVG, size, (argb, recolor), &mut known, &mut textures).unwrap()
        };
        let (red, red_tint) = get(0xFFFF_0000, true, (16, 16));
        let (green, green_tint) = get(0xFF00_FF00, true, (16, 16));
        let (plain, _) = get(0xFFFF_FFFF, false, (16, 16));
        let (bigger, _) = get(0xFFFF_0000, true, (32, 32));
        assert_eq!(red, green);
        assert_eq!((red_tint, green_tint), (0xFFFF_0000, 0xFF00_FF00));
        assert!(plain != red && bigger != red);
        assert_eq!(textures.len(), 3);
        let white = textures.iter().find(|(k, _)| *k == red).unwrap();
        assert_eq!(decode(&white.1)[..4], [255, 255, 255, 255]);
        let own = textures.iter().find(|(k, _)| *k == plain).unwrap();
        assert_eq!(decode(&own.1)[..4], [255, 0, 0, 255]);
    }
}
