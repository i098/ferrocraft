//! Rust side of the mod host. See the README of this crate.

pub mod keys;
pub mod wire;

use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::PhysicalKey;

pub use wire::{DrawCmd, Input};

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

struct Shared {
    input: Input,
    commands: Vec<DrawCmd>,
}

/// A running JVM and the last frame of draw commands it sent.
pub struct ModHost {
    child: Child,
    shared: Arc<Mutex<Shared>>,
}

impl ModHost {
    /// Start the JVM and wait until it connects, which is after FML has loaded the mods.
    pub fn start(cfg: &Config) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
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
        let mut host = Self {
            child,
            shared: Arc::new(Mutex::new(Shared {
                input: Input::default(),
                commands: Vec::new(),
            })),
        };
        let (mut stream, _) = listener.accept()?;
        stream.set_nodelay(true)?;
        let mut hello = [0; 1];
        stream.read_exact(&mut hello)?;
        if hello[0] != b'H' {
            host.stop();
            return Err(io::Error::other("the JVM sent no hello"));
        }
        let shared = Arc::clone(&host.shared);
        thread::spawn(move || pump(stream, &shared));
        Ok(host)
    }

    /// Set the window size, GUI scale and mouse for the next frame.
    pub fn set_view(&self, width: i32, height: i32, gui_scale: i32, mouse: (i32, i32)) {
        let mut s = self.shared.lock().unwrap();
        s.input.width = width;
        s.input.height = height;
        s.input.gui_scale = gui_scale;
        s.input.mouse = mouse;
    }

    /// Track key and mouse button state from a winit window event.
    pub fn window_event(&self, event: &WindowEvent) {
        let mut s = self.shared.lock().unwrap();
        match event {
            WindowEvent::KeyboardInput { event: k, .. } => {
                let PhysicalKey::Code(code) = k.physical_key else {
                    return;
                };
                let Some(code) = keys::lwjgl(code) else {
                    return;
                };
                s.input.keys.retain(|&c| c != code);
                if k.state == ElementState::Pressed {
                    s.input.keys.push(code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    _ => return,
                };
                if *state == ElementState::Pressed {
                    s.input.buttons |= bit;
                } else {
                    s.input.buttons &= !bit;
                }
            }
            _ => {}
        }
    }

    /// The draw commands of the last finished frame.
    pub fn commands(&self) -> Vec<DrawCmd> {
        self.shared.lock().unwrap().commands.clone()
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ModHost {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One request and one reply per round, as fast as the JVM answers, until it closes the socket.
fn pump(mut stream: TcpStream, shared: &Mutex<Shared>) {
    loop {
        let input = shared.lock().unwrap().input.clone();
        let reply =
            wire::write_input(&mut stream, &input).and_then(|()| wire::read_commands(&mut stream));
        match reply {
            Ok(cmds) => shared.lock().unwrap().commands = cmds,
            Err(_) => return,
        }
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
