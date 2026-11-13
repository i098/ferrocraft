//! Rust side of the mod host. See the README of this crate.

pub mod keys;
pub mod mirror;
pub mod overrides;
pub mod wire;

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::PhysicalKey;

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

/// What the JVM needs to start.
pub struct Config {
    /// `bin/java` of a Java 8 runtime.
    pub java: PathBuf,
    /// Class path: LaunchWrapper, Forge, the stand-in Minecraft jar, the host classes and the libraries.
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
    /// A player tick or a world values request got no answer in time: no more are asked until the pump has finished the round trip it is in,
    /// and the late answer is dropped.
    stalled: bool,
    /// `ClientBrandRetriever.getClientModName()` from the hello; `None` until the JVM connected.
    brand: Option<String>,
}

struct Link {
    state: Mutex<Shared>,
    view: Condvar,
    answered: Condvar,
}

/// How long the game tick waits for the JVM's answer to a player tick.
const PLAYER_TICK_WAIT: Duration = Duration::from_millis(100);

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
        let child = Command::new(&cfg.java)
            .env("FERROCRAFT_BRIDGE_TOKEN", &token)
            .arg(format!("-Dferro.bridge=127.0.0.1:{port}"))
            .args(["-cp", &cp, "net.minecraft.launchwrapper.Launch"])
            .args([
                "--tweakClass",
                "net.minecraftforge.fml.common.launcher.FMLTweaker",
            ])
            .args(["--tweakClass", "net.ferrocraft.tweak.HostTweaker"])
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
                    commands: Vec::new(),
                    screen: false,
                    sources: HashMap::new(),
                    textures: HashMap::new(),
                    player: None,
                    answer: None,
                    world: None,
                    world_answer: None,
                    stalled: false,
                    brand: None,
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
                DrawCmd::Clip(rect) => g.set_clip(*rect),
                DrawCmd::Define { .. } => {}
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

    /// The brand the hosted `ClientBrandRetriever.getClientModName()` returned (`fml,forge`), once the JVM is
    /// live. The client sends it as `MC|Brand`; `None` means no mod host answers, so the brand stays `vanilla`.
    pub fn client_brand(&self) -> Option<String> {
        let s = self.link.state.lock().unwrap();
        s.brand.clone().filter(|_| s.phase == Phase::Live)
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
    let (stream, brand) = loop {
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
    Ok(stream)
}

/// Read `'H'`, the token the JVM got through its environment and the brand (`writeUTF`: u16 length, bytes).
fn hello(mut stream: TcpStream, token: &str) -> io::Result<(TcpStream, String)> {
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
    stream.set_read_timeout(None)?;
    Ok((stream, brand))
}

/// One frame per `set_view`: send the input and the events since the last frame, read the commands.
fn pump(mut stream: TcpStream, link: &Link) -> io::Result<()> {
    loop {
        let (input, mirror) = {
            let mut s = link.state.lock().unwrap();
            while s.phase == Phase::Live && !s.fresh && s.player.is_none() && s.world.is_none() {
                s = link.view.wait(s).unwrap();
            }
            if s.phase != Phase::Live {
                return Ok(());
            }
            if let Some(p) = s.player.take() {
                drop(s);
                wire::write_player(&mut stream, &p)?;
                let answer = wire::read_player(&mut stream)?;
                let mut s = link.state.lock().unwrap();
                if !s.stalled {
                    s.answer = Some(answer);
                }
                s.stalled = false;
                link.answered.notify_all();
                continue;
            }
            if let Some(w) = s.world.take() {
                drop(s);
                overrides::write_state(&mut stream, &w)?;
                let answer = overrides::read_overrides(&mut stream)?;
                let mut s = link.state.lock().unwrap();
                if !s.stalled {
                    s.world_answer = Some(answer);
                }
                s.stalled = false;
                link.answered.notify_all();
                continue;
            }
            s.fresh = false;
            let input = s.input.clone();
            s.input.events.clear();
            (input, s.mirror.take())
        };
        if let Some(m) = &mirror {
            mirror::write(&mut stream, m)?;
        }
        wire::write_input(&mut stream, &input)?;
        let (screen, cmds) = wire::read_frame(&mut stream)?;
        let mut s = link.state.lock().unwrap();
        s.commands = Vec::with_capacity(cmds.len());
        for c in cmds {
            match c {
                DrawCmd::Define { name, data } => {
                    s.sources.insert(name, data);
                }
                c => s.commands.push(c),
            }
        }
        s.screen = screen;
        s.stalled = false;
    }
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
            java,
            class_path,
            assets_dir: game_dir.join("assets"),
            game_dir,
        })
    }
}

/// Largest side of a rasterized SVG, in window pixels.
const MAX_SVG_PX: f32 = 2048.0;

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Instant;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="red"/></svg>"#;

    fn link() -> Arc<Link> {
        Arc::new(Link {
            state: Mutex::new(Shared {
                phase: Phase::Starting,
                fresh: false,
                input: Input::default(),
                mirror: None,
                commands: Vec::new(),
                screen: false,
                sources: HashMap::new(),
                textures: HashMap::new(),
                player: None,
                answer: None,
                world: None,
                world_answer: None,
                stalled: false,
                brand: None,
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
