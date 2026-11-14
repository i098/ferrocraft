//! Fabric side of provisioning: installs Mojang's Java 8, Legacy Fabric for 1.8.9 (loader, intermediary and libraries,
//! through `portablemc`) into `~/.ferrocraft/modhost-fabric`, and builds the name table the host needs: Forge's MCP
//! names to the intermediary names that Knot loads the game under (`java/modhost-prepare` `NameTable`, which reads
//! the MCP SRG file and the intermediary jar with mapping-io). Nothing is installed or built twice.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fmt, fs, io};

use portablemc::{base, fabric};
use sha1::{Digest, Sha1};

use crate::Config;

const GAME: &str = "1.8.9";
const HOST_JAR: &str = "ferro-host.jar";
const NAMES_JAR: &str = "ferro-names.jar";
/// Forge's MCP 1.8.9 SRG data (`joined.srg`: notch names to MCP names), the same file Forge's own tools use.
const SRG_URL: &str =
    "https://maven.minecraftforge.net/de/oceanlabs/mcp/mcp/1.8.9/mcp-1.8.9-srg.zip";
const SRG_SHA1: &str = "7612cfa7e48787ce5aa989c6c513807d18b31a36";

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Install(fabric::Error),
    Download(String),
    /// `HOME` is unset, a jar built by a script is nowhere to be found, or the name table builder failed.
    Missing(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Install(e) => write!(f, "legacy fabric install: {e}"),
            Self::Download(what) | Self::Missing(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<fabric::Error> for Error {
    fn from(e: fabric::Error) -> Self {
        Self::Install(e)
    }
}

/// `~/.ferrocraft/modhost-fabric`: the Minecraft main directory and the game directory of the Fabric host.
pub fn main_dir() -> Result<PathBuf, Error> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let home = home.ok_or_else(|| Error::Missing("HOME is not set".into()))?;
    Ok(Path::new(&home).join(".ferrocraft").join("modhost-fabric"))
}

/// The name table, a Tiny v2 file from Forge (MCP) names to intermediary names.
pub fn names_path() -> Result<PathBuf, Error> {
    Ok(main_dir()?.join("names.tiny"))
}

impl Config {
    /// Install Java 8 and Legacy Fabric 1.8.9, build the name table, and return the config to start the Fabric host
    /// with: the class path is the host jar, then the loader, the intermediary, Mixin, ASM and the game jar. Needs the
    /// network on the first call, `ferro-host.jar` from `java/modhost/build.sh` (`FERROCRAFT_MODHOST_JAR`, or next to
    /// this executable, or in the source tree) and `ferro-names.jar` from `java/modhost-prepare/run.sh names`
    /// (`FERROCRAFT_NAMES_JAR`, same places).
    pub fn provision_fabric() -> Result<Config, Error> {
        let main = main_dir()?;
        let mut installer = fabric::Installer::new(
            fabric::Loader::LegacyFabric,
            fabric::GameVersion::Name(GAME.into()),
            fabric::LoaderVersion::Stable,
        );
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
            .ok_or_else(|| Error::Missing("the Legacy Fabric version has no class path".into()))?;
        let libs: Vec<PathBuf> = std::env::split_paths(cp).collect();

        let table = names_path()?;
        if !table.is_file() {
            let intermediary = libs
                .iter()
                .find(|j| file_name(j).starts_with("intermediary-"))
                .ok_or_else(|| {
                    Error::Missing("the Legacy Fabric class path has no intermediary jar".into())
                })?;
            build_names(&game.jvm_file, intermediary, &main, &table)?;
        }

        let host = find_jar(
            "FERROCRAFT_MODHOST_JAR",
            HOST_JAR,
            "java/modhost",
            "java/modhost/build.sh",
        )?;
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

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Downloads the SRG zip (checked against its sha1) and runs `NameTable` on the Java 8 runtime.
fn build_names(java: &Path, intermediary: &Path, main: &Path, table: &Path) -> Result<(), Error> {
    let names_jar = find_jar(
        "FERROCRAFT_NAMES_JAR",
        NAMES_JAR,
        "java/modhost-prepare",
        "java/modhost-prepare/run.sh names <out-dir>",
    )?;
    let srg = main.join("mcp-1.8.9-srg.zip");
    if !fs::read(&srg).is_ok_and(|b| sha1_hex(&b) == SRG_SHA1) {
        let bytes = ureq::get(SRG_URL)
            .call()
            .and_then(|r| r.into_body().with_config().limit(32 << 20).read_to_vec())
            .map_err(|e| Error::Download(format!("{SRG_URL}: {e}")))?;
        let actual = sha1_hex(&bytes);
        if actual != SRG_SHA1 {
            return Err(Error::Download(format!(
                "sha1 of {SRG_URL} is {actual}, expected {SRG_SHA1}"
            )));
        }
        fs::write(&srg, bytes)?;
    }
    let tmp = table.with_extension("tiny.part");
    let out = Command::new(java)
        .arg("-cp")
        .arg(&names_jar)
        .arg("net.ferrocraft.prepare.NameTable")
        .arg(&srg)
        .arg(intermediary)
        .arg(&tmp)
        .output()?;
    if !out.status.success() {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Missing(format!(
            "NameTable failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(fs::rename(&tmp, table)?)
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A jar a script built: the env var, next to this executable, or in the source tree this was built from.
fn find_jar(env: &str, name: &str, tree_dir: &str, how: &str) -> Result<PathBuf, Error> {
    let mut found = Vec::new();
    found.extend(std::env::var_os(env).map(PathBuf::from));
    found.extend(
        std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.join(name))),
    );
    found.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(tree_dir)
            .join(name),
    );
    found
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| Error::Missing(format!("{name} not found: run {how}, or set {env}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the network, the JDK-built jars and a clean `HOME`; run by hand:
    /// `HOME=$(mktemp -d) FERROCRAFT_MODHOST_JAR=... FERROCRAFT_NAMES_JAR=... cargo test -p ferrocraft-modhost provision_fabric -- --ignored`.
    #[test]
    #[ignore]
    fn clean_home_gives_config_and_name_table() {
        let config = Config::provision_fabric().expect("provision");
        assert!(config.java.is_file());
        assert!(config
            .class_path
            .iter()
            .any(|j| file_name(j).starts_with("fabric-loader-")));
        let table = fs::read_to_string(names_path().unwrap()).unwrap();
        assert!(table.lines().any(|l| l.starts_with(
            "\tf\tLnet/minecraft/client/entity/EntityPlayerSP;\tfield_71439_g\tfield_"
        )));
    }
}
