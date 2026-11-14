//! Starts the mod host with no `FERROCRAFT_MODHOST_*` variables: the OneClient cluster gives the mods and
//! `Config::provision` gives Java 8, Forge and the host jar.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use ferrocraft_modhost::{provision, provision_fabric, Config, ModHost};
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
    let loader = match oneclient::loader(cluster) {
        Ok(l) => l,
        Err(e) => return fail(&e),
    };
    // Ornithe keeps the Forge twin route until the Fabric host loads Ornithe jars (slice B7).
    let fabric = loader == oneclient::Loader::Fabric;
    let main = if fabric {
        provision_fabric::main_dir().map_err(|e| e.to_string())
    } else {
        provision::main_dir().map_err(|e| e.to_string())
    };
    let mods = match main {
        Ok(main) => main.join("mods"),
        Err(e) => return fail(&e),
    };
    let resolved = if fabric {
        oneclient::fabric_jars(cluster, &mods)
    } else {
        oneclient::resolve(cluster, &mods)
    };
    match resolved {
        Ok(r) => {
            prune(&mods, &r.jars);
            log::line(&format!(
                "cluster {} ({loader:?}): {} mods, {} denied {:?}, {} with no Forge 1.8.9 build {:?}",
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
    if fabric {
        Config::provision_fabric()
            .map_err(|e| fail::<Config>(&e))
            .ok()
    } else {
        Config::provision().map_err(|e| fail::<Config>(&e)).ok()
    }
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
        prune(&d, std::slice::from_ref(&a));
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

    /// Real run, by hand (network, JDK-built host jars, a clean `HOME`): `FERRO_LIVE_CLUSTER=<cluster dir> HOME=$(mktemp -d)
    /// cargo test -p ferrocraft-engine live_cluster -- --ignored --nocapture`. A JVM that stops before the hello fails it.
    #[test]
    #[ignore]
    fn live_cluster_starts_its_host_and_gets_the_hello() {
        let cluster =
            PathBuf::from(std::env::var_os("FERRO_LIVE_CLUSTER").expect("FERRO_LIVE_CLUSTER"));
        let mut host = start(&cluster).expect("the cluster started no host");
        let end = std::time::Instant::now() + std::time::Duration::from_secs(300);
        while host.client_brand().is_none() {
            assert!(
                std::time::Instant::now() < end,
                "no hello: the JVM stopped or hung"
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        println!("hello: client brand {:?}", host.client_brand());
        host.stop();
    }
}
