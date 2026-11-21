//! Bakes the rustc version into `FC_RUSTC_VERSION` for the F3 screen.
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = Command::new(rustc)
        .arg("--version")
        .output()
        .expect("run rustc --version");
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text.split_whitespace().nth(1).unwrap_or("unknown");
    println!("cargo:rustc-env=FC_RUSTC_VERSION={version}");
}
