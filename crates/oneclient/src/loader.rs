//! The mod loader of a OneClient cluster, and the jars of a Fabric cluster (used as they are: no twin lookup).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{file_name, manifest_path, read_manifest, sha1_hex, Error, Resolved};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loader {
    Forge,
    /// Fabric Loader (Legacy Fabric on 1.8.9).
    Fabric,
    /// Ornithe: Java 25 mods, not loadable by the Fabric host before slice B7.
    Ornithe,
    /// Vanilla, Quilt, NeoForge: no host for it.
    Other,
}

/// The loader in `<cluster>/.instance.json`. OneClient numbers its loaders: Vanilla 0, Forge 1, NeoForge 2, Quilt 3,
/// Fabric 4, Ornithe 5. A cluster with no `.instance.json` is Forge, the loader of the twin route.
/// `cluster` is the cluster directory or its `.oneclient-mods.json`.
pub fn loader(cluster: &Path) -> Result<Loader, Error> {
    let file = manifest_path(cluster).with_file_name(".instance.json");
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Loader::Forge),
        Err(e) => return Err(e.into()),
    };
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Format(format!("{}: {e}", file.display())))?;
    let raw = &v["mc_loader"];
    match raw.as_i64() {
        Some(1) => Ok(Loader::Forge),
        Some(4) => Ok(Loader::Fabric),
        Some(5) => Ok(Loader::Ornithe),
        Some(_) => Ok(Loader::Other),
        None => Err(Error::Format(format!(
            "{}: no numeric `mc_loader`",
            file.display()
        ))),
    }
}

/// Copy the jars of a Fabric cluster into `out_dir`, each checked against its manifest sha1.
/// The deny list does not apply here: it names Modrinth slugs, and slice B7 adds `pylon` for Ornithe.
pub fn fabric_jars(cluster: &Path, out_dir: &Path) -> Result<Resolved, Error> {
    let manifest = manifest_path(cluster);
    let dir = manifest.parent().unwrap_or(Path::new("."));
    let mut out = Resolved {
        jars: vec![],
        denied: vec![],
        no_forge: vec![],
    };
    std::fs::create_dir_all(out_dir)?;
    for (path, hash) in read_manifest(&manifest)? {
        let src = dir.join(&path);
        // A missing jar is an error, never a quiet "no cluster".
        let bytes =
            std::fs::read(&src).map_err(|e| Error::Format(format!("{}: {e}", src.display())))?;
        let actual = sha1_hex(&bytes);
        if actual != hash {
            return Err(Error::Sha1 {
                file: path,
                expected: hash,
                actual,
            });
        }
        let dest: PathBuf = out_dir.join(file_name(&path));
        if out.jars.contains(&dest) {
            return Err(Error::Format(format!(
                "two mods named {}",
                file_name(&path)
            )));
        }
        std::fs::write(&dest, bytes)?;
        out.jars.push(dest);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fc-loader-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn loader_comes_from_instance_json() {
        let d = temp("kind");
        assert_eq!(loader(&d).unwrap(), Loader::Forge, "no file");
        for (json, want) in [
            (r#"{"mc_loader":4}"#, Loader::Fabric),
            (r#"{"mc_loader":5}"#, Loader::Ornithe),
            (r#"{"mc_loader":1}"#, Loader::Forge),
            (r#"{"mc_loader":3}"#, Loader::Other),
            (r#"{"mc_loader":0}"#, Loader::Other),
        ] {
            std::fs::write(d.join(".instance.json"), json).unwrap();
            assert_eq!(loader(&d).unwrap(), want, "{json}");
        }
        std::fs::write(d.join(".instance.json"), "{}").unwrap();
        assert!(matches!(loader(&d), Err(Error::Format(_))));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn fabric_jars_are_copied_with_no_network_and_checked() {
        let d = temp("jars");
        std::fs::create_dir_all(d.join("mods")).unwrap();
        std::fs::write(d.join("mods/a.jar"), b"jar a").unwrap();
        let manifest =
            |hash: &str| format!(r#"{{"entries":[{{"path":"mods/a.jar","hash":"{hash}"}}]}}"#);
        let out = d.join("out");
        std::fs::write(
            d.join(".oneclient-mods.json"),
            manifest(&sha1_hex(b"jar a")),
        )
        .unwrap();
        let r = fabric_jars(&d, &out).unwrap();
        assert_eq!(r.jars, vec![out.join("a.jar")]);
        assert_eq!(std::fs::read(&r.jars[0]).unwrap(), b"jar a");

        std::fs::write(d.join(".oneclient-mods.json"), manifest(&"0".repeat(40))).unwrap();
        assert!(matches!(fabric_jars(&d, &out), Err(Error::Sha1 { .. })));
        std::fs::remove_file(d.join("mods/a.jar")).unwrap();
        assert!(matches!(fabric_jars(&d, &out), Err(Error::Format(_))));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn fabric_jars_with_one_base_name_fail() {
        let d = temp("dup");
        let mut entries = vec![];
        for dir in ["a", "b"] {
            std::fs::create_dir_all(d.join("mods").join(dir)).unwrap();
            std::fs::write(d.join("mods").join(dir).join("x.jar"), b"jar").unwrap();
            entries.push(format!(
                r#"{{"path":"mods/{dir}/x.jar","hash":"{}"}}"#,
                sha1_hex(b"jar")
            ));
        }
        std::fs::write(
            d.join(".oneclient-mods.json"),
            format!(r#"{{"entries":[{}]}}"#, entries.join(",")),
        )
        .unwrap();
        assert!(matches!(
            fabric_jars(&d, &d.join("out")),
            Err(Error::Format(_))
        ));
        std::fs::remove_dir_all(d).unwrap();
    }
}
