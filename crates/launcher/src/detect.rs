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
        #[cfg(windows)]
        let data = dirs::data_dir().unwrap_or_default();
        #[cfg(not(windows))]
        let data = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            var("XDG_DATA_HOME")
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"))
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

/// True when `text` has `1.8.9` as a whole version token (`11.8.9`, `1.8.90` do not count).
fn has_189(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_digit() && c != '.')
        .any(|t| t.trim_matches('.') == "1.8.9")
}

/// Select 1.8.9 Fabric or Ornithe instances.
fn is_fabric_189(dir: &Path) -> bool {
    if let Ok(bytes) = fs::read(dir.join(".instance.json")) {
        let Ok(instance) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return false;
        };
        if instance.get("mc_loader").is_some() {
            let loader = &instance["mc_loader"];
            if loader != 4 {
                return instance["mc_version"] == "1.8.9" && loader == "Ornithe";
            }
            if instance["mc_version"] != "1.8.9" {
                return false;
            }
        }
    }
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

/// Find compatible clusters under the specified OneClient data root.
pub fn fabric_mods_dirs(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join("clusters"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir() && is_fabric_189(dir))
        .map(|dir| dir.join("mods"))
        .collect()
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
        let mut got = fabric_mods_dirs(&root);
        got.sort();
        assert_eq!(got, [c.join("1.8.9-fabric/mods"), c.join("a/mods")]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn explicit_root_selects_only_supported_loader_metadata() {
        let root = tmp("ornithe");
        let c = root.join("OneClient/clusters");
        for (name, version, loader) in [
            ("ornithe", "1.8.9", serde_json::json!("Ornithe")),
            ("ornithe-numeric", "1.8.9", serde_json::json!(5)),
            ("fabric-numeric", "1.8.9", serde_json::json!(4)),
            ("pvp", "1.8.9", serde_json::json!(4)),
            ("fabric-string", "1.8.9", serde_json::json!("Fabric")),
            ("forge", "1.8.9", serde_json::json!(1)),
            ("other-version", "1.21", serde_json::json!("Ornithe")),
        ] {
            fs::create_dir_all(c.join(name)).unwrap();
            fs::write(
                c.join(name).join(".instance.json"),
                serde_json::json!({
                    "mc_version": version,
                    "mc_loader": loader,
                    "description": if name == "fabric-numeric" { "Fabric" } else { "PvP" }
                })
                .to_string(),
            )
            .unwrap();
        }
        let mut found = fabric_mods_dirs(&root.join("OneClient"));
        found.sort();
        assert_eq!(
            found,
            [
                c.join("fabric-numeric/mods"),
                c.join("ornithe/mods")
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }
}
