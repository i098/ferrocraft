//! `ferrocraft-launcher install`: write the `ferrocraft-1.8.9` version into a `.minecraft` dir.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::{env, fs, process};

const BASE_VERSION: &str = "1.8.9";
const VERSION_ID: &str = "ferrocraft-1.8.9";
const BOOT_COORD: &str = "net.ferrocraft:boot:0.1.0";
const BOOT_MAIN: &str = "net.ferrocraft.boot.Main";

/// Version json: inherits vanilla, our jar is a library so it is ahead of the vanilla jar.
fn version_json() -> Value {
    json!({
        "id": VERSION_ID,
        "inheritsFrom": BASE_VERSION,
        "type": "release",
        "mainClass": BOOT_MAIN,
        "libraries": [{ "name": BOOT_COORD }],
    })
}

/// Maven path of `group:artifact:version` under `libraries/`.
fn library_path(coord: &str) -> PathBuf {
    let mut p = coord.split(':');
    let (g, a, v) = (p.next().unwrap(), p.next().unwrap(), p.next().unwrap());
    let mut path = PathBuf::from("libraries");
    path.extend(g.split('.'));
    path.push(a);
    path.push(v);
    path.push(format!("{a}-{v}.jar"));
    path
}

fn install(mc: &Path, boot_jar: &Path, engine: &Path) -> Result<(), String> {
    if !mc.join("versions").join(BASE_VERSION).is_dir() {
        eprintln!("note: versions/{BASE_VERSION} is not installed yet; the launcher fetches it on first run");
    }
    let vdir = mc.join("versions").join(VERSION_ID);
    let jar_dst = mc.join(library_path(BOOT_COORD));
    let engine_name = if engine.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
        "ferrocraft-engine.exe"
    } else {
        "ferrocraft-engine"
    };
    let engine_dst = mc.join("ferrocraft").join(engine_name);
    for dir in [
        &vdir,
        jar_dst.parent().unwrap(),
        engine_dst.parent().unwrap(),
    ] {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&version_json()).unwrap();
    fs::write(vdir.join(format!("{VERSION_ID}.json")), json).map_err(|e| e.to_string())?;
    fs::copy(boot_jar, &jar_dst).map_err(|e| format!("{}: {e}", boot_jar.display()))?;
    fs::copy(engine, &engine_dst).map_err(|e| format!("{}: {e}", engine.display()))?;
    println!("installed {VERSION_ID} into {}", mc.display());
    Ok(())
}

fn default_minecraft_dir() -> PathBuf {
    let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/minecraft")
    } else if cfg!(windows) {
        PathBuf::from(env::var_os("APPDATA").unwrap_or_default()).join(".minecraft")
    } else {
        home.join(".minecraft")
    }
}

fn run(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("install") {
        return Err(
            "usage: ferrocraft-launcher install --boot-jar JAR --engine BIN [--minecraft-dir DIR]"
                .into(),
        );
    }
    let (mut mc, mut jar, mut engine) = (default_minecraft_dir(), None, None);
    let mut it = args[1..].iter();
    while let Some(flag) = it.next() {
        let val = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--minecraft-dir" => mc = val.into(),
            "--boot-jar" => jar = Some(PathBuf::from(val)),
            "--engine" => engine = Some(PathBuf::from(val)),
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    install(
        &mc,
        &jar.ok_or("--boot-jar is required")?,
        &engine.ok_or("--engine is required")?,
    )
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_writes_version_jar_and_engine() {
        let tmp = env::temp_dir().join(format!("fc-launcher-test-{}", process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("boot.jar"), b"jar").unwrap();
        fs::write(tmp.join("engine"), b"bin").unwrap();
        let mc = tmp.join("mc");
        install(&mc, &tmp.join("boot.jar"), &tmp.join("engine")).unwrap();

        let v: Value = serde_json::from_slice(
            &fs::read(mc.join("versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["inheritsFrom"], "1.8.9");
        assert_eq!(v["mainClass"], BOOT_MAIN);
        assert_eq!(
            fs::read(mc.join("libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar")).unwrap(),
            b"jar"
        );
        assert_eq!(
            fs::read(mc.join("ferrocraft/ferrocraft-engine")).unwrap(),
            b"bin"
        );
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn install_names_engine_by_platform_contract() {
        let tmp = env::temp_dir().join(format!("fc-launcher-name-{}", process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("boot.jar"), b"jar").unwrap();
        for (src, dst) in [("e.exe", "ferrocraft-engine.exe"), ("e.bin", "ferrocraft-engine")] {
            fs::write(tmp.join(src), b"bin").unwrap();
            let mc = tmp.join(dst).with_extension("mc");
            install(&mc, &tmp.join("boot.jar"), &tmp.join(src)).unwrap();
            assert!(mc.join("ferrocraft").join(dst).is_file(), "{src} -> {dst}");
        }
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn rejects_missing_inputs() {
        assert!(run(&["install".into()]).is_err());
        assert!(run(&["bogus".into()]).is_err());
    }
}
