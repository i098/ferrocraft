use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use ferrocraft_modhost::{provision, provision_fabric, Config, ModHost, StagedStartError};
use ferrocraft_oneclient as oneclient;

use crate::log;

/// Runs on its own thread: Modrinth, jar downloads and the first-run Java 8 and Forge install take long.
pub fn spawn(game_dir: PathBuf) -> Boot {
    spawn_preparing(move || prepare(&game_dir))
}

fn spawn_preparing(
    prepare: impl FnOnce() -> std::io::Result<Option<Prepared>> + Send + 'static,
) -> Boot {
    let (tx, result) = mpsc::channel();
    let closed = Arc::new(Mutex::new(false));
    let gate = Arc::clone(&closed);
    std::thread::spawn(move || {
        let prepared = prepare();
        let closed = gate.lock().unwrap();
        if !*closed {
            let _ = tx.send(prepared.and_then(|p| p.map_or(Ok(None), Prepared::start)));
        }
    });
    Boot { result, closed }
}

pub struct Boot {
    result: Receiver<std::io::Result<Option<ModHost>>>,
    closed: Arc<Mutex<bool>>,
}

impl Boot {
    pub fn poll(&mut self) -> Option<std::io::Result<Option<ModHost>>> {
        match self.result.try_recv() {
            Ok(host) => Some(host),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Ok(None)),
        }
    }
}

impl Drop for Boot {
    fn drop(&mut self) {
        *self.closed.lock().unwrap_or_else(|e| e.into_inner()) = true;
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

    pub fn retry_after_archive_error(&mut self, now: Instant) {
        self.changed_at = Some(now);
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
    if !forge {
        return Ok(scan_legacy(dir));
    }
    let mut jars = Vec::new();
    scan_folder(dir, Path::new(""), &mut jars)?;
    scan_folder(dir, Path::new("1.8.9"), &mut jars)?;
    jars.sort();
    Ok(jars)
}

fn scan_legacy(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut jars: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jar"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((e.file_name().into(), m.modified().ok()?, m.len()))
        })
        .collect();
    jars.sort();
    jars
}

fn scan_folder(
    dir: &Path,
    folder: &Path,
    jars: &mut Vec<(PathBuf, SystemTime, u64)>,
) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir.join(folder)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e)
            if !folder.as_os_str().is_empty() && e.kind() == std::io::ErrorKind::NotADirectory =>
        {
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        match Path::new(&name).extension().and_then(|ext| ext.to_str()) {
            Some("jar" | "zip") => {}
            _ => continue,
        }
        let m = match std::fs::metadata(entry.path()) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if m.is_file() {
            jars.push((folder.join(name), m.modified()?, m.len()));
        }
    }
    Ok(())
}

struct Prepared {
    cfg: Config,
    plain: Option<(PathBuf, Vec<PathBuf>)>,
}

impl Prepared {
    fn start(self) -> std::io::Result<Option<ModHost>> {
        let result = match self.plain {
            Some((src, paths)) => ModHost::start_staged(self.cfg, &src, paths),
            None => ModHost::start(&self.cfg).map_err(StagedStartError::Start),
        };
        match result {
            Ok(host) => Ok(Some(host)),
            Err(StagedStartError::Archive(e)) => Err(e),
            Err(StagedStartError::Start(e)) => Ok(fail(&e)),
        }
    }
}

