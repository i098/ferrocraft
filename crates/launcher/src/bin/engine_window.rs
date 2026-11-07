//! Stand-in engine: opens a window, prints its X11/Wayland/Win32 handle, exits after a timeout.
//! The `engine` crate replaces this binary.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Rc<Window>>,
    deadline: Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("Ferrocraft 1.8.9");
        let window = Rc::new(el.create_window(attrs).expect("create window"));
        let handle = window.window_handle().expect("window handle").as_raw();
        match handle {
            RawWindowHandle::Xlib(h) => println!("ferrocraft-engine: window xlib id={}", h.window),
            RawWindowHandle::Xcb(h) => println!("ferrocraft-engine: window xcb id={}", h.window),
            other => println!("ferrocraft-engine: window {other:?}"),
        }
        self.window = Some(window);
        el.set_control_flow(ControlFlow::WaitUntil(self.deadline));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if Instant::now() >= self.deadline {
            println!("ferrocraft-engine: exiting");
            el.exit();
        }
    }
}

fn main() {
    // Seconds to keep the window open; default 0 means until closed (24 h cap).
    let secs: u64 = std::env::var("FERROCRAFT_WINDOW_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let secs = if secs == 0 { 86_400 } else { secs };
    println!(
        "ferrocraft-engine: args={:?}",
        std::env::args().skip(1).collect::<Vec<_>>()
    );
    let el = EventLoop::new().expect("event loop");
    let mut app = App {
        window: None,
        deadline: Instant::now() + Duration::from_secs(secs),
    };
    el.run_app(&mut app).expect("run");
}
