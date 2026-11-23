//! Install an unmodified OneClient release from Polyfrost.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const VERSION: &str = "2.12.0";

pub fn require_closed() -> Result<(), String> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("tasklist.exe");
        command.args(["/FO", "CSV", "/NH"]);
        command
    } else {
        let mut command = Command::new("/bin/ps");
        command.args(["-A", "-o", "comm="]);
        command
    };
    let output = command
        .output()
        .map_err(|e| format!("Cannot check if OneClient is running: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cannot check if OneClient is running: {}",
            output.status
        ));
    }
    if String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| is_oneclient_process(line, cfg!(windows)))
    {
        return Err("Close OneClient and keep it closed until the command finishes.".into());
    }
    Ok(())
}

fn is_oneclient_process(line: &str, windows: bool) -> bool {
    if windows {
        return line
            .strip_prefix('"')
            .and_then(|line| line.split_once("\","))
            .is_some_and(|(name, _)| {
                name.eq_ignore_ascii_case("oneclient_app.exe")
                    || name.eq_ignore_ascii_case("OneClient.exe")
            });
    }
    Path::new(line.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(name, "oneclient_app" | "OneClient" | "OneClient.AppImage" | "OneClient.AppIm")
                || name.starts_with("OneClient_")
                || name.starts_with("oneclient_app_")
        })
}

pub type Storage = (PathBuf, Option<PathBuf>);

pub fn data_dirs(app_data: &Path) -> Result<Storage, String> {
    resolve_user_storage(app_data).map_err(|e| format!("OneClient data directory: {e}"))
}

fn resolve_user_storage(app_data: &Path) -> io::Result<Storage> {
    if std::env::vars_os().any(|(key, _)| {
        key.to_str()
            .and_then(|key| key.strip_prefix("ONECLIENT_SETTING_"))
            .is_some_and(|key| key.eq_ignore_ascii_case("data_dir"))
    }) {
        return Err(io::Error::other(
            "Unset ONECLIENT_SETTING_DATA_DIR and save the data directory in OneClient settings",
        ));
    }
    #[cfg(windows)]
    let legacy = dirs::data_local_dir()
        .ok_or_else(|| io::Error::other("Cannot locate Local AppData"))?
        .join("Polyfrost/OneClient/data");
    #[cfg(target_os = "macos")]
    let legacy = app_data.join("org.Polyfrost.OneClient");
    #[cfg(not(any(windows, target_os = "macos")))]
    let legacy = app_data.join("oneclient");

    resolve_data_dir(app_data.join("Polyfrost/OneClient"), &legacy)
}

pub(super) fn resolve_data_dir(standard: PathBuf, legacy: &Path) -> io::Result<Storage> {
    if !standard.is_absolute() {
        return Err(io::Error::other(
            "Cannot locate the standard OneClient data directory",
        ));
    }
    let legacy_settings = legacy.join("settings.json");
    let standard_settings = standard.join("settings.json");
    let has_legacy = legacy_settings.try_exists()?;
    let settings = if has_legacy {
        &legacy_settings
    } else {
        &standard_settings
    };
    let bytes = match fs::read(settings) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound && !has_legacy => return Ok((standard, None)),
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                format!("{}: {e}", settings.display()),
            ));
        }
    };
    let config: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::other(format!("{}: {e}", settings.display())))?;
    let previous = previous_data_dir(&config["previous_data_dir"], settings)?;
    let active = active_data_dir(&config, standard, has_legacy.then_some(legacy), settings)?;
    Ok((active, previous))
}

fn active_data_dir(
    config: &serde_json::Value,
    standard: PathBuf,
    legacy: Option<&Path>,
    settings: &Path,
) -> io::Result<PathBuf> {
    if config.is_object() && config["data_dir"].is_null() {
        if let Some(legacy) = legacy {
            return Err(io::Error::other(format!(
                "{}: OneClient can move this legacy directory at startup; open OneClient first or set a data directory in OneClient",
                legacy.display()
            )));
        }
        return Ok(standard);
    }
    if let Some(path) = config["data_dir"].as_str().map(PathBuf::from) {
        if path.is_absolute() && !path.join(".oneclient_migrating").try_exists()? {
            return Ok(path);
        }
    }
    Err(io::Error::other(format!(
        "{}: invalid data directory or unfinished move; check the storage settings in OneClient",
        settings.display()
    )))
}

