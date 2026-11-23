//! Prepares the shared Forge cache and builds the [`Config`].
//! Plain Forge launches use a separate game directory for each host process.

use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

use portablemc::{base, forge, moj};
use sha1::{Digest, Sha1};

use crate::{Config, Loader, StagingDir};

/// Forge 1.8.9 build the mods are made for.
const FORGE: &str = "1.8.9-11.15.1.2318-1.8.9";
const HOST_JAR: &str = "ferro-host.jar";

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Install(forge::Error),
    /// `HOME` is unset, or the host jar built by `java/modhost/build.sh` is nowhere to be found.
    Missing(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Install(e) => write!(f, "forge install: {e}"),
            Self::Missing(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<forge::Error> for Error {
    fn from(e: forge::Error) -> Self {
        Self::Install(e)
    }
}

/// `~/.ferrocraft/modhost`.
pub fn main_dir() -> Result<PathBuf, Error> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let home = home.ok_or_else(|| Error::Missing("HOME is not set".into()))?;
    Ok(std::path::absolute(Path::new(&home))?
        .join(".ferrocraft")
        .join("modhost"))
}

/// Folder with the jars under the names `java/modhost/build.sh` and `test.sh` expect (`FERRO_LIB`): `lw.jar`, `forge.jar`,
/// `minecraft-1.8.9.jar` and the other libraries under their own names.
pub fn lib_dir() -> Result<PathBuf, Error> {
    Ok(main_dir()?.join("lib"))
}

impl Config {
    /// Install Java 8, Forge and the libraries, and return the config to start the host with. Needs the network on the
    /// first call, and `ferro-host.jar` from the release archive or `java/modhost/build.sh`.
    pub fn provision() -> Result<Config, Error> {
        let main = main_dir()?;
        let host = host_jar()?;
        fs::create_dir_all(&main)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(main.join("provision.lock"))?;
        lock.lock()?;
        discard_partial_version(&main, &format!("forge-{FORGE}"))?;
        discard_partial_version(&main, "1.8.9")?;
        fetch(
            "https://launchermeta.mojang.com/v1/packages/f6ad102bcaa53b1a58358f16e376d548d44933ec/1.8.json",
            &main.join("assets/indexes/1.8.json"),
            "f6ad102bcaa53b1a58358f16e376d548d44933ec",
        )?;
        let mut installer =
            forge::Installer::new(forge::Loader::Forge, forge::Version::Name(FORGE.into()));
        let moj = installer.mojang_mut();
        moj.base_mut().set_main_dir(&main);
        #[cfg(unix)]
        let policy = java_policy(
            &main,
            std::env::consts::OS,
            std::env::consts::ARCH,
            java8_x64,
        )?;
        #[cfg(not(unix))]
        let policy = base::JvmPolicy::Mojang;
        moj.base_mut().set_jvm_policy(policy);
        let mut legacy = LegacyLibraries {
            dir: main.join("libraries"),
            result: Ok(()),
        };
        let game = installer.install(&mut legacy);
        legacy.result?;
        let game = game?;

        let cp = game
            .jvm_args
            .iter()
            .zip(game.jvm_args.iter().skip(1))
            .find(|(flag, _)| matches!(flag.as_str(), "-cp" | "-classpath"))
            .map(|(_, cp)| cp)
            .ok_or_else(|| Error::Missing("the Forge version has no class path".into()))?;
        let lib = lib_dir()?;
        fs::create_dir_all(&lib)?;
        match fs::remove_file(lib.join("asm-all-5.0.3.jar")) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut class_path = vec![std::path::absolute(host)?];
        for (name, url, sha1) in [
            (
                "asm-all-5.2.jar",
                "https://maven.minecraftforge.net/org/ow2/asm/asm-all/5.2/asm-all-5.2.jar",
                "2ea49e08b876bbd33e0a7ce75c8f371d29e1f10a",
            ),
            (
                "maven-artifact-3.3.9.jar",
                "https://repo.maven.apache.org/maven2/org/apache/maven/maven-artifact/3.3.9/maven-artifact-3.3.9.jar",
                "0f43afa184555fbc6e36b3334b17246c39b30f6e",
            ),
        ] {
            let path = lib.join(name);
            fetch(url, &path, sha1)?;
            class_path.push(path);
        }
        for jar in std::env::split_paths(cp) {
            link(&jar, &lib.join(lib_name(&jar, &main.join("versions"))))?;
            class_path.push(jar);
        }
        fs::create_dir_all(main.join("mods"))?;
        Ok(Config {
            loader: Loader::Forge,
            java: game.jvm_file,
            class_path,
            assets_dir: main.join("assets"),
            game_dir: main,
        })
    }
}

fn discard_partial_version(main: &Path, version: &str) -> io::Result<()> {
    let path = main
        .join("versions")
        .join(version)
        .join(format!("{version}.json"));
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    match serde_json::from_reader::<_, serde_json::Value>(io::BufReader::new(file)) {
        Ok(_) => Ok(()),
        Err(e) if e.is_syntax() || e.is_eof() => fs::remove_file(path),
        Err(e) => Err(io::Error::from(e)),
    }
}

struct LegacyLibraries {
    dir: PathBuf,
    result: io::Result<()>,
}

impl forge::Handler for LegacyLibraries {
    fn on_event(&mut self, event: forge::Event<'_>) {
        if let forge::Event::Mojang(moj::Event::Base(base::Event::FilterLibraries { libraries })) =
            event
        {
            libraries.retain(|lib| {
                !(lib.name.group() == "org.ow2.asm"
                    && lib.name.artifact() == "asm-all"
                    && lib.name.version() == "5.0.3")
            });
            for lib in libraries {
                let Some(download) = lib.download.as_mut() else {
                    continue;
                };
                if lib.path.is_some() || download.size.is_some() || download.sha1.is_some() {
                    continue;
                }
                if lib.name.group() == "net.minecraftforge"
                    && lib.name.artifact() == "forge"
                    && lib.name.version() == FORGE
                {
                    download.url = format!(
                        "https://maven.minecraftforge.net/net/minecraftforge/forge/{FORGE}/forge-{FORGE}-universal.jar"
                    );
                    download.sha1 =
                        Some(parse_sha1("beda619c465af293e63952dd573c137c17c0a4cd").unwrap());
                }
                if let Err(error) = discard_partial_library(&self.dir.join(lib.name.file())) {
                    self.result = Err(error);
                    return;
                }
            }
        }
    }
}

fn discard_partial_library(path: &Path) -> io::Result<()> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    match zip::ZipArchive::new(file) {
        Ok(jar) if !jar.is_empty() => Ok(()),
        Err(zip::result::ZipError::Io(e)) if e.kind() != io::ErrorKind::UnexpectedEof => Err(e),
        _ => fs::remove_file(path),
    }
}

