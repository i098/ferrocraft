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

/// OneClient data dir names are not documented; `--oneclient-dir` overrides them.
pub fn oneclient_roots(env: &Env, root: Option<PathBuf>) -> Vec<PathBuf> {
    let names = ["OneClient", "oneclient", "OneLauncher", "onelauncher"];
    root.map_or_else(
        || names.iter().map(|n| env.data.join(n)).collect(),
        |r| vec![r],
    )
}

/// True when `text` has `1.8.9` as a whole version token (`11.8.9`, `1.8.90` do not count).
fn has_189(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_digit() && c != '.')
        .any(|t| t.trim_matches('.') == "1.8.9")
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
            text.push(' ');
            text.push_str(&fs::read_to_string(&p).unwrap_or_default());
        }
    }
    has_189(&text) && text.to_lowercase().contains("fabric")
}

/// Mods dir of every 1.8.9 Fabric cluster under the OneClient data dirs `roots`.
pub fn fabric_mods_dirs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        for entry in fs::read_dir(root.join("clusters"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let inst = entry.path();
            if inst.is_dir() && is_fabric_189(&inst) {
                out.push(inst.join("mods"));
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
            ("d", r#"{"mc":"1.21","loader":"fabric","mod":"11.8.9"}"#),
            ("e", r#"{"mc":"1.21","loader":"fabric","mod":"0.1.8.9"}"#),
            ("f", r#"{"mc":"1.21","loader":"fabric","mod":"1.8.90"}"#),
            ("1.8.9-fabric", "{}"),
        ] {
            fs::create_dir_all(c.join(name)).unwrap();
            fs::write(c.join(name).join("cluster.json"), json).unwrap();
        }
        let mut got = fabric_mods_dirs(&oneclient_roots(
            &Env {
                home: root.clone(),
                data: root.clone(),
            },
            Some(root.clone()),
        ));
        got.sort();
        assert_eq!(got, [c.join("1.8.9-fabric/mods"), c.join("a/mods")]);
        fs::remove_dir_all(&root).unwrap();
    }
}
