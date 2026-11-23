use std::{env, fs, process::Command};

#[test]
fn install_without_flags_creates_the_default_game_folder() {
    let root = env::temp_dir().join(format!("fc-fresh-install-{}", std::process::id()));
    let home = root.join("home");
    let appdata = root.join("appdata");
    let package = root.join("package");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&package).unwrap();
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_ferrocraft-launcher"));
    let launcher = package.join(binary.file_name().unwrap());
    fs::copy(binary, &launcher).unwrap();
    fs::write(package.join("ferrocraft-boot.jar"), b"boot").unwrap();
    let engine = if cfg!(windows) {
        "ferrocraft-engine.exe"
    } else {
        "ferrocraft-engine"
    };
    fs::write(package.join(engine), b"engine").unwrap();
    let output = Command::new(launcher)
        .arg("install")
        .env("HOME", &home)
        .env("APPDATA", &appdata)
        .env("XDG_DATA_HOME", &appdata)
        .output()
        .unwrap();
    let game = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/minecraft")
    } else if cfg!(windows) {
        appdata.join(".minecraft")
    } else {
        home.join(".minecraft")
    };
    let descriptor = fs::read(game.join("versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json"));
    fs::remove_dir_all(root).unwrap();
    assert!(output.status.success(), "{:?}", output);
    let version: serde_json::Value = serde_json::from_slice(&descriptor.unwrap()).unwrap();
    assert_eq!(version["id"], "ferrocraft-1.8.9");
    assert_eq!(version["inheritsFrom"], "1.8.9");
    assert_eq!(version["mainClass"], "net.ferrocraft.boot.Main");
}
