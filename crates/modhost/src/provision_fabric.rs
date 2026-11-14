//! Fabric side of provisioning. `provision_fabric` installs Mojang's Java 8, Legacy Fabric for 1.8.9 (loader, intermediary and
//! libraries, through `portablemc`) into `~/.ferrocraft/modhost-fabric`; `provision_ornithe` installs Java 25, Fabric Loader on
//! Ornithe's gen2 intermediary and its library upgrades into `~/.ferrocraft/modhost-ornithe`. Both build the name table the host
//! needs: Forge's MCP names to the intermediary names that Knot loads the game under (`java/modhost-prepare` `NameTable`, which
//! reads the MCP SRG file and the intermediary jar with mapping-io). Nothing is installed or built twice.
//! The class path holds the Fabric host jar first (`java/modhost-fabric/build.sh`): it is the Java agent of the host, and
//! it needs LaunchWrapper's classes, which the loader does not ship.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fmt, fs, io};

use portablemc::{base, fabric, moj};
use sha1::{Digest, Sha1};

use crate::{Config, Loader};

const GAME: &str = "1.8.9";
const HOST_JAR: &str = "ferro-host-fabric.jar";
const NAMES_JAR: &str = "ferro-names.jar";
/// Forge's MCP 1.8.9 SRG data (`joined.srg`: notch names to MCP names), the same file Forge's own tools use.
const SRG_URL: &str =
    "https://maven.minecraftforge.net/de/oceanlabs/mcp/mcp/1.8.9/mcp-1.8.9-srg.zip";
const SRG_SHA1: &str = "7612cfa7e48787ce5aa989c6c513807d18b31a36";
/// Ornithe's meta API: the Fabric Loader 0.20.1 profile of Minecraft 1.8.9 on the gen2 intermediary.
const ORNITHE_PROFILE_URL: &str =
    "https://meta.ornithemc.net/v3/versions/gen2/fabric-loader/1.8.9/0.20.1/profile/json";
const ORNITHE_ID: &str = "fabric-loader-0.20.1-1.8.9-ornithe-gen2";
/// The `libraryUpgrades` of Ornithe's meta API (`/v3/versions`) that apply to 1.8.9, each with the sha1 of the jar.
const ORNITHE_UPGRADES: [(&str, &str); 8] = [
    ("https://libraries.minecraft.net/org/slf4j/slf4j-api/2.0.17/slf4j-api-2.0.17.jar", "d9e58ac9c7779ba3bf8142aff6c830617a7fe60f"),
    ("https://libraries.minecraft.net/org/apache/logging/log4j/log4j-slf4j2-impl/2.19.0/log4j-slf4j2-impl-2.19.0.jar", "5c04bfdd63ce9dceb2e284b81e96b6a70010ee10"),
    ("https://libraries.minecraft.net/org/apache/logging/log4j/log4j-api/2.19.0/log4j-api-2.19.0.jar", "ea1b37f38c327596b216542bc636cfdc0b8036fa"),
    ("https://libraries.minecraft.net/org/apache/logging/log4j/log4j-core/2.19.0/log4j-core-2.19.0.jar", "3b6eeb4de4c49c0fe38a4ee27188ff5fee44d0bb"),
    ("https://libraries.minecraft.net/it/unimi/dsi/fastutil/8.5.9/fastutil-8.5.9.jar", "bb7ea75ecdb216654237830b3a96d87ad91f8cc5"),
    ("https://libraries.minecraft.net/com/google/code/gson/gson/2.10/gson-2.10.jar", "dd9b193aef96e973d5a11ab13cd17430c2e4306b"),
    ("https://maven.ornithemc.net/releases/net/ornithemc/log4j-patch/1.0.0/log4j-patch-1.0.0.jar", "00bf01289838da1061d03a249cca149f3d98c0a0"),
    ("https://libraries.minecraft.net/net/sf/jopt-simple/jopt-simple/5.0.4/jopt-simple-5.0.4.jar", "4fdac2fbe92dfad86aa6e9301736f6b4342a3f5c"),
];
/// LWJGL 3 core and its natives: Argentum, a perf mod of the bundles, allocates with `MemoryUtil` (`pylon` ships them; it is denied).
#[cfg(target_os = "macos")]
const LWJGL3_NATIVES: (&str, &str) = (
    "https://repo1.maven.org/maven2/org/lwjgl/lwjgl/3.3.6/lwjgl-3.3.6-natives-macos-arm64.jar",
    "832e2964ce74e02e768814a29c06d60645115b9a",
);
#[cfg(not(target_os = "macos"))]
const LWJGL3_NATIVES: (&str, &str) = (
    "https://repo1.maven.org/maven2/org/lwjgl/lwjgl/3.3.6/lwjgl-3.3.6-natives-linux.jar",
    "dc7db8fc4f1e6563867f9155b9a42d9c73d8992f",
);
const LWJGL3: [(&str, &str); 2] = [
    (
        "https://repo1.maven.org/maven2/org/lwjgl/lwjgl/3.3.6/lwjgl-3.3.6.jar",
        "ce7721d30cfaf47feaed10213e0c43eb55c49d68",
    ),
    LWJGL3_NATIVES,
];
/// Fabric Loader's dependency overrides: the three mods of the OneClient bundles that depend on `pylon` drop that dependency.
const PYLON_DEPENDS_OVERRIDE: &str = r#"{"version":1,"overrides":{"oneconfig":{"-depends":{"pylon":""}},"forgecompat":{"-depends":{"pylon":""}},"packed_packs":{"-depends":{"pylon":""}}}}"#;
/// `fabric.mod.json` of the mod that stands in for `legacy-lwjgl3` (`pylon` is denied: the host owns the window): SoundFix and
/// ArgentumExtras ask whether it is loaded; the host jar has the class `LegacyLWJGL3` they read (`USE_SDL` is false).
const PYLON_STAND_IN: &str = r#"{"schemaVersion":1,"id":"legacy-lwjgl3","version":"0.2.8","name":"Legacy LWJGL3 (host stand-in)","environment":"client"}"#;
/// LaunchWrapper 1.12: `BridgeTransformer` and the other transformers of the host implement its `IClassTransformer`.
const LW_URL: &str =
    "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar";