fn previous_data_dir(value: &serde_json::Value, settings: &Path) -> io::Result<Option<PathBuf>> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(path) if Path::new(path).is_absolute() => {
            Ok(Some(PathBuf::from(path)))
        }
        _ => Err(io::Error::other(format!(
            "{}: previous data directory must be an absolute path",
            settings.display()
        ))),
    }
}

// Digests from https://github.com/Polyfrost/OneLauncher/releases/tag/oneclient-2.12.0.
fn release_asset(os: &str, arch: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match (os, arch) {
        ("linux", "x86_64") => Some((
            "linux_x86_64.AppImage",
            "5cdbc52cd1b67a72e08e3eca49001ac7e185b27657722fc286bda53a66d5cf63",
            "OneClient.AppImage",
        )),
        ("macos", "aarch64") => Some((
            "darwin_aarch64.app.tar.gz",
            "d1bd1255dfa353dbf3c88bf332d9cf52547d01cef2969ab4181cbf853414925d",
            "OneClient.app/Contents/MacOS/oneclient_app",
        )),
        ("macos", "x86_64") => Some((
            "darwin_x86_64.app.tar.gz",
            "e6c5f9d19a2213c9329837ef17219089b7afed3ca328169c83fe25aba4133c02",
            "OneClient.app/Contents/MacOS/oneclient_app",
        )),
        ("windows", "x86_64") => Some((
            "windows_x86_64.exe",
            "86cf2fbfc131251026a7aef5033c0f9614e82607b8724d7165a865ef2c8d9dc3",
            "oneclient_app.exe",
        )),
        _ => None,
    }
}

fn check(mut bytes: impl Read, expected: &str) -> io::Result<()> {
    let mut hash = Sha256::new();
    let mut buf = [0; 16 * 1024];
    loop {
        match bytes.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hash.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    let actual = hash.finalize();
    let hex = b"0123456789abcdef";
    let encoded = actual
        .iter()
        .flat_map(|&b| [hex[(b >> 4) as usize], hex[(b & 0xf) as usize]]);
    if !encoded.eq(expected.bytes()) {
        return Err(io::Error::other("OneClient SHA-256 mismatch"));
    }
    Ok(())
}

fn run(command: &mut Command) -> io::Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "{:?} exited with {status}",
            command.get_program()
        )));
    }
    Ok(())
}

pub fn prepare(
    base: &Path,
    resolve_storage: impl FnOnce() -> Result<Storage, String>,
) -> Result<(PathBuf, Storage), String> {
    #[cfg(windows)]
    let installed = installed_windows(&base.join(format!("oneclient-{VERSION}")))
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let installed: Option<PathBuf> = None;
    let storage = resolve_storage()?;
    let app = match installed {
        Some(app) => app,
        None => install(base)
            .map_err(|e| format!("OneClient install failed: {e}"))?,
    };
    Ok((app, storage))
}

fn install(base: &Path) -> io::Result<PathBuf> {
    let Some((asset, digest, executable)) =
        release_asset(std::env::consts::OS, std::env::consts::ARCH)
    else {
        return Err(io::Error::other(
            "OneClient is not available for this platform",
        ));
    };
    let dest = base.join(format!("oneclient-{VERSION}"));
    let installed = dest.join(executable);
    #[cfg(not(windows))]
    if installed.is_file() {
        return Ok(installed);
    }

    fs::create_dir_all(base)?;
    let stage = tempfile::tempdir_in(base)?;
    let asset = format!("OneClient_{VERSION}_{asset}");
    let url = format!(
        "https://github.com/Polyfrost/OneLauncher/releases/download/oneclient-{VERSION}/{asset}"
    );
    let archive = stage.path().join(asset);
    println!("Downloading OneClient {VERSION} from Polyfrost.");
    run(
        Command::new(if cfg!(windows) { "curl.exe" } else { "curl" })
            .args([
                "--disable",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-time",
                "300",
                "--max-filesize",
                "100000000",
                "--output",
            ])
            .arg(&archive)
            .arg(&url),
    )?;
    check(fs::File::open(&archive)?, digest)?;
    println!("Verified the OneClient download (SHA-256).");
    #[cfg(windows)]
    if let Some(app) = installed_windows(&dest)? {
        return Ok(app);
    }
    unpack(&archive, stage.path(), &dest)?;
    if !installed.is_file() {
        return Err(io::Error::other(format!(
            "OneClient executable is missing: {}",
            installed.display()
        )));
    }
    Ok(installed)
}

