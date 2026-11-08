//! `ferrocraft-launcher install`: write the `ferrocraft-1.8.9` version into a `.minecraft` dir.

mod detect;

use detect::Env;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

const BASE_VERSION: &str = "1.8.9";
const VERSION_ID: &str = "ferrocraft-1.8.9";
const BOOT_COORD: &str = "net.ferrocraft:boot:0.1.0";
const FABRIC_JAR: &str = "ferrocraft-fabric.jar";
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

fn engine_file_name(engine: &Path) -> &'static str {
    if engine
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        "ferrocraft-engine.exe"
    } else {
        "ferrocraft-engine"
    }
}

fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst.parent().unwrap()).map_err(|e| format!("{}: {e}", dst.display()))?;
    fs::copy(src, dst).map_err(|e| format!("{}: {e}", src.display()))?;
    Ok(())
}

/// Official launcher: the version json, the boot jar library and the engine. Returns what it wrote.
fn install_official(mc: &Path, boot_jar: &Path, engine: &Path) -> Result<Vec<PathBuf>, String> {
    if !mc.join("versions").join(BASE_VERSION).is_dir() {
        eprintln!("note: versions/{BASE_VERSION} is not installed yet; the launcher fetches it on first run");
    }
    let vdir = mc.join("versions").join(VERSION_ID);
    let jar_dst = mc.join(library_path(BOOT_COORD));
    let engine_dir = mc.join("ferrocraft");
    fs::create_dir_all(&vdir).map_err(|e| format!("{}: {e}", vdir.display()))?;
    let json = serde_json::to_string_pretty(&version_json()).unwrap();
    fs::write(vdir.join(format!("{VERSION_ID}.json")), json).map_err(|e| e.to_string())?;
    copy(boot_jar, &jar_dst)?;
    copy(engine, &engine_dir.join(engine_file_name(engine)))?;
    Ok(vec![vdir, jar_dst, engine_dir])
}

/// Fabric launchers (OneClient, Prism, MultiMC): the mod jar in each mods dir, one shared engine.
fn install_fabric(
    mods_dirs: &[PathBuf],
    fabric_jar: &Path,
    engine: &Path,
    home: &Path,
) -> Result<Vec<PathBuf>, String> {
    let engine_dst = home.join(".ferrocraft").join(engine_file_name(engine));
    copy(engine, &engine_dst)?;
    let mut written = vec![engine_dst];
    for dir in mods_dirs {
        let dst = dir.join(FABRIC_JAR);
        copy(fabric_jar, &dst)?;
        written.push(dst);
    }
    Ok(written)
}

fn manifest_path(home: &Path) -> PathBuf {
    home.join(".ferrocraft").join("install-manifest.txt")
}

/// Add `new` to the manifest of installed paths, which `uninstall` reads.
fn record(manifest: &Path, new: &[PathBuf]) -> Result<(), String> {
    let mut all: BTreeSet<String> = fs::read_to_string(manifest)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    all.extend(new.iter().map(|p| p.display().to_string()));
    let body: Vec<_> = all.into_iter().collect();
    fs::create_dir_all(manifest.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(manifest, body.join("\n") + "\n").map_err(|e| format!("{}: {e}", manifest.display()))
}

/// Remove every path in the manifest, then the manifest. Returns the removed paths.
fn uninstall(manifest: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("{}: {e} (nothing installed?)", manifest.display()))?;
    let mut removed = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let p = Path::new(line);
        let res = if p.is_dir() {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p)
        };
        match res {
            Ok(()) => removed.push(line.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{line}: {e}")),
        }
    }
    fs::remove_file(manifest).map_err(|e| e.to_string())?;
    Ok(removed)
}

/// Copy this executable to `~/.ferrocraft`, so `uninstall` works after a temp download dir is gone.
fn keep_launcher(home: &Path) -> Result<PathBuf, String> {
    let me = env::current_exe().map_err(|e| e.to_string())?;
    let dst = home.join(".ferrocraft").join(me.file_name().unwrap());
    if me != dst {
        copy(&me, &dst)?;
    }
    Ok(dst)
}

/// Files to install; each defaults to the file next to this executable (the release archive layout).
struct Files {
    boot: PathBuf,
    engine: PathBuf,
    fabric: PathBuf,
}

#[derive(Default)]
struct Opts {
    mc: Option<PathBuf>,
    oneclient: Option<PathBuf>,
    prism: Option<PathBuf>,
    boot: Option<PathBuf>,
    engine: Option<PathBuf>,
    fabric: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let val = PathBuf::from(it.next().ok_or(format!("{flag} needs a value"))?);
        let slot = match flag.as_str() {
            "--minecraft-dir" => &mut o.mc,
            "--oneclient-dir" => &mut o.oneclient,
            "--prism-dir" => &mut o.prism,
            "--boot-jar" => &mut o.boot,
            "--engine" => &mut o.engine,
            "--fabric-jar" => &mut o.fabric,
            _ => return Err(format!("unknown flag {flag}")),
        };
        *slot = Some(val);
    }
    Ok(o)
}

