//! Rust side of the mod host. See the README of this crate.

pub mod keys;
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

pub use wire::{DrawCmd, Event, Input};

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
    commands: Vec<DrawCmd>,
    /// A screen a mod opened is current in the JVM.
    screen: bool,
    /// The files the JVM defined, by name.
    sources: HashMap<String, Vec<u8>>,
    /// Texture names handed to the renderer, and whether the file decoded.
    textures: HashMap<String, bool>,
}

struct Link {
    state: Mutex<Shared>,
    view: Condvar,
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
        let child = Command::new(&cfg.java)
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
                    commands: Vec::new(),
                    screen: false,
                    sources: HashMap::new(),
                    textures: HashMap::new(),
                }),
                view: Condvar::new(),
            }),
        };
        let (child, link) = (Arc::clone(&host.child), Arc::clone(&host.link));
        thread::spawn(move || run(&listener, &child, &link));
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

    /// Queue key, mouse button and wheel transitions from a winit window event for the next frame.
    /// `screen` is whether a screen was open when the event arrived: the game does not see those.
    pub fn window_event(&self, event: &WindowEvent, screen: bool) {
        let Some(e) = transition(event) else {
            return;
        };
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
                        let px = |v: f32| (v * factor as f32).round().clamp(1.0, MAX_SVG_PX) as u32;
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
            Some(Event::Key {
                code: keys::lwjgl(code)?,
                pressed: k.state == ElementState::Pressed,
                repeat: k.repeat,
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
fn run(listener: &TcpListener, child: &Mutex<Child>, link: &Link) {
    let result = connect(listener, child, link).and_then(|stream| pump(stream, link));
    let mut s = link.state.lock().unwrap();
    if let (Err(e), false) = (result, s.phase == Phase::Done) {
        eprintln!("mod host stopped: {e}");
    }
    s.phase = Phase::Done;
    s.commands.clear();
    s.screen = false;
}

fn connect(listener: &TcpListener, child: &Mutex<Child>, link: &Link) -> io::Result<TcpStream> {
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
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
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut hello = [0; 1];
    stream.read_exact(&mut hello)?;
    if hello[0] != b'H' {
        return Err(io::Error::other("the JVM sent no hello"));
    }
    stream.set_read_timeout(None)?;
    let mut s = link.state.lock().unwrap();
    if s.phase != Phase::Starting {
        return Err(io::Error::other("stopped"));
    }
    s.phase = Phase::Live;
    Ok(stream)
}

/// One frame per `set_view`: send the input and the events since the last frame, read the commands.
fn pump(mut stream: TcpStream, link: &Link) -> io::Result<()> {
    loop {
        let input = {
            let mut s = link.state.lock().unwrap();
            while s.phase == Phase::Live && !s.fresh {
                s = link.view.wait(s).unwrap();
            }
            if s.phase != Phase::Live {
                return Ok(());
            }
            s.fresh = false;
            let input = s.input.clone();
            s.input.events.clear();
            input
        };
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
/// at `size` pixels, in the tint's color when `recolor`. `None` when the file does not decode (logged once).
fn texture(
    name: &str,
    data: &[u8],
    size: (u32, u32),
    (argb, recolor): (u32, bool),
    known: &mut HashMap<String, bool>,
    textures: &mut Vec<(String, Vec<u8>)>,
) -> Option<(String, u32)> {
    let svg = data.trim_ascii_start().starts_with(b"<");
    let color = (svg && recolor).then_some(argb & 0x00FF_FFFF);
    let key = match (svg, color) {
        (false, _) => format!("modhost/{name}"),
        (true, None) => format!("modhost/{name}@{}x{}", size.0, size.1),
        (true, Some(c)) => format!("modhost/{name}@{}x{}#{c:06x}", size.0, size.1),
    };
    let ok = *known.entry(key.clone()).or_insert_with(|| {
        let png = if svg {
            rasterize(data, size, color)
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
    ok.then(|| {
        (
            key,
            if color.is_some() {
                argb | 0x00FF_FFFF
            } else {
                argb
            },
        )
    })
}

/// An SVG file as a PNG of `w` x `h` pixels; with `color` every pixel takes that RGB and keeps its alpha.
fn rasterize(svg: &[u8], (w, h): (u32, u32), color: Option<u32>) -> Result<Vec<u8>, String> {
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
        if let Some(c) = color {
            p[..3].copy_from_slice(&c.to_be_bytes()[1..]);
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
