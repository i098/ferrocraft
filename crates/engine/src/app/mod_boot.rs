//! Starts the mod host with no `FERROCRAFT_MODHOST_*` variables: the OneClient cluster gives the mods and
//! `Config::provision` gives Java 8, Forge and the host jar.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use ferrocraft_modhost::{provision, Config, ModHost};
use ferrocraft_oneclient as oneclient;

use crate::log;

/// The variables win when set. Otherwise the cluster is `FERROCRAFT_CLUSTER` (the Fabric shim sets it from
/// `fabric.modsFolder`) or `--gameDir`. A folder with no `.oneclient-mods.json` is not a cluster: no mods then.
/// Runs on its own thread: Modrinth, jar downloads and the first-run Java 8 and Forge install take long.
pub fn spawn(game_dir: PathBuf) -> Receiver<Option<ModHost>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(start(&game_dir));
    });
    rx
}

/// `Some(Some(host))` once the host is up; `Some(None)` when it will never come (no mods, an error, a dead thread).
pub fn poll<T>(rx: &Receiver<Option<T>>) -> Option<Option<T>> {
    match rx.try_recv() {
        Ok(host) => Some(host),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(None),
    }
}

fn start(game_dir: &Path) -> Option<ModHost> {
    let cfg = match Config::from_env() {
        Some(cfg) => cfg,
        None => from_cluster(&cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir))?,
    };
    match ModHost::start(&cfg) {
        Ok(host) => Some(host),
        Err(e) => {
            log::line(&format!("mod host did not start: {e}"));
            None
        }
    }
}

fn cluster(env: Option<std::ffi::OsString>, game_dir: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty())
        .map_or_else(|| game_dir.to_path_buf(), PathBuf::from)
}

fn from_cluster(cluster: &Path) -> Option<Config> {
    let mods = match provision::main_dir() {
        Ok(main) => main.join("mods"),
        Err(e) => return fail(&e),
    };
    match oneclient::resolve(cluster, &mods) {
        Ok(r) => {
            prune(&mods, &r.jars);
            log::line(&format!(
                "cluster {}: {} mods, {} denied {:?}, {} with no Forge 1.8.9 build {:?}",
                cluster.display(),
                r.jars.len(),
                r.denied.len(),
                r.denied,
                r.no_forge.len(),
                r.no_forge
            ));
        }
        Err(oneclient::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return fail(&e),
    }
    Config::provision().map_err(|e| fail::<Config>(&e)).ok()
}

fn prune(mods: &Path, keep: &[PathBuf]) {
    let Ok(dir) = std::fs::read_dir(mods) else {
        return;
    };
    for p in dir.flatten().map(|e| e.path()) {
        if p.extension().is_some_and(|x| x == "jar") && !keep.contains(&p) {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn fail<T>(e: &dyn std::fmt::Display) -> Option<T> {
    log::line(&format!("mod host did not start: {e}"));
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_is_env_then_game_dir() {
        let gd = Path::new("/g");
        assert_eq!(cluster(Some("/c".into()), gd), Path::new("/c"));
        assert_eq!(cluster(Some("".into()), gd), gd);
        assert_eq!(cluster(None, gd), gd);
    }

    #[test]
    fn poll_reports_host_error_and_dead_thread() {
        let (tx, rx) = channel();
        assert!(poll::<u8>(&rx).is_none());
        tx.send(Some(7u8)).unwrap();
        assert_eq!(poll(&rx), Some(Some(7)));
        tx.send(None).unwrap();
        assert_eq!(poll::<u8>(&rx), Some(None));
        drop(tx);
        assert_eq!(poll::<u8>(&rx), Some(None));
    }

    #[test]
    fn prune_keeps_only_listed_jars() {
        let d = std::env::temp_dir().join(format!("fc-prune-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a.jar"), d.join("b.jar"));
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();
        prune(&d, &[a.clone()]);
        assert!(a.exists() && !b.exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn folder_without_manifest_starts_no_mods() {
        let d = std::env::temp_dir().join(format!("fc-modboot-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(from_cluster(&d).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
