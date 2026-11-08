//! `ferrocraft-launcher install`: write the `ferrocraft-1.8.9` version into a `.minecraft` dir.

mod detect;

use detect::Env;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::ErrorKind::NotFound;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

const BASE_VERSION: &str = "1.8.9";
const VERSION_ID: &str = "ferrocraft-1.8.9";
const BOOT_COORD: &str = "net.ferrocraft:boot:0.1.0";
const FABRIC_JAR: &str = "ferrocraft-fabric.jar";
const BOOT_MAIN: &str = "net.ferrocraft.boot.Main";
const ENGINE_NAMES: [&str; 2] = ["ferrocraft-engine", "ferrocraft-engine.exe"];
const ROOT_PREFIX: &str = "root ";

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
        ENGINE_NAMES[1]
    } else {
        ENGINE_NAMES[0]
    }
}

fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst.parent().unwrap()).map_err(|e| format!("{}: {e}", dst.display()))?;
    fs::copy(src, dst).map_err(|e| format!("{}: {e}", src.display()))?;
    Ok(())
}

/// Official launcher: the version json, the boot jar library and the engine.
/// Each path goes into `written` before it is touched, so a failed install still records it.
fn install_official(
    mc: &Path,
    boot_jar: &Path,
    engine: &Path,
    written: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !mc.join("versions").join(BASE_VERSION).is_dir() {
        eprintln!("note: versions/{BASE_VERSION} is not installed yet; the launcher fetches it on first run");
    }
    let vdir = mc.join("versions").join(VERSION_ID);
    let jar_dst = mc.join(library_path(BOOT_COORD));
    let engine_dir = mc.join("ferrocraft");
    written.extend([vdir.clone(), jar_dst.clone(), engine_dir.clone()]);
    fs::create_dir_all(&vdir).map_err(|e| format!("{}: {e}", vdir.display()))?;
    let json = serde_json::to_string_pretty(&version_json()).unwrap();
    fs::write(vdir.join(format!("{VERSION_ID}.json")), json).map_err(|e| e.to_string())?;
    copy(boot_jar, &jar_dst)?;
    copy(engine, &engine_dir.join(engine_file_name(engine)))
}

/// OneClient: the mod jar in each mods dir, one shared engine. Paths go into `written` first.
fn install_fabric(
    mods_dirs: &[PathBuf],
    fabric_jar: &Path,
    engine: &Path,
    home: &Path,
    written: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let engine_dst = home.join(".ferrocraft").join(engine_file_name(engine));
    written.push(engine_dst.clone());
    copy(engine, &engine_dst)?;
    for dir in mods_dirs {
        let dst = dir.join(FABRIC_JAR);
        written.push(dst.clone());
        copy(fabric_jar, &dst)?;
    }
    Ok(())
}

fn manifest_path(home: &Path) -> PathBuf {
    home.join(".ferrocraft").join("install-manifest.txt")
}

/// Add the launcher dirs `install` wrote to (`roots`) and the paths it wrote (`new`) to the
/// manifest, which `uninstall` reads. A root that does not exist yet is skipped.
fn record(manifest: &Path, roots: &[PathBuf], new: &[PathBuf]) -> Result<(), String> {
    let mut all: BTreeSet<String> = fs::read_to_string(manifest)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    all.extend(
        roots
            .iter()
            .filter_map(|r| fs::canonicalize(r).ok())
            .map(|r| format!("{ROOT_PREFIX}{}", r.display())),
    );
    all.extend(new.iter().map(|p| p.display().to_string()));
    let body: Vec<_> = all.into_iter().collect();
    fs::create_dir_all(manifest.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(manifest, body.join("\n") + "\n").map_err(|e| format!("{}: {e}", manifest.display()))
}

/// True when `path` (canonical) is exactly one of the artifacts `install` writes into a recorded root.
fn is_artifact(path: &Path, is_dir: bool, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| {
        let dirs = [r.join("ferrocraft"), r.join("versions").join(VERSION_ID)];
        let files = [
            r.join(FABRIC_JAR),
            r.join(ENGINE_NAMES[0]),
            r.join(ENGINE_NAMES[1]),
            r.join(library_path(BOOT_COORD)),
        ];
        let candidates: &[PathBuf] = if is_dir { &dirs } else { &files };
        candidates.iter().any(|c| c == path)
    })
}

