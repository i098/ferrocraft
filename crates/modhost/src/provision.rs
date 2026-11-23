//! Gets what the JVM needs and builds the [`Config`]: Mojang's Java 8 (`jre-legacy`), Forge 1.8.9-11.15.1.2318, LaunchWrapper
//! and the libraries, all installed by `portablemc` into `~/.ferrocraft/modhost` (the Minecraft main directory and the
//! game directory of the host). Nothing is installed twice: `portablemc` skips what is there and checked.

use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

use portablemc::{base, forge};
use sha1::{Digest, Sha1};

use crate::{Config, Loader};

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
    /// first call, and `ferro-host.jar` from `java/modhost/build.sh`: `FERROCRAFT_MODHOST_JAR`, or next to this
    /// executable, or in the source tree this was built from.
    pub fn provision() -> Result<Config, Error> {
        let main = main_dir()?;
        let host = host_jar()?;
        let mut installer =
            forge::Installer::new(forge::Loader::Forge, forge::Version::Name(FORGE.into()));
        let moj = installer.mojang_mut();
        moj.base_mut().set_main_dir(&main);
        moj.base_mut().set_jvm_policy(java_policy(&main)?);
        let game = installer.install(())?;

        let cp = game
            .jvm_args
            .iter()
            .zip(game.jvm_args.iter().skip(1))
            .find(|(flag, _)| matches!(flag.as_str(), "-cp" | "-classpath"))
            .map(|(_, cp)| cp)
            .ok_or_else(|| Error::Missing("the Forge version has no class path".into()))?;
        let libs = std::env::split_paths(cp)
            .filter(|p| p.file_name().is_none_or(|name| name != "asm-all-5.0.3.jar"));

        let lib = lib_dir()?;
        fs::create_dir_all(&lib)?;
        match fs::remove_file(lib.join("asm-all-5.0.3.jar")) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut class_path = vec![main.join(HOST_JAR)];
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
        for jar in libs {
            link(&jar, &lib.join(lib_name(&jar, &main.join("versions"))))?;
            class_path.push(jar);
        }
        fs::copy(&host, &class_path[0])?;
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

#[cfg(any(unix, test))]
fn needs_rosetta(os: &str, arch: &str) -> bool {
    (os, arch) == ("macos", "aarch64")
}

fn java_policy(main: &Path) -> Result<base::JvmPolicy, Error> {
    #[cfg(unix)]
    if needs_rosetta(std::env::consts::OS, std::env::consts::ARCH) {
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
        return java8_x64(main).map(base::JvmPolicy::Static);
    }
    let _ = main;
    Ok(base::JvmPolicy::Mojang)
}

#[cfg(unix)]
fn java8_x64(main: &Path) -> Result<PathBuf, Error> {
    use std::os::unix::fs::PermissionsExt;
    let root = main.join("jvm/jre-legacy-mac-os");
    let manifest = root.join("manifest.json");
    fetch(
        "https://piston-meta.mojang.com/v1/packages/78e31aa15b3f437f37263351436f63e0a96e0067/manifest.json",
        &manifest,
        "78e31aa15b3f437f37263351436f63e0a96e0067",
    )?;
    let manifest: serde_json::Value = serde_json::from_reader(fs::File::open(manifest)?)
        .map_err(io::Error::other)?;
    let invalid = || Error::Missing("Invalid Mojang x64 Java 8 manifest".into());
    let files = manifest["files"].as_object().ok_or_else(invalid)?;
    let mut batch = portablemc::download::Batch::new();
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
            batch.push(url, path).set_expected_sha1(Some(sha1));
        }
    }
    if !batch.is_empty() {
        let result = batch.download(()).map_err(io::Error::other)?;
        if let Some(error) = result.iter_errors().next() {
            return Err(io::Error::other(error.to_string()).into());
        }
    }
    for (name, entry) in files {
        if entry["executable"].as_bool() == Some(true) {
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(root.join("jre.bundle/Contents/Home/bin/java"))
}

fn parse_sha1(text: &str) -> Result<[u8; 20], Error> {
    if text.len() != 40 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Missing("Invalid download SHA-1".into()));
    }
    Ok(std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()))
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
        portablemc::download::single(url, path)
            .set_expected_sha1(Some(sha1))
            .download(())
            .map_err(io::Error::other)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn forge_java_uses_rosetta_only_on_apple_silicon() {
        assert!(super::needs_rosetta("macos", "aarch64"));
        assert!(!super::needs_rosetta("macos", "x86_64"));
        assert!(!super::needs_rosetta("linux", "aarch64"));
        assert!(!super::needs_rosetta("windows", "aarch64"));
    }
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
    let _ = fs::remove_file(dst);
    fs::hard_link(src, dst).or_else(|_| fs::copy(src, dst).map(drop))
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