#[cfg(any(unix, test))]
fn java_policy(
    main: &Path,
    os: &str,
    arch: &str,
    install_x64: impl FnOnce(&Path, &str) -> Result<PathBuf, Error>,
) -> Result<base::JvmPolicy, Error> {
    if (os, arch) == ("macos", "aarch64") {
        install_x64(
            &main.join("jvm/jre-legacy-mac-os"),
            "78e31aa15b3f437f37263351436f63e0a96e0067",
        )
        .map(base::JvmPolicy::Static)
    } else {
        Ok(base::JvmPolicy::Mojang)
    }
}

#[cfg(unix)]
fn java8_x64(root: &Path, manifest_sha1: &str) -> Result<PathBuf, Error> {
    use std::process::{Command, Stdio};
    let supported = Command::new("/usr/bin/arch")
        .args(["-x86_64", "/usr/bin/true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !supported {
        return Err(Error::Missing(
            "Forge needs Rosetta to run Java 8. Run `softwareupdate --install-rosetta`.".into(),
        ));
    }
    let manifest = root.join("manifest.json");
    fetch(
        &format!("https://piston-meta.mojang.com/v1/packages/{manifest_sha1}/manifest.json"),
        &manifest,
        manifest_sha1,
    )?;
    let manifest: serde_json::Value =
        serde_json::from_reader(fs::File::open(manifest)?).map_err(io::Error::other)?;
    let invalid = || Error::Missing("Invalid Mojang x64 Java 8 manifest".into());
    let files = manifest["files"].as_object().ok_or_else(invalid)?;
    download_java_files(root, files)?;
    Ok(root.join("jre.bundle/Contents/Home/bin/java"))
}

#[cfg(unix)]
fn download_java_files(
    root: &Path,
    files: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let invalid = || Error::Missing("Invalid Mojang x64 Java 8 manifest".into());
    let mut batch = portablemc::download::Batch::new();
    fs::create_dir_all(root)?;
    let scratch = StagingDir::new(root, "download")?;
    let mut pending = Vec::new();
    for (name, entry) in files {
        let relative = Path::new(name);
        if !relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(invalid());
        }
        let path = root.join(relative);
        let raw = &entry["downloads"]["raw"];
        let url = raw["url"].as_str().ok_or_else(invalid)?;
        let sha1 = parse_sha1(raw["sha1"].as_str().ok_or_else(invalid)?)?;
        if !matches_sha1(&path, sha1) {
            let temp = scratch.0.join(relative);
            batch.push(url, temp.clone()).set_expected_sha1(Some(sha1));
            pending.push((temp, path));
        }
    }
    if !batch.is_empty() {
        let result = batch.download(()).map_err(io::Error::other)?;
        if let Some(error) = result.iter_errors().next() {
            return Err(io::Error::other(error.to_string()).into());
        }
    }
    for (temp, path) in pending {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::rename(temp, path)?;
    }
    for (name, entry) in files {
        if entry["executable"].as_bool() == Some(true) {
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

fn parse_sha1(text: &str) -> Result<[u8; 20], Error> {
    if text.len() != 40 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Missing("Invalid download SHA-1".into()));
    }
    Ok(std::array::from_fn(|i| {
        u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()
    }))
}

fn matches_sha1(path: &Path, expected: [u8; 20]) -> bool {
    let mut hash = Sha1::new();
    fs::File::open(path)
        .and_then(|mut file| io::copy(&mut file, &mut hash))
        .is_ok()
        && hash.finalize()[..] == expected
}

fn fetch(url: &str, path: &Path, sha1: &str) -> Result<(), Error> {
    let sha1 = parse_sha1(sha1)?;
    if !matches_sha1(path, sha1) {
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent)?;
        let scratch = StagingDir::new(parent, "download")?;
        let temp = scratch.0.join("file");
        portablemc::download::single(url, temp.as_path())
            .set_expected_sha1(Some(sha1))
            .download(())
            .map_err(io::Error::other)?;
        fs::rename(temp, path)?;
    }
    Ok(())
}

