use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use ferrocraft_modhost::{provision, provision_fabric, Config, ModHost};
use ferrocraft_oneclient as oneclient;

use crate::log;

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
    forge: bool,
    seen: Option<Vec<(PathBuf, SystemTime, u64)>>,
    last_scan: Option<Instant>,
    changed_at: Option<Instant>,
}

const SCAN_EVERY: Duration = Duration::from_millis(500);
/// A copy of a jar takes time: reload only after the folder stopped changing.
const SETTLE: Duration = Duration::from_secs(1);

impl Watch {
    pub fn new(game_dir: &Path) -> Self {
        let (root, forge) = if let Some(cfg) = Config::from_env() {
            (cfg.game_dir, false)
        } else if let Some(cluster) = cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir) {
            (cluster, false)
        } else {
            (game_dir.to_path_buf(), true)
        };
        let dir = root.join("mods");
        Self {
            seen: scan(&dir, forge)
                .map_err(|e| log::line(&e.to_string()))
                .ok(),
            forge,
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
        let Ok(jars) = scan(&self.dir, self.forge) else {
            self.changed_at = self.changed_at.map(|_| now);
            return false;
        };
        if self.seen.as_ref() != Some(&jars) {
            self.seen = Some(jars);
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

fn scan(dir: &Path, forge: bool) -> std::io::Result<Vec<(PathBuf, SystemTime, u64)>> {
    let mut jars = Vec::new();
    scan_folder(dir, Path::new(""), forge, &mut jars)?;
    if forge {
        scan_folder(dir, Path::new("1.8.9"), true, &mut jars)?;
    }
    jars.sort();
    Ok(jars)
}

fn scan_folder(
    dir: &Path,
    folder: &Path,
    forge: bool,
    jars: &mut Vec<(PathBuf, SystemTime, u64)>,
) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir.join(folder)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        match Path::new(&name).extension().and_then(|ext| ext.to_str()) {
            Some("jar") => {}
            Some("zip") if forge => {}
            _ => continue,
        }
        let m = std::fs::metadata(entry.path())?;
        if m.is_file() {
            jars.push((folder.join(name), m.modified()?, m.len()));
        }
    }
    Ok(())
}

fn start(game_dir: &Path) -> Option<ModHost> {
    let result = if let Some(cfg) = Config::from_env() {
        ModHost::start(&cfg)
    } else if let Some(cluster) = cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir) {
        ModHost::start(&from_cluster(&cluster)?)
    } else {
        let src = game_dir.join("mods");
        let mods = scan(&src, true).map_err(|e| fail::<ModHost>(&e)).ok()?;
        if mods.is_empty() {
            return None;
        }
        let cfg = Config::provision().map_err(|e| fail::<Config>(&e)).ok()?;
        ModHost::start_staged(cfg, &src, mods.into_iter().map(|(path, _, _)| path))
    };
    result.map_err(|e| fail::<ModHost>(&e)).ok()
}

fn cluster(env: Option<OsString>, game_dir: &Path) -> Option<PathBuf> {
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            game_dir
                .join(".oneclient-mods.json")
                .is_file()
                .then(|| game_dir.to_path_buf())
        })
}

fn from_cluster(cluster: &Path) -> Option<Config> {
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
    stage(cluster, loader, host)?;
    let cfg = match host {
        Host::Forge => Config::provision().map_err(|e| e.to_string()),
        Host::Fabric => Config::provision_fabric().map_err(|e| e.to_string()),
        Host::Ornithe => Config::provision_ornithe().map_err(|e| e.to_string()),
    };
    cfg.map_err(|e| fail::<Config>(&e)).ok()
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
fn stage(cluster: &Path, loader: oneclient::Loader, host: Host) -> Option<()> {
    let main = match host {
        Host::Forge => provision::main_dir().map_err(|e| e.to_string()),
        Host::Fabric => provision_fabric::main_dir().map_err(|e| e.to_string()),
        Host::Ornithe => provision_fabric::ornithe_dir().map_err(|e| e.to_string()),
    };
    let mods = match main {
        Ok(main) => main.join("mods"),
        Err(e) => return fail(&e),
    };
    let resolved = if host == Host::Forge {
        oneclient::resolve(cluster, &mods)
    } else {
        oneclient::fabric_jars(cluster, &mods)
    };
    let r = match resolved {
        Ok(r) => r,
        Err(oneclient::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return fail(&e),
    };
    prune(&mods, &r.jars);
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
    fn explicit_cluster_wins() {
        let gd = Path::new("/g");
        assert_eq!(cluster(Some("/c".into()), gd), Some(PathBuf::from("/c")));
    }

    #[test]
    fn watch_fires_once_after_a_versioned_zip_settles() {
        let d = std::env::temp_dir().join(format!("fc-watch-{}", std::process::id()));
        std::fs::create_dir_all(d.join("mods/1.8.9")).unwrap();
        let mut w = Watch::new(&d);
        let t0 = Instant::now();
        assert!(!w.poll(t0));
        std::fs::write(d.join("mods/1.8.9/a.zip"), "x").unwrap();
        assert!(!w.poll(t0 + Duration::from_secs(1)));
        assert!(w.poll(t0 + Duration::from_secs(3)));
        assert!(!w.poll(t0 + Duration::from_secs(5)));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn failed_scan_does_not_replace_the_watch_snapshot() {
        let root = std::env::temp_dir().join(format!("fc-plain-{}", std::process::id()));
        let mods = root.join("mods");
        let versioned = mods.join("1.8.9");
        let parked = root.join("parked");
        std::fs::create_dir_all(&versioned).unwrap();
        std::fs::write(mods.join("top.jar"), "top").unwrap();
        std::fs::write(versioned.join("nested.zip"), "nested").unwrap();
        let mut watch = Watch {
            seen: Some(scan(&mods, true).unwrap()),
            dir: mods.clone(),
            forge: true,
            last_scan: None,
            changed_at: None,
        };
        std::fs::rename(&versioned, &parked).unwrap();
        std::fs::write(&versioned, "not a directory").unwrap();
        assert!(scan(&mods, true).is_err());
        assert_eq!(scan(&mods, false).unwrap().len(), 1);
        let t = Instant::now();
        assert!(!watch.poll(t));
        assert!(!watch.poll(t + Duration::from_secs(2)));
        std::fs::remove_file(&versioned).unwrap();
        std::fs::rename(parked, versioned).unwrap();
        assert!(!watch.poll(t + Duration::from_secs(4)));
        assert!(!watch.poll(t + Duration::from_secs(6)));
        std::fs::remove_dir_all(root).unwrap();
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
    fn game_dir_is_a_cluster_only_with_a_manifest() {
        let d = std::env::temp_dir().join(format!("fc-modboot-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(cluster(None, &d).is_none());
        assert!(cluster(Some("".into()), &d).is_none());
        std::fs::write(d.join(".oneclient-mods.json"), "{}").unwrap();
        assert_eq!(cluster(None, &d).as_deref(), Some(d.as_path()));
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