#[cfg(target_os = "linux")]
fn unpack(archive: &Path, stage: &Path, dest: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let app = stage.join("app");
    fs::create_dir(&app)?;
    let path = app.join("OneClient.AppImage");
    fs::rename(archive, &path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    fs::rename(app, dest)
}

#[cfg(target_os = "macos")]
fn unpack(archive: &Path, stage: &Path, dest: &Path) -> io::Result<()> {
    let app = stage.join("app");
    fs::create_dir(&app)?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&app))?;
    fs::rename(app, dest)
}

#[cfg(windows)]
fn compatible_windows_executable(app: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null_mut, read_unaligned};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    let path: Vec<u16> = app.as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
        if size == 0 {
            return false;
        }
        let mut bytes = vec![0_u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, bytes.as_mut_ptr().cast()) == 0 {
            return false;
        }
        let (mut value, mut len) = (null_mut(), 0);
        if VerQueryValueW(
            bytes.as_ptr().cast(),
            [b'\\' as u16, 0].as_ptr(),
            &mut value,
            &mut len,
        ) == 0
            || value.is_null()
            || (len as usize) < size_of::<VS_FIXEDFILEINFO>()
        {
            return false;
        }
        let info = read_unaligned(value.cast::<VS_FIXEDFILEINFO>());
        info.dwSignature == 0xFEEF04BD
            && info.dwFileVersionMS >> 16 == 2
            && info.dwFileVersionMS & 0xffff >= 12
    }
}

#[cfg(windows)]
fn windows_executable(dir: &Path, installed: &mut bool) -> Option<PathBuf> {
    if !dir.is_absolute() {
        return None;
    }
    let app = dir.join("oneclient_app.exe");
    if !app.is_file() {
        *installed |= dir.join("OneClient.exe").is_file();
        return None;
    }
    *installed = true;
    compatible_windows_executable(&app).then_some(app)
}

#[cfg(windows)]
fn installed_windows(managed_dir: &Path) -> io::Result<Option<PathBuf>> {
    let (app, mut registered) = registered_windows()?;
    let app = app.or_else(|| {
        dirs::data_local_dir()
            .into_iter()
            .chain(
                ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                    .into_iter()
                    .filter_map(std::env::var_os)
                    .map(PathBuf::from),
            )
            .map(|dir| dir.join("OneClient"))
            .chain([managed_dir.to_path_buf()])
            .find_map(|dir| windows_executable(&dir, &mut registered))
    });
    if app.is_none() && registered {
        return Err(io::Error::other(
            "OneClient is already installed without a compatible executable; update to OneClient 2.12 or later in the 2.x series before retrying",
        ));
    }
    Ok(app)
}

#[cfg(windows)]
const WINDOWS_PROTOCOL_COMMAND: &str = r"Software\Classes\oneclient\shell\open\command";

#[cfg(any(windows, test))]
fn protocol_executable(command: &str) -> Option<&str> {
    let command = command.trim_start();
    if let Some(quoted) = command.strip_prefix('"') {
        quoted.split_once('"').map(|(path, _)| path)
    } else {
        command.split_ascii_whitespace().next()
    }
}

#[cfg(windows)]
fn registered_protocol(command: &str, registered: &mut bool) -> Option<PathBuf> {
    let app = Path::new(protocol_executable(command)?);
    if !app.is_absolute() || !app.is_file() {
        return None;
    }
    let name = app.file_name()?;
    if name.eq_ignore_ascii_case("oneclient_app.exe") {
        *registered = true;
        return compatible_windows_executable(app).then(|| app.to_path_buf());
    }
    *registered |= name.eq_ignore_ascii_case("OneClient.exe");
    None
}

