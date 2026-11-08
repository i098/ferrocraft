//! Find the launchers on this machine and the dirs ferrocraft installs into.

use std::fs;
use std::path::{Path, PathBuf};

/// Home and app-data dirs of the current user.
pub struct Env {
    pub home: PathBuf,
    pub data: PathBuf,
}

impl Env {
    pub fn current() -> Env {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let home = var("HOME")
            .or_else(|| var("USERPROFILE"))
            .unwrap_or_default();
        let data = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(windows) {
            var("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"))
        } else {
            var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"))
        };
        Env { home, data }
    }

    /// Official launcher `.minecraft` dir.
    pub fn minecraft_dir(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.data.join("minecraft")
        } else if cfg!(windows) {
            self.data.join(".minecraft")
        } else {
            self.home.join(".minecraft")
        }
    }
}

/// A launcher whose instances are dirs under `<root>/<sub>/`, each with json metadata.
pub struct Layout {
    pub roots: Vec<PathBuf>,
    pub sub: &'static str,
    /// Where the mods dir sits inside an instance; the first existing one wins, else the first.
    pub mods: &'static [&'static str],
}

/// OneClient data dir names are not documented; `--oneclient-dir` overrides them.
pub fn oneclient(env: &Env, root: Option<PathBuf>) -> Layout {
    let names = ["OneClient", "oneclient", "OneLauncher", "onelauncher"];
    Layout {
        roots: root.map_or_else(
            || names.iter().map(|n| env.data.join(n)).collect(),
            |r| vec![r],
        ),
        sub: "clusters",
        mods: &["mods"],
    }
}

pub fn prism(env: &Env, root: Option<PathBuf>) -> Layout {
    let names = ["PrismLauncher", "PolyMC", "multimc", "MultiMC"];
    let flatpak = env
        .home
        .join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher");
    Layout {
        roots: root.map_or_else(
            || {
                names
                    .iter()
                    .map(|n| env.data.join(n))
                    .chain([flatpak])
                    .collect()
            },
            |r| vec![r],
        ),
        sub: "instances",
        mods: &[".minecraft/mods", "minecraft/mods"],
    }
}

/// An instance is a 1.8.9 Fabric one when its name or json files mention both.
fn is_fabric_189(dir: &Path) -> bool {
    let mut text = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "json") {
            text.push_str(&fs::read_to_string(&p).unwrap_or_default());
        }
    }
    let text = text.to_lowercase();
    text.contains("1.8.9") && text.contains("fabric")
}

/// Mods dir of every 1.8.9 Fabric instance of the launcher.
pub fn fabric_mods_dirs(layout: &Layout) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in &layout.roots {
        for entry in fs::read_dir(root.join(layout.sub))
            .into_iter()
            .flatten()
            .flatten()
        {
            let inst = entry.path();
            if inst.is_dir() && is_fabric_189(&inst) {
                let mods = layout
                    .mods
                    .iter()
                    .map(|m| inst.join(m))
                    .find(|p| p.is_dir())
                    .unwrap_or_else(|| inst.join(layout.mods[0]));
                out.push(mods);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fc-detect-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn picks_only_189_fabric_instances() {
        let root = tmp("pick");
        let c = root.join("clusters");
        for (name, json) in [
            ("a", r#"{"mc":"1.8.9","loader":"Fabric"}"#),
            ("b", r#"{"mc":"1.8.9","loader":"forge"}"#),
            ("c", r#"{"mc":"1.21","loader":"fabric"}"#),
            ("1.8.9-fabric", "{}"),
        ] {
            fs::create_dir_all(c.join(name)).unwrap();
            fs::write(c.join(name).join("cluster.json"), json).unwrap();
        }
        let env = Env {
            home: root.clone(),
            data: root.clone(),
        };
        let mut got = fabric_mods_dirs(&oneclient(&env, Some(root.clone())));
        got.sort();
        assert_eq!(got, [c.join("1.8.9-fabric/mods"), c.join("a/mods")]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn prism_uses_existing_mods_dir() {
        let root = tmp("prism");
        let inst = root.join("instances/x");
        fs::create_dir_all(inst.join("minecraft/mods")).unwrap();
        fs::write(
            inst.join("mmc-pack.json"),
            r#"{"uid":"net.fabricmc.fabric-loader","v":"1.8.9"}"#,
        )
        .unwrap();
        let env = Env {
            home: root.clone(),
            data: root.clone(),
        };
        assert_eq!(
            fabric_mods_dirs(&prism(&env, Some(root.clone()))),
            [inst.join("minecraft/mods")]
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