fn files(o: &Opts) -> Files {
    let exe = env::current_exe().ok();
    let beside = |name: &str| {
        exe.as_ref()
            .and_then(|e| e.parent())
            .unwrap_or(Path::new("."))
            .join(name)
    };
    let engine = if cfg!(windows) {
        "ferrocraft-engine.exe"
    } else {
        "ferrocraft-engine"
    };
    Files {
        boot: o
            .boot
            .clone()
            .unwrap_or_else(|| beside("ferrocraft-boot.jar")),
        engine: o.engine.clone().unwrap_or_else(|| beside(engine)),
        fabric: o.fabric.clone().unwrap_or_else(|| beside(FABRIC_JAR)),
    }
}

fn install(o: &Opts, env: &Env) -> Result<(), String> {
    let f = files(o);
    // A dir flag limits the install to the flagged launchers; none means every launcher found.
    let explicit = o.mc.is_some() || o.oneclient.is_some() || o.prism.is_some();
    let mc = o.mc.clone().or_else(|| {
        let d = env.minecraft_dir();
        (!explicit && d.is_dir()).then_some(d)
    });
    let mut mods = Vec::new();
    for (flag, layout) in [
        (&o.oneclient, detect::oneclient(env, o.oneclient.clone())),
        (&o.prism, detect::prism(env, o.prism.clone())),
    ] {
        if flag.is_some() || !explicit {
            mods.extend(detect::fabric_mods_dirs(&layout));
        }
    }
    if mc.is_none() && mods.is_empty() {
        return Err("no launcher found. Start your launcher once, or pass --minecraft-dir, --oneclient-dir or --prism-dir".into());
    }
    let mut written = Vec::new();
    if let Some(mc) = &mc {
        written.extend(install_official(mc, &f.boot, &f.engine)?);
        println!(
            "official launcher: installed {VERSION_ID} into {}",
            mc.display()
        );
    }
    if !mods.is_empty() {
        written.extend(install_fabric(&mods, &f.fabric, &f.engine, &env.home)?);
        for m in &mods {
            println!(
                "fabric launcher: installed {FABRIC_JAR} into {}",
                m.display()
            );
        }
    }
    record(&manifest_path(&env.home), &written)?;
    println!("Lunar Client and Badlion cannot start custom versions: not installed there.");
    let kept = keep_launcher(&env.home)?;
    println!("To remove everything: {} uninstall", kept.display());
    println!("Then delete {}", kept.parent().unwrap().display());
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let env = Env::current();
    match args.first().map(String::as_str) {
        Some("install") => install(&parse(&args[1..])?, &env),
        Some("uninstall") if args.len() == 1 => {
            for p in uninstall(&manifest_path(&env.home))? {
                println!("removed {p}");
            }
            Ok(())
        }
        _ => Err("usage: ferrocraft-launcher install [--minecraft-dir DIR] [--oneclient-dir DIR] [--prism-dir DIR] [--boot-jar JAR] [--engine BIN] [--fabric-jar JAR]\n       ferrocraft-launcher uninstall".into()),
    }
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
        install_official(&mc, &tmp.join("boot.jar"), &tmp.join("engine")).unwrap();

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
        for (src, dst) in [
            ("e.exe", "ferrocraft-engine.exe"),
            ("e.bin", "ferrocraft-engine"),
        ] {
            fs::write(tmp.join(src), b"bin").unwrap();
            let mc = tmp.join(dst).with_extension("mc");
            install_official(&mc, &tmp.join("boot.jar"), &tmp.join(src)).unwrap();
            assert!(mc.join("ferrocraft").join(dst).is_file(), "{src} -> {dst}");
        }
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn uninstall_removes_what_install_recorded() {
        let tmp = env::temp_dir().join(format!("fc-launcher-un-{}", process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        for f in ["boot.jar", "engine", "mod.jar"] {
            fs::write(tmp.join(f), b"x").unwrap();
        }
        let mods = tmp.join("cluster/mods");
        let mut written =
            install_official(&tmp.join("mc"), &tmp.join("boot.jar"), &tmp.join("engine")).unwrap();
        written.extend(
            install_fabric(
                std::slice::from_ref(&mods),
                &tmp.join("mod.jar"),
                &tmp.join("engine"),
                &tmp.join("home"),
            )
            .unwrap(),
        );
        let manifest = manifest_path(&tmp.join("home"));
        record(&manifest, &written).unwrap();
        record(&manifest, &written).unwrap();
        assert_eq!(
            fs::read_to_string(&manifest).unwrap().lines().count(),
            written.len()
        );
        assert_eq!(uninstall(&manifest).unwrap().len(), written.len());
        assert!(
            !mods.join(FABRIC_JAR).exists() && !tmp.join("mc/versions/ferrocraft-1.8.9").exists()
        );
        assert!(!manifest.exists());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn rejects_bad_args() {
        assert!(run(&["bogus".into()]).is_err());
        assert!(parse(&["--engine".into()]).is_err());
        assert!(parse(&["--nope".into(), "x".into()]).is_err());
    }
}
