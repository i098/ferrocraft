//! `ferrocraft-launcher install`: write the `ferrocraft-1.8.9` version into a `.minecraft` dir.

mod detect;
mod installed_mods;
mod oneclient;

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
const PROFILE_MARKER: &str = ".ferrocraft-profile";

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
    if let (Ok(a), Ok(b)) = (fs::canonicalize(src), fs::canonicalize(dst)) {
        if a == b {
            return Ok(());
        }
    }
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
    installed_mods::check_reinstall(mods_dirs)?;
    let engine_dst = home.join(".ferrocraft").join(engine_file_name(engine));
    written.push(engine_dst.clone());
    copy(engine, &engine_dst)?;
    for dir in mods_dirs {
        let dst = dir.join(FABRIC_JAR);
        written.push(dst.clone());
        let stage = || -> std::io::Result<()> {
            fs::create_dir_all(dir)?;
            let mut temp = tempfile::NamedTempFile::new_in(dir)?;
            std::io::copy(&mut fs::File::open(fabric_jar)?, &mut temp)?;
            temp.persist(&dst)?;
            Ok(())
        };
        stage().map_err(|e| format!("{}: {e}", dst.display()))?;
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
    for p in new {
        let abs = std::path::absolute(p).map_err(|e| format!("{}: {e}", p.display()))?;
        all.insert(abs.display().to_string());
    }
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

fn recorded_mod(line: &str) -> Option<&Path> {
    let path = Path::new(line);
    (path.is_absolute() && path.file_name().is_some_and(|name| name == FABRIC_JAR))
        .then_some(path)
}

/// Remove every install path in the manifest that passes `removable` under a recorded root.
/// A line that fails is skipped, not removed; the manifest stays then. Returns (removed, skipped).
fn uninstall(manifest: &Path, app_data: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("{}: {e} (nothing installed?)", manifest.display()))?;
    let lines = || text.lines().filter(|l| !l.is_empty());
    let roots: Vec<PathBuf> = lines()
        .filter_map(|l| l.strip_prefix(ROOT_PREFIX))
        .filter_map(|r| fs::canonicalize(r).ok())
        .collect();
    let (mut removed, mut skipped) = (Vec::new(), Vec::new());
    let artifacts = || lines().filter(|l| !l.starts_with(ROOT_PREFIX));
    let mods: Vec<_> = artifacts().filter_map(recorded_mod).collect();
    installed_mods::check_uninstall(&mods, app_data)?;
    for line in artifacts() {
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
    copy(&me, &dst)?;
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

fn registered_oneclient_profiles(data: &Path) -> std::io::Result<Vec<(String, String)>> {
    let database = data.join("user_data.db");
    if !database.try_exists()? {
        return Ok(Vec::new());
    }
    let db = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(std::io::Error::other)?;
    let mut query = db
        .prepare("SELECT folder_name, name FROM clusters")
        .map_err(std::io::Error::other)?;
    let rows = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(std::io::Error::other)?;
    rows.collect::<Result<_, _>>().map_err(std::io::Error::other)
}

struct OwnedProfile {
    slot: usize,
    path: PathBuf,
    folder: String,
    name: String,
}

fn scan_oneclient_profiles(
    clusters: &Path,
) -> std::io::Result<(Vec<String>, Option<OwnedProfile>)> {
    let mut disk_names = Vec::new();
    let mut owned: Option<OwnedProfile> = None;
    for entry in fs::read_dir(clusters)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.file_name();
        let Some(folder) = folder.to_str() else {
            continue;
        };
        let path = entry.path();
        let slot = if folder == VERSION_ID {
            Some(1)
        } else {
            folder.strip_prefix(VERSION_ID).and_then(|suffix| {
                let suffix = suffix.strip_prefix('-')?;
                let number = suffix.parse::<usize>().ok()?;
                (number >= 2 && !suffix.starts_with(['0', '+'])).then_some(number)
            })
        };
        let identity = fs::read(path.join(".instance.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(slot) = slot.filter(|_| {
            fs::read(path.join(PROFILE_MARKER)).as_deref().ok() == Some(b"1\n")
        }) {
            let name = identity
                .as_ref()
                .filter(|identity| {
                    identity["mc_version"] == BASE_VERSION
                        && identity["mc_loader"] == "Ornithe"
                        && identity["kind"] == "modded"
                })
                .and_then(|identity| identity["name"].as_str())
                .ok_or_else(|| std::io::Error::other("The Ferrocraft profile is incompatible"))?;
            if owned.as_ref().is_none_or(|profile| slot < profile.slot) {
                owned = Some(OwnedProfile {
                    slot,
                    path,
                    folder: folder.to_owned(),
                    name: name.to_owned(),
                });
            }
        } else if let Some(name) = identity.as_ref().and_then(|identity| identity["name"].as_str()) {
            disk_names.push(name.to_owned());
        }
    }
    Ok((disk_names, owned))
}

/// OneClient 2.12 reads .instance.json on startup to adopt this profile from disk.
fn default_oneclient_profile(data: &Path) -> Result<(PathBuf, String), String> {
    let prepare = || -> std::io::Result<_> {
        let clusters = data.join("clusters");
        fs::create_dir_all(&clusters)?;
        let registered = registered_oneclient_profiles(data)?;
        let (disk_names, owned) = scan_oneclient_profiles(&clusters)?;
        if let Some(OwnedProfile { path, folder, name, .. }) = owned {
            let name = registered
                .iter()
                .find(|(stem, _)| stem == &folder)
                .map_or(name.as_str(), |(_, name)| name);
            if disk_names.iter().any(|n| n == name)
                || registered.iter().any(|(stem, n)| stem != &folder && n == name)
            {
                return Err(std::io::Error::other(format!(
                    "More than one OneClient profile is named {name}; rename the other profile before reinstalling"
                )));
            }
            return Ok((path, name.to_owned()));
        }
        for number in 1.. {
            let (stem, name) = if number == 1 {
                (VERSION_ID.to_owned(), "Ferrocraft 1.8.9".to_owned())
            } else {
                (format!("{VERSION_ID}-{number}"), format!("Ferrocraft 1.8.9 ({number})"))
            };
            if registered.iter().any(|(folder, display)| {
                folder.eq_ignore_ascii_case(&stem) || display == &name
            }) || disk_names.contains(&name) {
                continue;
            }
            let profile = clusters.join(stem);
            match fs::symlink_metadata(&profile) {
                Ok(_) => continue,
                Err(e) if e.kind() == NotFound => {}
                Err(e) => return Err(e),
            }
            let identity = json!({
                "name": name,
                "mc_version": BASE_VERSION,
                "mc_loader": "Ornithe",
                "kind": "modded",
                "user_created": true
            });
            let stage = tempfile::tempdir_in(data)?;
            fs::write(stage.path().join(".instance.json"), identity.to_string())?;
            fs::write(stage.path().join(PROFILE_MARKER), b"1\n")?;
            fs::rename(stage.path(), &profile)?;
            return Ok((profile, name));
        }
        unreachable!()
    };
    prepare().map_err(|e| format!("{}: {e}", data.display()))
}

fn owned_mods_dir(profile: &Path) -> Result<PathBuf, String> {
    let mods = profile.join("mods");
    let check = || -> std::io::Result<()> {
        match fs::symlink_metadata(&mods) {
            Ok(meta)
                if meta.is_dir()
                    && fs::canonicalize(&mods)? == fs::canonicalize(profile)?.join("mods") =>
            {
                Ok(())
            }
            Err(e) if e.kind() == NotFound => Ok(()),
            Err(e) => Err(e),
            Ok(_) => Err(std::io::Error::other("Expected a directory inside the Ferrocraft profile")),
        }
    };
    check().map_err(|e| format!("{}: {e}", mods.display()))?;
    Ok(mods)
}

fn install(o: &Opts, env: &Env) -> Result<(), String> {
    // Explicit paths do not download OneClient or create a new profile.
    let explicit = o.mc.is_some() || o.oneclient.is_some();
    let oneclient = if explicit {
        None
    } else {
        let (data, _) =
            oneclient::data_dirs(&env.data).map_err(|e| format!("OneClient data directory: {e}"))?;
        let app = oneclient::install(&env.home.join(".ferrocraft"))
            .map_err(|e| format!("OneClient install failed: {e}"))?;
        Some((app, default_oneclient_profile(&data)?))
    };
    let mc = o.mc.clone().or_else(|| {
        let d = env.minecraft_dir();
        (!explicit && d.is_dir()).then_some(d)
    });
    let mods = if let Some((_, (profile, _))) = &oneclient {
        vec![owned_mods_dir(profile)?]
    } else {
        o.oneclient
            .as_deref()
            .map_or_else(Vec::new, detect::fabric_mods_dirs)
    };
    if mc.is_none() && mods.is_empty() {
        return Err("no 1.8.9 Fabric or Ornithe cluster found in --oneclient-dir".into());
    }
    install_targets(o, env, mc, mods)?;
    if let Some((app, (_, name))) = oneclient {
        println!(
            "Exit OneClient if it is running. Open OneClient at {} and select {name} after signing in.",
            app.display()
        );
        if cfg!(target_os = "linux") {
            println!("If FUSE is unavailable, run the AppImage with --appimage-extract-and-run.");
        }
    }
    Ok(())
}

fn install_targets(
    o: &Opts,
    env: &Env,
    mc: Option<PathBuf>,
    mods: Vec<PathBuf>,
) -> Result<(), String> {
    let f = files(o);
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
    let kept =
        record(&manifest_path(&env.home), &roots, &written).and_then(|()| keep_launcher(&env.home));
    res?;
    let kept = kept?;
    println!("Lunar Client and Badlion cannot start custom versions: not installed there.");
    println!("To remove the Ferrocraft files: {} uninstall", kept.display());
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let env = Env::current();
    match args.first().map(String::as_str) {
        Some("install") => install(&parse(&args[1..])?, &env),
        Some("uninstall") if args.len() == 1 => {
            let (removed, skipped) = uninstall(&manifest_path(&env.home), &env.data)?;
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
    fn oneclient_install_creates_a_profile_with_the_mod_and_engine() {
        let tmp = tmp("oneclient");
        let env = Env {
            home: tmp.join("home"),
            data: tmp.join("data"),
        };
        let fabric = tmp.join("mod.jar");
        let engine = tmp.join("engine");
        fs::write(&fabric, b"mod").unwrap();
        fs::write(&engine, b"engine").unwrap();
        let (profile, _) = default_oneclient_profile(&env.data.join("Polyfrost/OneClient")).unwrap();
        install(
            &Opts {
                oneclient: Some(env.data.join("Polyfrost/OneClient")),
                fabric: Some(fabric),
                engine: Some(engine),
                ..Opts::default()
            },
            &env,
        )
        .unwrap();

        let identity = fs::read(profile.join(".instance.json")).unwrap();
        let identity: Value = serde_json::from_slice(&identity).unwrap();
        assert_eq!(identity["mc_version"], "1.8.9");
        assert_eq!(identity["mc_loader"], "Ornithe");
        assert_eq!(identity["kind"], "modded");
        let jar = profile.join("mods").join(FABRIC_JAR);
        assert_eq!(fs::read(&jar).unwrap(), b"mod");
        let engine = env.home.join(".ferrocraft/ferrocraft-engine");
        assert_eq!(fs::read(&engine).unwrap(), b"engine");
        uninstall(&manifest_path(&env.home), &env.data).unwrap();
        assert!(!jar.exists());
        assert!(!engine.exists());
        assert!(profile.join(".instance.json").exists());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn default_install_refuses_linked_mods_directory() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let env = Env {
            home: tmp.path().join("home"),
            data: tmp.path().join("data"),
        };
        let installed = env.home.join(".ferrocraft/oneclient-2.12.0/OneClient.AppImage");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::write(installed, []).unwrap();
        let data = env.data.join("Polyfrost/OneClient");
        let (profile, _) = default_oneclient_profile(&data).unwrap();
        let personal = data.join("clusters/personal/mods");
        fs::create_dir_all(&personal).unwrap();
        symlink(&personal, profile.join("mods")).unwrap();
        let jar = tmp.path().join("incoming.jar");
        let engine = tmp.path().join("engine");
        fs::write(&jar, b"bootstrap").unwrap();
        fs::write(&engine, b"engine").unwrap();

        let opts = Opts {
            fabric: Some(jar),
            engine: Some(engine),
            ..Opts::default()
        };
        let err = install(&opts, &env).unwrap_err();
        assert!(err.contains("Expected a directory inside the Ferrocraft profile"), "{err}");
        assert!(fs::read_dir(personal).unwrap().next().is_none());
        assert!(!manifest_path(&env.home).exists());
        fs::remove_file(profile.join("mods")).unwrap();
        install(&opts, &env).unwrap();
        assert_eq!(fs::read(profile.join("mods").join(FABRIC_JAR)).unwrap(), b"bootstrap");
    }

    #[test]
    fn profile_collision_preserves_matching_personal_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let personal = tmp.path().join("clusters/ferrocraft-1.8.9");
        fs::create_dir_all(&personal).unwrap();
        let original = br#"{"name":"Personal","mc_version":"1.8.9","mc_loader":"Ornithe","kind":"modded"}"#;
        fs::write(personal.join(".instance.json"), original).unwrap();
        let (profile, name) = default_oneclient_profile(tmp.path()).unwrap();
        assert_eq!(profile, tmp.path().join("clusters/ferrocraft-1.8.9-2"));
        assert_eq!(name, "Ferrocraft 1.8.9 (2)");
        assert_eq!(fs::read(personal.join(".instance.json")).unwrap(), original);
        assert!(!personal.join("mods").exists());
        assert_eq!(default_oneclient_profile(tmp.path()).unwrap().0, profile);
        fs::remove_dir_all(personal).unwrap();
        assert_eq!(default_oneclient_profile(tmp.path()).unwrap(), (profile, name));
    }

    #[test]
    fn profile_allocation_reserves_registered_folder_and_display_names() {
        let tmp = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(tmp.path().join("user_data.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE clusters(folder_name, name);
             INSERT INTO clusters VALUES ('ferrocraft-1.8.9', 'Personal');
             INSERT INTO clusters VALUES ('different-folder', 'Ferrocraft 1.8.9 (2)');",
        )
        .unwrap();
        let (profile, name) = default_oneclient_profile(tmp.path()).unwrap();
        assert_eq!(profile, tmp.path().join("clusters/ferrocraft-1.8.9-3"));
        assert_eq!(name, "Ferrocraft 1.8.9 (3)");
    }

    fn write_fabric_jar(path: &Path, id: &str, version: &str) {
        use std::io::Write;

        let mut jar = zip::ZipWriter::new(fs::File::create(path).unwrap());
        jar.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        let metadata = json!({ "schemaVersion": 1, "id": id, "version": version });
        jar.write_all(metadata.to_string().as_bytes()).unwrap();
        jar.finish().unwrap();
    }

    #[test]
    fn uninstall_refuses_database_owned_mod_before_removing_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("oneclient");
        let profile = data.join("clusters/ferrocraft-1.8.9");
        let mods = profile.join("mods");
        let jar = mods.join(FABRIC_JAR);
        let home = tmp.path().join("home");
        let engine = home.join(".ferrocraft/ferrocraft-engine");
        let mc = tmp.path().join("mc");
        let version = mc.join("versions").join(VERSION_ID);
        fs::create_dir_all(&mods).unwrap();
        fs::create_dir_all(engine.parent().unwrap()).unwrap();
        fs::create_dir_all(&version).unwrap();
        fs::write(&engine, b"engine").unwrap();
        let cached = data.join("old.jar");
        write_fabric_jar(&cached, "ferrocraft", "1.0.0");
        let db = rusqlite::Connection::open(data.join("user_data.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE clusters(id, folder_name);
             CREATE TABLE artifacts(hash, content_type, path, file_name);
             CREATE TABLE cluster_artifacts(cluster_id, hash, cluster_file_name, enabled);
             INSERT INTO clusters VALUES (1, 'ferrocraft-1.8.9');
             INSERT INTO artifacts VALUES ('old', 0, 'old.jar', 'cache-renamed.jar');
             INSERT INTO cluster_artifacts VALUES (1, 'old', 'renamed-bootstrap.jar', 1);",
        )
        .unwrap();
        let manifest = manifest_path(&home);
        record(
            &manifest,
            &[mods.clone(), engine.parent().unwrap().into(), mc],
            &[jar.clone(), engine.clone(), version.clone()],
        )
        .unwrap();

        let incoming = tmp.path().join("incoming.jar");
        let new_engine = tmp.path().join("incoming-engine");
        write_fabric_jar(&incoming, "ferrocraft", "2.0.0");
        fs::write(&new_engine, b"new engine").unwrap();
        let old_jar = fs::read(&cached).unwrap();
        fs::write(&jar, &old_jar).unwrap();
        for manifest in [None, Some("invalid"), Some(r#"{"version":1,"cluster_id":2,"files":[]}"#)] {
            if let Some(contents) = manifest {
                fs::write(profile.join(".oneclient-mods.json"), contents).unwrap();
            }
            assert!(install_fabric(
                std::slice::from_ref(&mods),
                &incoming,
                &new_engine,
                &home,
                &mut Vec::new(),
            )
            .is_err());
        }
        db.execute("UPDATE cluster_artifacts SET enabled = 0", []).unwrap();
        fs::remove_file(profile.join(".oneclient-mods.json")).unwrap();
        assert!(install_fabric(
            std::slice::from_ref(&mods),
            &cached,
            &new_engine,
            &home,
            &mut Vec::new(),
        )
        .is_err());
        assert_eq!(fs::read(&engine).unwrap(), b"engine");
        assert_eq!(fs::read(&jar).unwrap(), old_jar);

        for materialized in [true, false] {
            if materialized {
                fs::write(&jar, b"managed copy").unwrap();
            } else {
                fs::remove_file(&jar).unwrap();
            }
            let err = uninstall(&manifest, &tmp.path().join("app-data")).unwrap_err();
            assert!(err.contains("OneClient"), "{err}");
            assert_eq!(jar.exists(), materialized);
            assert_eq!(fs::read(&engine).unwrap(), b"engine");
            assert!(version.is_dir());
            assert!(manifest.is_file());
        }
        db.execute("UPDATE cluster_artifacts SET cluster_file_name = 'ferrocraft-fabric.jar'", [])
            .unwrap();

        write_fabric_jar(&cached, "unrelated", "1.0.0");
        let unrelated = fs::read(&cached).unwrap();
        fs::write(&jar, &unrelated).unwrap();
        let err = install_fabric(
            std::slice::from_ref(&mods),
            &incoming,
            &new_engine,
            &home,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(err.contains("mod at the install destination"), "{err}");
        let err = uninstall(&manifest, &tmp.path().join("app-data")).unwrap_err();
        assert!(err.contains("OneClient still manages"), "{err}");
        assert_eq!(fs::read(&jar).unwrap(), unrelated);
        assert_eq!(fs::read(&engine).unwrap(), b"engine");
        assert!(manifest.is_file());
        fs::remove_file(&jar).unwrap();
        fs::write(&cached, &old_jar).unwrap();

        drop(db);
        let app_data = tmp.path().join("app-data");
        let moved = app_data.join("Polyfrost/OneClient");
        fs::create_dir_all(&moved).unwrap();
        fs::copy(data.join("user_data.db"), moved.join("user_data.db")).unwrap();
        fs::copy(&cached, moved.join("old.jar")).unwrap();
        let db = rusqlite::Connection::open(moved.join("user_data.db")).unwrap();
        db.execute("UPDATE cluster_artifacts SET cluster_file_name='renamed.jar'", [])
            .unwrap();
        let err = uninstall(&manifest, &app_data).unwrap_err();
        assert!(err.contains("renamed.jar") && err.contains("not at a recorded install path"));
        assert!(err.contains("remove it in OneClient"), "{err}");
        assert!(engine.is_file() && version.is_dir() && manifest.is_file());

        db.execute("DELETE FROM cluster_artifacts", []).unwrap();
        let err = uninstall(&manifest, &app_data).unwrap_err();
        assert!(err.contains("remove the mod from the OneClient installation"), "{err}");
        assert!(!err.contains("OneClient Storage"), "{err}");
        let previous = tmp.path().join("intermediate-root");
        fs::rename(&data, &previous).unwrap();
        fs::write(
            moved.join("settings.json"),
            json!({ "previous_data_dir": previous }).to_string(),
        )
        .unwrap();
        let err = uninstall(&manifest, &app_data).unwrap_err();
        let recovery = format!("clear {} in OneClient Storage", previous.display());
        assert!(err.contains(&recovery), "{err}");
        assert!(engine.is_file() && version.is_dir() && manifest.is_file());

        fs::remove_dir_all(previous).unwrap();
        let unrelated = moved.join(FABRIC_JAR);
        write_fabric_jar(&unrelated, "unrelated", "1.0.0");
        db.execute_batch(
            "INSERT INTO artifacts VALUES ('other', 0, 'ferrocraft-fabric.jar', 'ferrocraft-fabric.jar');
             INSERT INTO cluster_artifacts VALUES (1, 'other', 'ferrocraft-fabric.jar', 1);",
        )
        .unwrap();
        let (removed, skipped) = uninstall(&manifest, &app_data).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(skipped.is_empty());
        assert!(unrelated.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn reinstall_replaces_mod_link_without_changing_cache() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let mods = tmp.path().join("cluster/mods");
        fs::create_dir_all(&mods).unwrap();
        let jar = mods.join(FABRIC_JAR);
        let cached = tmp.path().join("cached.jar");
        let incoming = tmp.path().join("new.jar");
        let engine = tmp.path().join("engine");
        fs::write(&cached, b"cached").unwrap();
        fs::write(&incoming, b"new").unwrap();
        fs::write(&engine, b"engine").unwrap();
        symlink(&cached, &jar).unwrap();

        let home = tmp.path().join("home");
        let manifest = manifest_path(&home);
        let mut written = Vec::new();
        install_fabric(
            std::slice::from_ref(&mods),
            &incoming,
            &engine,
            &home,
            &mut written,
        )
        .unwrap();
        assert_eq!(fs::read(&cached).unwrap(), b"cached");
        assert_eq!(fs::read(&jar).unwrap(), b"new");
        assert!(!fs::symlink_metadata(&jar).unwrap().is_symlink());

        fs::remove_file(&jar).unwrap();
        symlink(&cached, &jar).unwrap();
        record(&manifest, &[mods, home.join(".ferrocraft")], &written).unwrap();
        assert!(uninstall(&manifest, &tmp.path().join("app-data"))
            .unwrap_err()
            .contains("OneClient"));
        assert_eq!(fs::read_link(&jar).unwrap(), cached);
        assert_eq!(
            fs::read(home.join(".ferrocraft/ferrocraft-engine")).unwrap(),
            b"engine"
        );
        assert!(manifest.is_file());
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
            install_official(&mc, &tmp.join("boot.jar"), &tmp.join(src), &mut Vec::new()).unwrap();
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
        let (removed, skipped) = uninstall(&manifest, &tmp.join("app-data")).unwrap();
        assert_eq!(removed.len(), written.len());
        assert!(skipped.is_empty());
        assert!(
            !mods.join(FABRIC_JAR).exists() && !tmp.join("mc/versions/ferrocraft-1.8.9").exists()
        );
        assert!(!manifest.exists());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn uninstall_removes_install_made_with_relative_dir() {
        let tmp = tmp("rel");
        fs::write(tmp.join("boot.jar"), b"jar").unwrap();
        fs::write(tmp.join("engine"), b"bin").unwrap();
        let rel = PathBuf::from(format!("fc-rel-mc-{}", process::id()));
        let mut written = Vec::new();
        let res = install_official(
            &rel,
            &tmp.join("boot.jar"),
            &tmp.join("engine"),
            &mut written,
        );
        let manifest = tmp.join("manifest.txt");
        let rec = record(&manifest, std::slice::from_ref(&rel), &written);
        let out = uninstall(&manifest, &tmp.join("app-data"));
        let left = rel.join("versions/ferrocraft-1.8.9").exists();
        let _ = fs::remove_dir_all(&rel);
        res.unwrap();
        rec.unwrap();
        let (removed, skipped) = out.unwrap();
        assert!(skipped.is_empty() && removed.len() == written.len());
        assert!(!left);
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn fabric_install_keeps_engine_already_in_place() {
        let tmp = tmp("same");
        let home = tmp.join("home");
        fs::create_dir_all(home.join(".ferrocraft")).unwrap();
        let engine = home.join(".ferrocraft/ferrocraft-engine");
        fs::write(&engine, b"bin").unwrap();
        fs::write(tmp.join("mod.jar"), b"jar").unwrap();
        let mods = tmp.join("cluster/mods");
        install_fabric(
            std::slice::from_ref(&mods),
            &tmp.join("mod.jar"),
            &engine,
            &home,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(fs::read(&engine).unwrap(), b"bin");
        assert_eq!(fs::read(mods.join(FABRIC_JAR)).unwrap(), b"jar");
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
            record(
                &manifest,
                std::slice::from_ref(&mods),
                std::slice::from_ref(&ours),
            )
            .unwrap();
            let mut text = fs::read_to_string(&manifest).unwrap();
            text.push_str(&format!("{bad}\n"));
            fs::write(&manifest, text).unwrap();
            let (removed, skipped) = uninstall(&manifest, &tmp.join("app-data")).unwrap();
            assert_eq!(removed.len(), 1, "{bad}");
            assert!(skipped.len() == 1 && skipped[0].starts_with(&bad), "{bad}");
            assert!(!ours.exists(), "{bad}");
            assert!(victim.join("data").is_file(), "{bad}");
            assert!(other_mods.join(FABRIC_JAR).is_file(), "{bad}");
            assert!(
                nested.join("data").is_file() && nested_jar.is_file(),
                "{bad}"
            );
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
        record(
            &manifest,
            std::slice::from_ref(&mc),
            &[mc.join("ferrocraft")],
        )
        .unwrap();
        let (removed, skipped) = uninstall(&manifest, &tmp.join("app-data")).unwrap();
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