/// The name of a class path jar in `lib_dir`.
fn lib_name(jar: &Path, versions: &Path) -> String {
    let name = jar.file_name().unwrap_or_default().to_string_lossy();
    if jar.starts_with(versions) {
        "minecraft-1.8.9.jar".into()
    } else if name.starts_with("launchwrapper-") {
        "lw.jar".into()
    } else if name.starts_with("forge-") {
        "forge.jar".into()
    } else {
        name.into_owned()
    }
}

/// Hard link, or copy across file systems; replaces what is there.
fn link(src: &Path, dst: &Path) -> io::Result<()> {
    let scratch = StagingDir::new(dst.parent().unwrap(), "export")?;
    let temp = scratch.0.join("jar");
    fs::hard_link(src, &temp).or_else(|_| fs::copy(src, &temp).map(drop))?;
    fs::rename(temp, dst)
}

fn host_jar() -> Result<PathBuf, Error> {
    let mut found = Vec::new();
    found.extend(std::env::var_os("FERROCRAFT_MODHOST_JAR").map(PathBuf::from));
    found.extend(
        std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.join(HOST_JAR))),
    );
    found.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../java/modhost")
            .join(HOST_JAR),
    );
    found.iter().find(|p| p.is_file()).cloned().ok_or_else(|| {
        Error::Missing(format!(
            "{HOST_JAR} not found: run java/modhost/build.sh, or set FERROCRAFT_MODHOST_JAR"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_version_write_allows_reinstall() {
        let root = std::env::temp_dir().join(format!("fc-version-{}", std::process::id()));
        let version = "forge-1.8.9-11.15.1.2318-1.8.9";
        let dir = root.join("versions").join(version);
        let file = dir.join(format!("{version}.json"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(&file, b"{\"id\":").unwrap();
        discard_partial_version(&root, version).unwrap();
        assert!(!file.exists());
        let complete = br#"{"id":"forge-1.8.9-11.15.1.2318-1.8.9","inheritsFrom":"1.8.9"}"#;
        fs::write(&file, complete).unwrap();
        discard_partial_version(&root, version).unwrap();
        assert_eq!(fs::read(file).unwrap(), complete);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn forge_java_uses_rosetta_only_on_apple_silicon() {
        let cache = Path::new("cache");
        let policy = java_policy(cache, "macos", "aarch64", |root, manifest_sha1| {
            assert_eq!(manifest_sha1, "78e31aa15b3f437f37263351436f63e0a96e0067");
            Ok(root.join("jre.bundle/Contents/Home/bin/java"))
        })
        .unwrap();
        assert!(matches!(policy, base::JvmPolicy::Static(path)
            if path == Path::new("cache/jvm/jre-legacy-mac-os/jre.bundle/Contents/Home/bin/java")));
        for (os, arch) in [
            ("macos", "x86_64"),
            ("linux", "aarch64"),
            ("windows", "aarch64"),
        ] {
            let policy =
                java_policy(cache, os, arch, |_, _| panic!("unexpected x64 install")).unwrap();
            assert!(matches!(policy, base::JvmPolicy::Mojang));
        }
    }
}
