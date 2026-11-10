//! Rust side of the mod host. See the README of this crate.

pub mod keys;
pub mod wire;

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
    /// The JVM has not said hello yet; input is dropped.
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
        if s.phase == Phase::Live {
            s.input.events.push((e, screen));
        }
    }

    /// The draw commands of the last finished frame; empty once the JVM is gone.
    pub fn commands(&self) -> Vec<DrawCmd> {
        self.link.state.lock().unwrap().commands.clone()
    }

    pub fn stop(&mut self) {
        {
            let mut s = self.link.state.lock().unwrap();
            s.phase = Phase::Done;
            s.commands.clear();
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
        let cmds = wire::read_commands(&mut stream)?;
        link.state.lock().unwrap().commands = cmds;
    }
}

impl Config {
    /// Read `FERROCRAFT_MODHOST_JAVA`, `FERROCRAFT_MODHOST_CP` (a platform path list) and
    /// `FERROCRAFT_MODHOST_GAMEDIR`. `None` when the java variable is unset: no mods then.
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

/// Draw the commands of a mod frame into the HUD's `Gui`, over the vanilla HUD.
pub fn draw(g: &mut gui::Gui, cmds: &[DrawCmd]) {
    for c in cmds {
        match c {
            DrawCmd::Rect {
                rect: [x0, y0, x1, y1],
                argb,
            } => {
                g.fill(
                    x0.round() as i32,
                    y0.round() as i32,
                    x1.round() as i32,
                    y1.round() as i32,
                    *argb,
                );
            }
            DrawCmd::Gradient {
                rect: [x0, y0, x1, y1],
                top,
                bottom,
            } => {
                g.gradient(
                    x0.round() as i32,
                    y0.round() as i32,
                    x1.round() as i32,
                    y1.round() as i32,
                    *top,
                    *bottom,
                );
            }
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
        }
    }
}