/// `Ok(None)` when the io error is NotFound: the path is already gone.
fn or_gone<T>(res: std::io::Result<T>, line: &str) -> Result<Option<T>, String> {
    match res {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == NotFound => Ok(None),
        Err(e) => Err(format!("{line}: {e}")),
    }
}

/// A manifest line is removable only if it is one of the artifacts `install` writes, directly
/// under a canonical recorded launcher root, and is not a symlink. `Ok(None)` means it is
/// already gone.
fn removable(line: &str, roots: &[PathBuf]) -> Result<Option<(PathBuf, bool)>, String> {
    let refuse = || format!("{line}: not a ferrocraft install path, refusing to remove");
    let p = Path::new(line);
    let (Some(name), Some(parent), true) = (p.file_name(), p.parent(), p.is_absolute()) else {
        return Err(refuse());
    };
    let Some(parent) = or_gone(fs::canonicalize(parent), line)? else {
        return Ok(None);
    };
    let path = parent.join(name);
    let Some(meta) = or_gone(fs::symlink_metadata(&path), line)? else {
        return Ok(None);
    };
    let is_dir = meta.is_dir();
    let ours = (is_dir || meta.is_file()) && is_artifact(&path, is_dir, roots);
    if ours {
        Ok(Some((path, is_dir)))
    } else {
        Err(refuse())
    }
}

