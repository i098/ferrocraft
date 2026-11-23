//! Starts the mod host with no `FERROCRAFT_MODHOST_*` variables: the OneClient cluster gives the mods and
//! `Config::provision` gives Java 8, Forge and the host jar.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

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

/// Hot reload: sees jars come, go or change in the mods folder and says when the folder has been still for `SETTLE`.
pub struct Watch {
    dir: PathBuf,
    seen: Vec<(OsString, SystemTime, u64)>,
    last_scan: Option<Instant>,
    changed_at: Option<Instant>,
}

const SCAN_EVERY: Duration = Duration::from_millis(500);
/// A copy of a jar takes time: reload only after the folder stopped changing.
const SETTLE: Duration = Duration::from_secs(1);

impl Watch {
    /// The folder is the one `start` reads the mods from: `FERROCRAFT_MODHOST_GAMEDIR` or the cluster, then `mods/`.
    pub fn new(game_dir: &Path) -> Self {
        let root = std::env::var_os("FERROCRAFT_MODHOST_GAMEDIR").map_or_else(
            || cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir),
            PathBuf::from,
        );
        let dir = root.join("mods");
        Self {
            seen: scan(&dir),
            dir,
            last_scan: None,
            changed_at: None,
        }
    }

    /// True once per change, after the change has settled.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self
            .last_scan
            .is_some_and(|t| now.duration_since(t) < SCAN_EVERY)
        {
            return false;
        }
        self.last_scan = Some(now);
        let jars = scan(&self.dir);
        if jars != self.seen {
            self.seen = jars;
            self.changed_at = Some(now);
        }
        match self.changed_at {
            Some(t) if now.duration_since(t) >= SETTLE => {
                self.changed_at = None;
                true
            }
            _ => false,
        }
    }
}

fn scan(dir: &Path) -> Vec<(OsString, SystemTime, u64)> {
    let mut jars: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jar"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((e.file_name(), m.modified().ok()?, m.len()))
        })
        .collect();
    jars.sort();
    jars
}

fn start(game_dir: &Path) -> Option<ModHost> {
    if let Some(cfg) = Config::from_env() {
        return started(ModHost::start(&cfg));
    }
    from_cluster(&cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir))
}

fn started(result: std::io::Result<ModHost>) -> Option<ModHost> {
    match result {
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

fn from_cluster(cluster: &Path) -> Option<ModHost> {
    let loader = match oneclient::loader(cluster) {
        Ok(l) => l,
        Err(e) => return fail(&e),
    };
    let Some(host) = host_for(loader) else {
        log::line(&format!(
            "cluster {}: loader {loader:?} has no mod host",
            cluster.display()
        ));
        return None;
    };
    if host == Host::Ornithe {
        return start_ornithe(cluster, loader);
    }
    let main = match host {
        Host::Forge => provision::main_dir().map_err(|e| e.to_string()),
        Host::Fabric => provision_fabric::main_dir().map_err(|e| e.to_string()),
        Host::Ornithe => unreachable!(),
    };
    let main = match main {
        Ok(main) => main,
        Err(e) => return fail(&e),
    };
    stage(cluster, loader, &main.join("mods"))?;
    let cfg = match host {
        Host::Forge => Config::provision().map_err(|e| e.to_string()),
        Host::Fabric => Config::provision_fabric().map_err(|e| e.to_string()),
        Host::Ornithe => unreachable!(),
    };
    match cfg {
        Ok(cfg) => started(ModHost::start(&cfg)),
        Err(e) => fail(&e),
    }
}

fn start_ornithe(cluster: &Path, loader: oneclient::Loader) -> Option<ModHost> {
    let workspace = match provision_fabric::ornithe_workspace() {
        Ok(dir) => dir,
        Err(e) => return fail(&e),
    };
    stage(cluster, loader, workspace.path())?;
    match Config::provision_ornithe(workspace.path()) {
        Ok(cfg) => started(ModHost::start_staged(&cfg, workspace)),
        Err(e) => fail(&e),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Forge,
    Fabric,
    /// The Fabric host with the Ornithe profile: Ornithe mods are Fabric-family mods.
    Ornithe,
}

/// The host that loads the loader, `None` when no host does.
fn host_for(loader: oneclient::Loader) -> Option<Host> {
    match loader {
        oneclient::Loader::Forge => Some(Host::Forge),
        oneclient::Loader::Fabric => Some(Host::Fabric),
        oneclient::Loader::Ornithe => Some(Host::Ornithe),
        oneclient::Loader::Other => None,
    }
}

/// Puts the cluster's jars in the host's mods folder. `None` when there is no cluster or it failed.
fn stage(cluster: &Path, loader: oneclient::Loader, mods: &Path) -> Option<()> {
    let resolved = if loader == oneclient::Loader::Forge {
        oneclient::resolve(cluster, mods)
    } else {
        oneclient::fabric_jars(cluster, mods)
    };
    let r = match resolved {
        Ok(r) => r,
        Err(oneclient::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return fail(&e),
    };
    prune(mods, &r.jars);
    let twin = if r.no_forge.is_empty() {
        String::new()
    } else {
        format!(
            ", {} with no Forge 1.8.9 build {:?}",
            r.no_forge.len(),
            r.no_forge
        )
    };
    log::line(&format!(
        "cluster {} ({loader:?}): {} mods, {} denied {:?}{twin}",
        cluster.display(),
        r.jars.len(),
        r.denied.len(),
        r.denied
    ));
    Some(())
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
    fn watch_fires_once_after_a_jar_settles() {
        let d = std::env::temp_dir().join(format!("fc-watch-{}", std::process::id()));
        std::fs::create_dir_all(d.join("mods")).unwrap();
        let mut w = Watch::new(&d);
        let t0 = Instant::now();
        assert!(!w.poll(t0));
        std::fs::write(d.join("mods/a.jar"), "x").unwrap();
        assert!(!w.poll(t0 + Duration::from_secs(1)));
        assert!(w.poll(t0 + Duration::from_secs(3)));
        assert!(!w.poll(t0 + Duration::from_secs(5)));
        std::fs::remove_dir_all(&d).unwrap();
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
    fn each_loader_gets_its_host_or_none() {
        use oneclient::Loader::*;
        assert_eq!(host_for(Forge), Some(Host::Forge));
        assert_eq!(host_for(Fabric), Some(Host::Fabric));
        assert_eq!(host_for(Ornithe), Some(Host::Ornithe));
        assert_eq!(host_for(Other), None);
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
