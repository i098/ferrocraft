//! Gets what the JVM needs and builds the [`Config`]: Mojang's Java 8 (`jre-legacy`), Forge 1.8.9-11.15.1.2318, LaunchWrapper
//! and the libraries, all installed by `portablemc` into `~/.ferrocraft/modhost` (the Minecraft main directory and the
//! game directory of the host). Nothing is installed twice: `portablemc` skips what is there and checked.

use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

use portablemc::{base, forge};

use crate::Config;

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
    Ok(Path::new(&home).join(".ferrocraft").join("modhost"))
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
        moj.base_mut().set_jvm_policy(base::JvmPolicy::Mojang);
        let game = installer.install(())?;

        let cp = game
            .jvm_args
            .iter()
            .zip(game.jvm_args.iter().skip(1))
            .find(|(flag, _)| matches!(flag.as_str(), "-cp" | "-classpath"))
            .map(|(_, cp)| cp)
            .ok_or_else(|| Error::Missing("the Forge version has no class path".into()))?;
        let libs: Vec<PathBuf> = std::env::split_paths(cp).collect();

        let lib = lib_dir()?;
        fs::create_dir_all(&lib)?;
        for jar in &libs {
            link(jar, &lib.join(lib_name(jar, &main.join("versions"))))?;
        }

        let host_dst = main.join(HOST_JAR);
        fs::copy(&host, &host_dst)?;
        let mut class_path = vec![host_dst];
        class_path.extend(libs);
        fs::create_dir_all(main.join("mods"))?;
        Ok(Config {
            java: game.jvm_file,
            class_path,
            assets_dir: main.join("assets"),
            game_dir: main,
        })
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
