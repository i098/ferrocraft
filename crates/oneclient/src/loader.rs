//! The mod loader of a OneClient cluster, and the jars of a Fabric cluster (used as they are: no twin lookup).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{file_name, manifest_path, read_manifest, sha1_hex, Error, Resolved};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loader {
    Forge,
    /// Fabric Loader (Legacy Fabric on 1.8.9).
    Fabric,
    /// Ornithe: Fabric-family mods on Java 25, loaded by the Fabric host with the Ornithe profile.
    Ornithe,
    /// Vanilla, Quilt, NeoForge: no host for it.
    Other,
}

/// The loader in `<cluster>/.instance.json`. Older clusters use numeric loaders.
/// A cluster with no `.instance.json` is Forge, the loader of the twin route.
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
    match (raw.as_i64(), raw.as_str()) {
        (Some(1), _) => Ok(Loader::Forge),
        (Some(4), _) => Ok(Loader::Fabric),
        (Some(5), _) | (_, Some("Ornithe")) => Ok(Loader::Ornithe),
        (Some(_), _) => Ok(Loader::Other),
        _ => Err(Error::Format(format!(
            "{}: invalid `mc_loader`",
            file.display()
        ))),
    }
}

pub fn is_bootstrap(jar: impl std::io::Read + std::io::Seek) -> Result<bool, Error> {
    use zip::result::ZipError;

    let invalid = |e: ZipError| Error::Format(e.to_string());
    let mut zip = zip::ZipArchive::new(jar).map_err(invalid)?;
    let entry = match zip.by_name("fabric.mod.json") {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(false),
        Err(e) => return Err(invalid(e)),
    };
    if entry.size() > 1024 * 1024 {
        return Err(Error::Format("fabric.mod.json exceeds 1 MiB".into()));
    }
    let metadata: Value = serde_json::from_reader(entry)
        .map_err(|e| Error::Format(format!("invalid fabric.mod.json: {e}")))?;
    metadata["id"]
        .as_str()
        .map(|id| id == "ferrocraft")
        .ok_or_else(|| Error::Format("fabric.mod.json has no mod ID".into()))
}

/// Copy the jars of a Fabric or Ornithe cluster into `out_dir`, each checked against its manifest sha1.
/// The deny list does not apply here: it names Modrinth slugs. The Fabric host's provisioning replaces `pylon` with a stand-in.
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
        if is_bootstrap(std::io::Cursor::new(&bytes))? {
            continue;
        }
        let dest: PathBuf = out_dir.join(file_name(&path));
        if out.jars.contains(&dest) {
            return Err(Error::Format(format!(
                "two mods named {}",
                file_name(&path)
            )));
        }
        #[cfg(windows)]
        let mut staged = crate::private_file::create(out_dir)?;
        #[cfg(not(windows))]
        let mut staged = tempfile::NamedTempFile::new_in(out_dir)?;
        staged.write_all(&bytes)?;
        staged.persist(&dest).map_err(|e| e.error)?;
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
            (r#"{"mc_loader":"Ornithe"}"#, Loader::Ornithe),
            (r#"{"mc_loader":1}"#, Loader::Forge),
            (r#"{"mc_loader":3}"#, Loader::Other),
            (r#"{"mc_loader":0}"#, Loader::Other),
        ] {
            std::fs::write(d.join(".instance.json"), json).unwrap();
            assert_eq!(loader(&d).unwrap(), want, "{json}");
        }
        for raw in ["{}", r#"{"mc_loader":"Forge"}"#, r#"{"mc_loader":"minecraft"}"#] {
            std::fs::write(d.join(".instance.json"), raw).unwrap();
            assert!(matches!(loader(&d), Err(Error::Format(_))));
        }
        std::fs::remove_dir_all(d).unwrap();
    }

    fn mod_jar(id: &str) -> Vec<u8> {
        let mut jar = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        jar.start_file(
            "fabric.mod.json",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        write!(jar, r#"{{"id":"{id}"}}"#).unwrap();
        jar.finish().unwrap().into_inner()
    }

    #[test]
    fn fabric_jars_are_copied_with_no_network_and_checked() {
        let d = temp("jars");
        std::fs::create_dir_all(d.join("mods")).unwrap();

        let ordinary = d.join("mods/ferrocraft-fabric.jar");
        let ordinary_bytes = mod_jar("ordinary");
        std::fs::write(&ordinary, &ordinary_bytes).unwrap();
        let bootstrap = mod_jar("ferrocraft");
        std::fs::write(d.join("mods/renamed.jar"), &bootstrap).unwrap();
        let manifest = |hash: &str| {
            format!(
                r#"{{"entries":[{{"path":"mods/ferrocraft-fabric.jar","hash":"{hash}"}},{{"path":"mods/renamed.jar","hash":"{}"}}]}}"#,
                sha1_hex(&bootstrap)
            )
        };
        let out = d.join("out");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir(&out).unwrap();
            let staged = out.join("ferrocraft-fabric.jar");
            std::fs::write(&staged, b"old copy").unwrap();
            std::fs::set_permissions(staged, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        std::fs::write(
            d.join(".oneclient-mods.json"),
            manifest(&sha1_hex(&ordinary_bytes)),
        )
        .unwrap();
        let r = fabric_jars(&d, &out).unwrap();
        assert_eq!(r.jars, vec![out.join("ferrocraft-fabric.jar")]);
        assert_eq!(std::fs::read(&r.jars[0]).unwrap(), ordinary_bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&r.jars[0]).unwrap().permissions().mode() & 0o077, 0);
        }
        assert!(!out.join("renamed.jar").exists());

        std::fs::write(d.join(".oneclient-mods.json"), manifest(&"0".repeat(40))).unwrap();
        assert!(matches!(fabric_jars(&d, &out), Err(Error::Sha1 { .. })));
        std::fs::remove_file(ordinary).unwrap();
        assert!(matches!(fabric_jars(&d, &out), Err(Error::Format(_))));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn fabric_jars_with_one_base_name_fail() {
        let d = temp("dup");
        let jar = mod_jar("ordinary");
        let mut entries = vec![];
        for dir in ["a", "b"] {
            std::fs::create_dir_all(d.join("mods").join(dir)).unwrap();
            std::fs::write(d.join("mods").join(dir).join("x.jar"), &jar).unwrap();
            entries.push(format!(
                r#"{{"path":"mods/{dir}/x.jar","hash":"{}"}}"#,
                sha1_hex(&jar)
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
