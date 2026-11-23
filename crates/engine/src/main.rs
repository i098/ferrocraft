//! `ferrocraft-engine`: the Rust client that the boot jar launches.

mod app;
mod args;
mod arm_look;
mod game;
mod game_state;
mod link;
mod log;
mod outline;
mod skins;
mod swing;
mod tag_gate;
mod ui;
mod weather;
mod x11_focus;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    let started = std::time::Instant::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match args::launch_token(&argv, std::io::stdin().lock())
        .and_then(|token| args::parse(&argv, std::env::var("FERROCRAFT_SERVER").ok(), token))
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ferrocraft-engine: {e}");
            std::process::exit(2);
        }
    };
    log::install_panic_hook();

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::new(args);
    event_loop.run_app(&mut app).expect("run");
    log::line(&format!(
        "event loop ended after {}s (window closed, quit or deadline)",
        started.elapsed().as_secs()
    ));
}