const LW_SHA1: &str = "111e7bea9c968cdb3d06ef4632bf7ff0824d0f36";

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
    /// with: the class path is the host jar, LaunchWrapper, the loader, the intermediary, Mixin, ASM and the game jar. Needs the
    /// network on the first call, `ferro-host-fabric.jar` from `java/modhost-fabric/build.sh` (`FERROCRAFT_MODHOST_JAR`, or
    /// next to this executable, or in the source tree) and `ferro-names.jar` from `java/modhost-prepare/run.sh names`
    /// (`FERROCRAFT_NAMES_JAR`, same places).
    pub fn provision_fabric() -> Result<Config, Error> {
        let main = main_dir()?;
        let mut installer = fabric::Installer::new(
            fabric::Loader::LegacyFabric,
            fabric::GameVersion::Name(GAME.into()),
            // 0.20.1 cannot patch Minecraft 1.8.9 ("Could not find game constructor"); 0.19.5 starts it.
            fabric::LoaderVersion::Name("0.19.5".into()),
        );
        let moj = installer.mojang_mut();
        moj.base_mut().set_main_dir(&main);
        moj.base_mut().set_jvm_policy(base::JvmPolicy::Mojang);
        let game = installer.install(())?;
        assemble(&main, game, &[])
    }

    /// The Ornithe profile: Java 25 (Mojang's `java-runtime-epsilon`), Fabric Loader 0.20.1 on Ornithe's gen2 intermediary
    /// (the names of the public OneClient 1.8.9 bundles), and Ornithe's library upgrades for 1.8.9 in place of the old
    /// log4j, gson and jopt-simple of the game. Everything lives in `~/.ferrocraft/modhost-ornithe`. The host owns the window and
    /// the input, so `pylon` (the mod that opens an SDL window and reads the keyboard) never loads: `mods/` loses any
    /// `pylon` jar and gets a stand-in mod of that id, which keeps the mods that depend on it. Same inputs as `provision_fabric`.
    pub fn provision_ornithe() -> Result<Config, Error> {
        let main = ornithe_dir()?;
        let profile = main
            .join("versions")
            .join(ORNITHE_ID)
            .join(format!("{ORNITHE_ID}.json"));
        if !profile.is_file() {
            let json = ureq::get(ORNITHE_PROFILE_URL)
                .call()
                .and_then(|r| r.into_body().read_to_string())
                .map_err(|e| Error::Download(format!("{ORNITHE_PROFILE_URL}: {e}")))?;
            // The meta API inherits from "1.8.9-vanilla"; the installer knows the version as "1.8.9". The runtime is Java 25.
            let json = json
                .replacen("\"inheritsFrom\":\"1.8.9-vanilla\"", "\"inheritsFrom\":\"1.8.9\"", 1)
                .replacen('{', "{\"javaVersion\":{\"component\":\"java-runtime-epsilon\",\"majorVersion\":25},", 1);
            if !json.contains("\"inheritsFrom\":\"1.8.9\"") {
                return Err(Error::Download(format!(
                    "{ORNITHE_PROFILE_URL}: unexpected profile"
                )));
            }
            let part = profile.with_extension("json.part");
            fs::create_dir_all(part.parent().unwrap_or(&main))?;
            fs::write(&part, json)?;
            fs::rename(&part, &profile)?;
        }
        let mut installer = moj::Installer::new(ORNITHE_ID);
        installer.base_mut().set_main_dir(&main);
        installer.base_mut().set_jvm_policy(base::JvmPolicy::Mojang);
        let game = installer
            .install(())
            .map_err(|e| Error::Missing(format!("ornithe install: {e}")))?;

        let upgrades = ORNITHE_UPGRADES
            .iter()
            .map(|(url, sha1)| {
                let dest = main
                    .join("upgrades")
                    .join(url.rsplit('/').next().unwrap_or(url));
                fs::create_dir_all(main.join("upgrades"))?;
                fetch(url, sha1, &dest).map(|()| dest)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut config = assemble(&main, game, &upgrades)?;
        // LWJGL 3 (core and the natives of this machine) for the mods that read its memory classes: the game's own LWJGL 2 stays.
        // They go right after the host jar and LaunchWrapper: LWJGL 2 has classes of the same names (`org.lwjgl.system.*`) that must lose.
        for (i, (url, sha1)) in LWJGL3.iter().enumerate() {
            let dest = main
                .join("upgrades")
                .join(url.rsplit('/').next().unwrap_or(url));
            fetch(url, sha1, &dest)?;
            config.class_path.insert(2 + i, dest);
        }

        let mods = main.join("mods");
        for entry in fs::read_dir(&mods)? {
            let path = entry?.path();
            if file_name(&path).to_lowercase().starts_with("pylon")
                && path.extension().is_some_and(|e| e == "jar")
            {
                fs::remove_file(path)?;
            }
        }
        let stub = mods.join("ferro-pylon-stand-in.jar");
        let jar = stored_jar("fabric.mod.json", PYLON_STAND_IN.as_bytes());
        if fs::read(&stub).map_or(true, |b| b != jar) {
            fs::write(&stub, jar)?;
        }
        // The mods that depend on `pylon` need not have it: the host owns the window and the input. A file the player wrote stays.
        let overrides = main.join("config").join("fabric_loader_dependencies.json");
        if !overrides.exists() {
            fs::create_dir_all(main.join("config"))?;
            fs::write(&overrides, PYLON_DEPENDS_OVERRIDE)?;
        }
        Ok(config)
    }
}

/// `~/.ferrocraft/modhost-ornithe`: the Minecraft main directory and the game directory of the Ornithe profile.
pub fn ornithe_dir() -> Result<PathBuf, Error> {
    Ok(main_dir()?.with_file_name("modhost-ornithe"))
}

/// What both profiles share after the installer ran: the name table (from the intermediary jar on the class path), the host jar
/// and LaunchWrapper, and the class path with `upgrades` in place of the game's older libraries of the same names.
fn assemble(main: &Path, game: base::Game, upgrades: &[PathBuf]) -> Result<Config, Error> {
    let cp = game
        .jvm_args
        .iter()
        .zip(game.jvm_args.iter().skip(1))
        .find(|(flag, _)| matches!(flag.as_str(), "-cp" | "-classpath"))
        .map(|(_, cp)| cp)
        .ok_or_else(|| Error::Missing("the loader version has no class path".into()))?;
    let upgraded: Vec<String> = upgrades.iter().map(|u| artifact(&file_name(u))).collect();
    let libs: Vec<PathBuf> = std::env::split_paths(cp)
        .filter(|j| !upgraded.contains(&artifact(&file_name(j))))
        .collect();

    let table = main.join("names.tiny");
    if !table.is_file() {
        let intermediary = libs
            .iter()
            .find(|j| file_name(j).contains("intermediary-"))
            .ok_or_else(|| {
                Error::Missing("the loader class path has no intermediary jar".into())
            })?;
        build_names(&game.jvm_file, intermediary, main, &table)?;
    }

    let host = find_jar(
        "FERROCRAFT_MODHOST_JAR",
        HOST_JAR,
        "target/modhost-fabric",
        "java/modhost-fabric/build.sh",
    )?;
    let host_dst = main.join(HOST_JAR);
    fs::copy(&host, &host_dst)?;
    let lw = main.join("launchwrapper-1.12.jar");
    fetch(LW_URL, LW_SHA1, &lw)?;
    let mut class_path = vec![host_dst.clone(), lw];
    class_path.extend_from_slice(upgrades);
    class_path.extend(libs);
    fs::create_dir_all(main.join("mods"))?;
    Ok(Config {
        loader: Loader::Fabric {
            agent: host_dst,
            names: table,
        },
        java: game.jvm_file,
        class_path,
        assets_dir: main.join("assets"),
        game_dir: main.to_path_buf(),
    })
}

/// The artifact of a library file: `log4j-api-2.0-beta9.jar` and `log4j-api-2.19.0.jar` are both `log4j-api`.
fn artifact(file: &str) -> String {
    let stem = file.strip_suffix(".jar").unwrap_or(file);
    let cut = stem
        .match_indices('-')
        .find(|(i, _)| stem[i + 1..].starts_with(|c: char| c.is_ascii_digit()))
        .map_or(stem.len(), |(i, _)| i);
    stem[..cut].to_string()
}

/// A jar with one stored (not compressed) entry: the host needs no zip crate for its stand-in mod.
fn stored_jar(name: &str, data: &[u8]) -> Vec<u8> {
    let crc = crc32(data);
    let (size, len) = (data.len() as u32, name.len() as u16);
    let mut common = Vec::new();
    for field in [10u16, 0, 0, 0, 0x21] {
        // version needed, flags, method stored, time, date 1980-01-01
        common.extend(field.to_le_bytes());
    }
    common.extend(crc.to_le_bytes());
    common.extend(size.to_le_bytes());
    common.extend(size.to_le_bytes());
    common.extend(len.to_le_bytes());
    let mut jar = b"PK\x03\x04".to_vec();
    jar.extend(&common);
    jar.extend([0, 0]); // extra length
    jar.extend(name.as_bytes());
    jar.extend(data);
    let dir = jar.len() as u32;
    jar.extend(b"PK\x01\x02\x0a\x00"); // central entry, version made by
    jar.extend(&common);
    jar.extend([0u8; 12]); // extra, comment, disk, internal and external attributes: 2 + 2 + 2 + 2 + 4
    jar.extend(0u32.to_le_bytes()); // offset of the local entry
    jar.extend(name.as_bytes());
    let dir_len = jar.len() as u32 - dir;
    jar.extend(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
    jar.extend(dir_len.to_le_bytes());
    jar.extend(dir.to_le_bytes());
    jar.extend([0, 0]); // comment length
    jar
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Downloads the SRG zip (checked against its sha1) and runs `NameTable` on the runtime of the profile (Java 8, or Java 25 for Ornithe).
fn build_names(java: &Path, intermediary: &Path, main: &Path, table: &Path) -> Result<(), Error> {
    let names_jar = find_jar(
        "FERROCRAFT_NAMES_JAR",
        NAMES_JAR,
        "java/modhost-prepare",
        "java/modhost-prepare/run.sh names <out-dir>",
    )?;
    let srg = main.join("mcp-1.8.9-srg.zip");
    fetch(SRG_URL, SRG_SHA1, &srg)?;
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

/// Downloads `url` to `dest` unless `dest` already has the sha1.
fn fetch(url: &str, sha1: &str, dest: &Path) -> Result<(), Error> {
    if fs::read(dest).is_ok_and(|b| sha1_hex(&b) == sha1) {
        return Ok(());
    }
    let bytes = ureq::get(url)
        .call()
        .and_then(|r| r.into_body().with_config().limit(32 << 20).read_to_vec())
        .map_err(|e| Error::Download(format!("{url}: {e}")))?;
    let actual = sha1_hex(&bytes);
    if actual != sha1 {
        return Err(Error::Download(format!(
            "sha1 of {url} is {actual}, expected {sha1}"
        )));
    }
    Ok(fs::write(dest, bytes)?)
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
        write_config(&config);
    }

    /// As above for the Ornithe profile: Java 25, the upgraded libraries first, gen2 names, `pylon` replaced by the stand-in.
    #[test]
    #[ignore]
    fn clean_home_gives_ornithe_config() {
        let mods = ornithe_dir().unwrap().join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("pylon-0.2.8.jar"), b"the real pylon").unwrap();
        let config = Config::provision_ornithe().expect("provision");
        let version = Command::new(&config.java).arg("-version").output().unwrap();
        assert!(String::from_utf8_lossy(&version.stderr).contains("\"25."));
        let names: Vec<String> = config.class_path.iter().map(|j| file_name(j)).collect();
        assert!(names.iter().any(|n| n == "log4j-core-2.19.0.jar"));
        assert!(!names.iter().any(|n| n.starts_with("log4j-core-2.0")));
        assert!(names.iter().any(|n| n.starts_with("fabric-loader-0.20.1")));
        let table = fs::read_to_string(ornithe_dir().unwrap().join("names.tiny")).unwrap();
        assert!(table.lines().any(|l| l
            .starts_with("\tf\tLnet/minecraft/client/entity/EntityPlayerSP;\tfield_71439_g\tf_")));
        assert!(!mods.join("pylon-0.2.8.jar").exists());
        assert!(mods.join("ferro-pylon-stand-in.jar").is_file());
        let overrides = fs::read_to_string(
            ornithe_dir()
                .unwrap()
                .join("config/fabric_loader_dependencies.json"),
        )
        .unwrap();
        assert!(overrides.contains("\"-depends\""));
        write_config(&config);
    }

    /// The config file of java/modhost-fabric/test.sh: java, main dir, class path.
    fn write_config(config: &Config) {
        if let Some(out) = std::env::var_os("FERRO_FABRIC_CONFIG_OUT") {
            let cp = std::env::join_paths(&config.class_path).unwrap();
            let text = format!(
                "{}\n{}\n{}\n",
                config.java.display(),
                config.game_dir.display(),
                cp.to_string_lossy()
            );
            fs::write(out, text).unwrap();
        }
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn artifact_drops_the_version() {
        assert_eq!(artifact("log4j-api-2.0-beta9.jar"), "log4j-api");
        assert_eq!(
            artifact("log4j-slf4j2-impl-2.19.0.jar"),
            "log4j-slf4j2-impl"
        );
        assert_eq!(
            artifact("calamus-intermediary-gen2-1.8.9.jar"),
            "calamus-intermediary-gen2"
        );
        assert_eq!(artifact("jopt-simple-4.6.jar"), "jopt-simple");
    }

    /// The central directory, the end record and the local entry must agree on offsets and sizes, or the loader cannot open the jar.
    #[test]
    fn stored_jar_has_consistent_offsets() {
        let jar = stored_jar("fabric.mod.json", b"{}");
        let eocd = jar.len() - 22;
        assert_eq!(&jar[eocd..eocd + 4], b"PK\x05\x06");
        let u32_at = |i: usize| u32::from_le_bytes(jar[i..i + 4].try_into().unwrap()) as usize;
        let (dir_len, dir) = (u32_at(eocd + 12), u32_at(eocd + 16));
        assert_eq!(dir + dir_len, eocd);
        assert_eq!(&jar[dir..dir + 4], b"PK\x01\x02");
        assert_eq!(&jar[..4], b"PK\x03\x04");
        assert_eq!(&jar[30..45], b"fabric.mod.json");
        assert_eq!(&jar[45..47], b"{}");
        assert_eq!(u32_at(dir + 42), 0);
    }
}
