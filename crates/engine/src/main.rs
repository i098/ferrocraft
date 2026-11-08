//! `ferrocraft-engine`: the Rust client that the boot jar launches.

mod app;
mod args;
mod game;
mod swing;
mod weather;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match args::parse(&argv, std::env::var("FERROCRAFT_SERVER").ok()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ferrocraft-engine: {e}");
            std::process::exit(2);
        }
    };
    let session = match ferrocraft_net_client::Session::connect(&args.server, &args.username) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ferrocraft-engine: cannot join {}: {e}", args.server);
            std::process::exit(1);
        }
    };
    println!(
        "ferrocraft-engine: joined {} as {}",
        args.server, args.username
    );
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::new(args, session);
    event_loop.run_app(&mut app).expect("run");
    if let Some(reason) = app.exit_reason() {
        eprintln!("ferrocraft-engine: disconnected: {reason}");
        std::process::exit(1);
    }
}