/// Remove every install path in the manifest that passes `removable` under a recorded root.
/// A line that fails is skipped, not removed; the manifest stays then. Returns (removed, skipped).
fn uninstall(manifest: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("{}: {e} (nothing installed?)", manifest.display()))?;
    let lines = || text.lines().filter(|l| !l.is_empty());
    let roots: Vec<PathBuf> = lines()
        .filter_map(|l| l.strip_prefix(ROOT_PREFIX))
        .filter_map(|r| fs::canonicalize(r).ok())
        .collect();
    let (mut removed, mut skipped) = (Vec::new(), Vec::new());
    for line in lines().filter(|l| !l.starts_with(ROOT_PREFIX)) {
        let found = match removable(line, &roots) {
            Ok(found) => found,
            Err(why) => {
                skipped.push(why);
                continue;
            }
        };
        let Some((path, is_dir)) = found else {
            continue;
        };
        let res = if is_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match res {
            Ok(()) => removed.push(path.display().to_string()),
            Err(e) if e.kind() == NotFound => {}
            Err(e) => skipped.push(format!("{}: {e}", path.display())),
        }
    }
    if skipped.is_empty() {
        fs::remove_file(manifest).map_err(|e| e.to_string())?;
    }
    Ok((removed, skipped))
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
        ENGINE_NAMES[1]
    } else {
        ENGINE_NAMES[0]
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
    let explicit = o.mc.is_some() || o.oneclient.is_some();
    let mc = o.mc.clone().or_else(|| {
        let d = env.minecraft_dir();
        (!explicit && d.is_dir()).then_some(d)
    });
    let mods = if o.oneclient.is_some() || !explicit {
        detect::fabric_mods_dirs(&detect::oneclient_roots(env, o.oneclient.clone()))
    } else {
        Vec::new()
    };
    if mc.is_none() && mods.is_empty() {
        return Err("no launcher found. Start your launcher once, or pass --minecraft-dir or --oneclient-dir".into());
    }
    let mut written = Vec::new();
    let res: Result<(), String> = (|| {
        if let Some(mc) = &mc {
            install_official(mc, &f.boot, &f.engine, &mut written)?;
            println!(
                "official launcher: installed {VERSION_ID} into {}",
                mc.display()
            );
        }
        if !mods.is_empty() {
            install_fabric(&mods, &f.fabric, &f.engine, &env.home, &mut written)?;
            for m in &mods {
                println!(
                    "fabric launcher: installed {FABRIC_JAR} into {}",
                    m.display()
                );
            }
        }
        Ok(())
    })();
    let mut roots: Vec<PathBuf> = mc.iter().chain(&mods).cloned().collect();
    roots.push(env.home.join(".ferrocraft"));
    record(&manifest_path(&env.home), &roots, &written)?;
    let kept = keep_launcher(&env.home)?;
    res?;
    println!("Lunar Client and Badlion cannot start custom versions: not installed there.");
    println!("To remove everything: {} uninstall", kept.display());
    println!("Then delete {}", kept.parent().unwrap().display());
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let env = Env::current();
    match args.first().map(String::as_str) {
        Some("install") => install(&parse(&args[1..])?, &env),
        Some("uninstall") if args.len() == 1 => {
            let (removed, skipped) = uninstall(&manifest_path(&env.home))?;
            for p in removed {
                println!("removed {p}");
            }
            for p in &skipped {
                eprintln!("warning: skipped {p}");
            }
            if skipped.is_empty() {
                Ok(())
            } else {
                Err(format!("{} manifest paths skipped", skipped.len()))
            }
        }
        _ => Err("usage: ferrocraft-launcher install [--minecraft-dir DIR] [--oneclient-dir DIR] [--boot-jar JAR] [--engine BIN] [--fabric-jar JAR]\n       ferrocraft-launcher uninstall".into()),
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

    fn tmp(tag: &str) -> PathBuf {
        let d = env::temp_dir().join(format!("fc-launcher-{tag}-{}", process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn install_writes_version_jar_and_engine() {
        let tmp = tmp("test");
        fs::write(tmp.join("boot.jar"), b"jar").unwrap();
        fs::write(tmp.join("engine"), b"bin").unwrap();
        let mc = tmp.join("mc");
        install_official(
            &mc,
            &tmp.join("boot.jar"),
            &tmp.join("engine"),
            &mut Vec::new(),
        )
        .unwrap();

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
        let tmp = tmp("name");
        fs::write(tmp.join("boot.jar"), b"jar").unwrap();
        for (src, dst) in [
            ("e.exe", "ferrocraft-engine.exe"),
            ("e.bin", "ferrocraft-engine"),
        ] {
            fs::write(tmp.join(src), b"bin").unwrap();
            let mc = tmp.join(dst).with_extension("mc");
            install_official(
                &mc,
                &tmp.join("boot.jar"),
                &tmp.join(src),
                &mut Vec::new(),
            )
            .unwrap();
            assert!(mc.join("ferrocraft").join(dst).is_file(), "{src} -> {dst}");
        }
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn uninstall_removes_what_install_recorded() {
        let tmp = tmp("un");
        for f in ["boot.jar", "engine", "mod.jar"] {
            fs::write(tmp.join(f), b"x").unwrap();
        }
        let mods = tmp.join("cluster/mods");
        let mut written = Vec::new();
        install_official(
            &tmp.join("mc"),
            &tmp.join("boot.jar"),
            &tmp.join("engine"),
            &mut written,
        )
        .unwrap();
        install_fabric(
            std::slice::from_ref(&mods),
            &tmp.join("mod.jar"),
            &tmp.join("engine"),
            &tmp.join("home"),
            &mut written,
        )
        .unwrap();
        let manifest = manifest_path(&tmp.join("home"));
        let roots = [
            tmp.join("mc"),
            mods.clone(),
            tmp.join("home").join(".ferrocraft"),
        ];
        record(&manifest, &roots, &written).unwrap();
        record(&manifest, &roots, &written).unwrap();
        assert_eq!(
            fs::read_to_string(&manifest).unwrap().lines().count(),
            written.len() + roots.len()
        );
        fs::write(tmp.join("mc/versions/ferrocraft-1.8.9/extra.jar"), b"x").unwrap();
        fs::write(tmp.join("mc/ferrocraft/engine.log"), b"x").unwrap();
        let (removed, skipped) = uninstall(&manifest).unwrap();
        assert_eq!(removed.len(), written.len());
        assert!(skipped.is_empty());
        assert!(
            !mods.join(FABRIC_JAR).exists() && !tmp.join("mc/versions/ferrocraft-1.8.9").exists()
        );
        assert!(!manifest.exists());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn failed_install_still_records_written_paths() {
        let tmp = tmp("partial");
        fs::write(tmp.join("engine"), b"x").unwrap();
        fs::write(tmp.join("mod.jar"), b"x").unwrap();
        let (good, bad) = (tmp.join("good/mods"), tmp.join("bad/mods"));
        fs::create_dir_all(bad.join(FABRIC_JAR)).unwrap();
        let mut written = Vec::new();
        let res = install_fabric(
            &[good.clone(), bad],
            &tmp.join("mod.jar"),
            &tmp.join("engine"),
            &tmp.join("home"),
            &mut written,
        );
        assert!(res.is_err());
        assert!(written.contains(&good.join(FABRIC_JAR)));
        assert!(good.join(FABRIC_JAR).is_file());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn uninstall_refuses_paths_outside_install_artifacts() {
        let tmp = tmp("refuse");
        let mods = tmp.join("cluster/mods");
        fs::create_dir_all(&mods).unwrap();
        let ours = mods.join(FABRIC_JAR);
        fs::write(&ours, b"x").unwrap();
        let other_mods = tmp.join("other/mods");
        fs::create_dir_all(&other_mods).unwrap();
        fs::write(other_mods.join(FABRIC_JAR), b"keep").unwrap();
        let victim = tmp.join("precious");
        fs::create_dir_all(&victim).unwrap();
        fs::write(victim.join("data"), b"keep").unwrap();
        let nested = mods.join("saves/ferrocraft");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("data"), b"keep").unwrap();
        let nested_jar = mods.join("saves").join(FABRIC_JAR);
        fs::write(&nested_jar, b"keep").unwrap();
        let manifest = tmp.join("manifest.txt");
        for bad in [
            victim.display().to_string(),
            victim.join("data").display().to_string(),
            "relative/ferrocraft-fabric.jar".to_string(),
            other_mods.join(FABRIC_JAR).display().to_string(),
            nested.display().to_string(),
            nested_jar.display().to_string(),
        ] {
            fs::write(&ours, b"x").unwrap();
            let _ = fs::remove_file(&manifest);
            record(&manifest, &[mods.clone()], &[ours.clone()]).unwrap();
            let mut text = fs::read_to_string(&manifest).unwrap();
            text.push_str(&format!("{bad}\n"));
            fs::write(&manifest, text).unwrap();
            let (removed, skipped) = uninstall(&manifest).unwrap();
            assert_eq!(removed.len(), 1, "{bad}");
            assert!(skipped.len() == 1 && skipped[0].starts_with(&bad), "{bad}");
            assert!(!ours.exists(), "{bad}");
            assert!(victim.join("data").is_file(), "{bad}");
            assert!(other_mods.join(FABRIC_JAR).is_file(), "{bad}");
            assert!(nested.join("data").is_file() && nested_jar.is_file(), "{bad}");
            assert!(manifest.exists(), "{bad}");
        }
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_does_not_follow_symlinks_out() {
        let tmp = tmp("symlink");
        let outside = tmp.join("outside");
        fs::create_dir_all(outside.join("ferrocraft")).unwrap();
        fs::write(outside.join("ferrocraft/ferrocraft-engine"), b"keep").unwrap();
        let mc = tmp.join("mc");
        fs::create_dir_all(&mc).unwrap();
        std::os::unix::fs::symlink(outside.join("ferrocraft"), mc.join("ferrocraft")).unwrap();
        let manifest = tmp.join("manifest.txt");
        record(&manifest, &[mc.clone()], &[mc.join("ferrocraft")]).unwrap();
        let (removed, skipped) = uninstall(&manifest).unwrap();
        assert!(removed.is_empty() && skipped.len() == 1);
        assert!(outside.join("ferrocraft/ferrocraft-engine").is_file());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn rejects_bad_args() {
        assert!(run(&["bogus".into()]).is_err());
        assert!(parse(&["--engine".into()]).is_err());
        assert!(parse(&["--nope".into(), "x".into()]).is_err());
    }
}