fn prepare(game_dir: &Path) -> std::io::Result<Option<Prepared>> {
    if let Some(cfg) = Config::from_env() {
        return Ok(Some(Prepared { cfg, plain: None }));
    }
    if let Some(cluster) = cluster(std::env::var_os("FERROCRAFT_CLUSTER"), game_dir) {
        return Ok(from_cluster(&cluster).map(|cfg| Prepared { cfg, plain: None }));
    }
    let src = game_dir.join("mods");
    let mods = scan(&src, true)?;
    if mods.is_empty() {
        return Ok(None);
    }
    let Some(cfg) = Config::provision().map_err(|e| fail::<Config>(&e)).ok() else {
        return Ok(None);
    };
    let paths = mods.into_iter().map(|(path, _, _)| path).collect();
    Ok(Some(Prepared {
        cfg,
        plain: Some((src, paths)),
    }))
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

    #[cfg(unix)]
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
        std::os::unix::fs::symlink("1.8.9", &versioned).unwrap();
        assert!(scan(&mods, true).is_err());
        assert_eq!(scan(&mods, false).unwrap().len(), 1);
        let t = Instant::now();
        assert!(!watch.poll(t));
        assert!(!watch.poll(t + Duration::from_secs(2)));
        std::fs::remove_file(&versioned).unwrap();
        std::fs::rename(parked, versioned).unwrap();
        assert!(!watch.poll(t + Duration::from_secs(4)));
        assert!(!watch.poll(t + Duration::from_secs(6)));
        watch.retry_after_archive_error(t + Duration::from_secs(6));
        assert!(watch.poll(t + Duration::from_secs(7)));
        assert!(!watch.poll(t + Duration::from_secs(9)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn legacy_watch_notices_edits_beside_a_looping_symlink() {
        let dir = std::env::temp_dir().join(format!("fc-legacy-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("good.jar");
        std::fs::write(&jar, "mod").unwrap();
        std::os::unix::fs::symlink("old.jar", dir.join("old.jar")).unwrap();
        let mut watch = Watch {
            seen: Some(scan(&dir, false).unwrap()),
            dir: dir.clone(),
            forge: false,
            last_scan: None,
            changed_at: None,
        };
        let t = Instant::now();
        assert!(!watch.poll(t));
        std::fs::write(jar, "updated mod").unwrap();
        assert!(!watch.poll(t + SCAN_EVERY));
        assert!(watch.poll(t + SCAN_EVERY + SETTLE));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn restored_archive_permissions_allow_a_pending_reload() {
        use std::fs::{self, File, Permissions};
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("fc-archive-permission-{}", std::process::id()));
        let dir = root.join("mods");
        fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("mod.jar");
        fs::write(&jar, "mod").unwrap();
        fs::set_permissions(&jar, Permissions::from_mode(0o000)).unwrap();
        if File::open(&jar).is_ok() {
            fs::remove_dir_all(root).unwrap();
            return;
        }
        let snapshot = scan(&dir, true).unwrap();
        let mut watch = Watch {
            seen: Some(snapshot.clone()),
            dir: dir.clone(),
            forge: true,
            last_scan: None,
            changed_at: None,
        };
        let prepared = || Prepared {
            cfg: Config {
                loader: ferrocraft_modhost::Loader::Forge,
                java: "/bin/true".into(),
                class_path: Vec::new(),
                game_dir: root.clone(),
                assets_dir: root.clone(),
            },
            plain: Some((dir.clone(), vec!["mod.jar".into()])),
        };
        let Err(error) = prepared().start() else {
            panic!("unreadable archive must produce a retryable failure");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        let t = Instant::now();
        watch.retry_after_archive_error(t);
        fs::set_permissions(&jar, Permissions::from_mode(0o600)).unwrap();
        assert_eq!(scan(&dir, true).unwrap(), snapshot);
        assert!(watch.poll(t + SETTLE));
        prepared().start().unwrap().unwrap().stop();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dangling_archives_do_not_hide_valid_mods() {
        let root = std::env::temp_dir().join(format!("fc-dangling-{}", std::process::id()));
        let versioned = root.join("1.8.9");
        std::fs::create_dir_all(&versioned).unwrap();
        std::fs::write(root.join("valid.jar"), "mod").unwrap();
        std::os::unix::fs::symlink("missing", root.join("dangling.jar")).unwrap();
        std::os::unix::fs::symlink("missing", versioned.join("dangling.zip")).unwrap();
        let mods = scan(&root, true).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].0, Path::new("valid.jar"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn optional_version_path_can_be_a_file() {
        let root = std::env::temp_dir().join(format!("fc-version-file-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("1.8.9"), "file").unwrap();
        std::fs::write(root.join("valid.jar"), "mod").unwrap();
        let mods = scan(&root, true).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].0, Path::new("valid.jar"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn closing_boot_does_not_wait_for_preparation() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let boot = spawn_preparing(move || {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
            Ok(None)
        });
        started_rx.recv().unwrap();
        let (closed_tx, closed_rx) = mpsc::channel();
        let closer = std::thread::spawn(move || {
            drop(boot);
            closed_tx.send(()).unwrap();
        });
        let closed = closed_rx.recv_timeout(Duration::from_secs(5));
        release_tx.send(()).unwrap();
        closer.join().unwrap();
        closed.expect("closing the engine must not wait for provisioning");
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
        let mut host = prepare(&cluster)
            .unwrap()
            .map_or(Ok(None), Prepared::start)
            .unwrap()
            .expect("the cluster started no host");
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
