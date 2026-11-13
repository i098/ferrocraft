//! Starts the mod host with no `FERROCRAFT_MODHOST_*` variables: the OneClient cluster gives the mods and
//! `Config::provision` gives Java 8, Forge and the host jar.

use std::path::{Path, PathBuf};

use ferrocraft_modhost::{provision, Config, ModHost};
use ferrocraft_oneclient as oneclient;

use crate::log;

/// The variables win when set. Otherwise the cluster is `FERROCRAFT_CLUSTER` (the Fabric shim sets it from
/// `fabric.modsFolder`) or `--gameDir`. A folder with no `.oneclient-mods.json` is not a cluster: no mods then.
pub fn start(game_dir: &Path) -> Option<ModHost> {
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
        Ok(r) => log::line(&format!(
            "cluster {}: {} mods, {} denied {:?}, {} with no Forge 1.8.9 build {:?}",
            cluster.display(),
            r.jars.len(),
            r.denied.len(),
            r.denied,
            r.no_forge.len(),
            r.no_forge
        )),
        Err(oneclient::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return fail(&e),
    }
    Config::provision().map_err(|e| fail::<Config>(&e)).ok()
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
    fn folder_without_manifest_starts_no_mods() {
        let d = std::env::temp_dir().join(format!("fc-modboot-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(from_cluster(&d).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