#[cfg(windows)]
fn registered_windows() -> io::Result<(Option<PathBuf>, bool)> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;
    const PATH_NOT_FOUND: i32 = 0x8007_0003_u32 as i32;
    let locations = [
        (WINDOWS_PROTOCOL_COMMAND, ""),
        (
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\OneClient",
            "InstallLocation",
        ),
        (r"Software\Polyfrost Inc.\OneClient", ""),
    ];
    let views = [CURRENT_USER, LOCAL_MACHINE]
        .into_iter()
        .flat_map(|hive| [KEY_WOW64_64KEY, KEY_WOW64_32KEY].map(|view| (hive, view)));
    let mut registered = false;
    for (hive, view, subkey, value) in
        views.flat_map(|(hive, view)| locations.map(|(subkey, value)| (hive, view, subkey, value)))
    {
        let key = match hive.options().read().access(view).open(subkey) {
            Ok(key) => key,
            Err(e) if matches!(e.code().0, FILE_NOT_FOUND | PATH_NOT_FOUND) => continue,
            Err(e) => return Err(io::Error::other(format!("Cannot read {subkey}: {e}"))),
        };
        let protocol = subkey == WINDOWS_PROTOCOL_COMMAND;
        registered |= !protocol;
        let dir = match key.get_string(value) {
            Ok(dir) => dir,
            Err(e) if e.code().0 == FILE_NOT_FOUND => continue,
            Err(e) => return Err(io::Error::other(format!("Cannot read {subkey}: {e}"))),
        };
        let app = if protocol {
            registered_protocol(&dir, &mut registered)
        } else {
            windows_executable(Path::new(dir.trim_matches('"')), &mut registered)
        };
        if let Some(app) = app {
            return Ok((Some(app), true));
        }
    }
    Ok((None, registered))
}

#[cfg(windows)]
fn unpack(archive: &Path, _: &Path, dest: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    // NSIS requires the unquoted /D= argument last, even when the path has spaces.
    let mut directory = std::ffi::OsString::from("/D=");
    directory.push(dest);
    run(Command::new(archive).arg("/S").raw_arg(directory))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn unpack(_: &Path, _: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::other(
        "OneClient is not available for this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_oneclient_process_images() {
        for command in [
            "oneclient_app",
            "/Applications/OneClient.app/Contents/MacOS/oneclient_app",
            "OneClient_2.12.0_amd64.AppImage",
            "OneClient.AppIm",
            "oneclient_app_2.12.0_x86_64.AppImage",
            "oneclient_app_2",
        ] {
            assert!(is_oneclient_process(command, false), "{command}");
        }
        assert!(is_oneclient_process(r#""oneclient_app.exe","123","Console""#, true));
        assert!(is_oneclient_process(r#""ONECLIENT.EXE","123","Console""#, true));
        assert!(!is_oneclient_process(r#""not-OneClient.exe","123","Console""#, true));
        assert!(!is_oneclient_process("oneclient_app-helper", false));
    }

    #[test]
    fn protocol_registration_selects_the_executable_not_its_arguments() {
        assert_eq!(
            protocol_executable(r#""D:\Custom Apps\OneClient\oneclient_app.exe" "%1""#),
            Some(r"D:\Custom Apps\OneClient\oneclient_app.exe")
        );
        assert_eq!(
            protocol_executable(r"D:\OneClient\oneclient_app.exe %1"),
            Some(r"D:\OneClient\oneclient_app.exe")
        );
        assert_eq!(protocol_executable(r#""D:\Custom Apps\unfinished"#), None);
    }

    #[test]
    fn modified_download_is_rejected() {
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        check(&b"abc"[..], digest).unwrap();
        assert!(check(&b"abd"[..], digest).is_err());
    }

    #[test]
    fn data_dir_follows_oneclient_settings_or_refuses_an_unstable_legacy_root() {
        let tmp = tempfile::tempdir().unwrap();
        let standard = tmp.path().join("Polyfrost/OneClient");
        let legacy = tmp.path().join("legacy");
        let custom = tmp.path().join("custom");
        assert_eq!(
            resolve_data_dir(standard.clone(), &legacy).unwrap(),
            (standard.clone(), None)
        );

        fs::create_dir_all(&standard).unwrap();
        fs::write(
            standard.join("settings.json"),
            serde_json::json!({ "data_dir": custom, "previous_data_dir": "relative" }).to_string(),
        )
        .unwrap();
        assert!(resolve_data_dir(standard.clone(), &legacy).is_err());

        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), "{}").unwrap();
        assert!(resolve_data_dir(standard.clone(), &legacy).is_err());
        let legacy_custom = tmp.path().join("legacy-custom");
        fs::create_dir_all(&legacy_custom).unwrap();
        fs::write(
            legacy.join("settings.json"),
            serde_json::json!({ "data_dir": legacy_custom }).to_string(),
        )
        .unwrap();
        assert_eq!(
            resolve_data_dir(standard.clone(), &legacy).unwrap(),
            (legacy_custom.clone(), None)
        );
        fs::write(legacy_custom.join(".oneclient_migrating"), []).unwrap();
        assert!(resolve_data_dir(standard, &legacy).is_err());
    }
}
