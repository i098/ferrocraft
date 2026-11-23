//! Install an unmodified OneClient release from Polyfrost.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const VERSION: &str = "2.12.0";

// Digests from https://github.com/Polyfrost/OneLauncher/releases/tag/oneclient-2.12.0.
const fn release_asset(os: &str, arch: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // Match the architecture of the Ferrocraft release, not the host process environment.
    match (os.as_bytes(), arch.as_bytes()) {
        (b"linux", b"x86_64") => Some((
            "linux_x86_64.AppImage",
            "5cdbc52cd1b67a72e08e3eca49001ac7e185b27657722fc286bda53a66d5cf63",
            "OneClient.AppImage",
        )),
        (b"macos", b"aarch64") => Some((
            "darwin_aarch64.app.tar.gz",
            "d1bd1255dfa353dbf3c88bf332d9cf52547d01cef2969ab4181cbf853414925d",
            "OneClient.app/Contents/MacOS/oneclient_app",
        )),
        (b"macos", b"x86_64") => Some((
            "darwin_x86_64.app.tar.gz",
            "e6c5f9d19a2213c9329837ef17219089b7afed3ca328169c83fe25aba4133c02",
            "OneClient.app/Contents/MacOS/oneclient_app",
        )),
        (b"windows", b"x86_64") => Some((
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

pub fn install(base: &Path) -> io::Result<PathBuf> {
    let Some((asset, digest, executable)) =
        release_asset(std::env::consts::OS, std::env::consts::ARCH)
    else {
        return Err(io::Error::other(
            "OneClient is not available for this platform",
        ));
    };
    let dest = base.join(format!("oneclient-{VERSION}"));
    let installed = dest.join(executable);
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
    fn modified_download_is_rejected() {
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        check(&b"abc"[..], digest).unwrap();
        assert!(check(&b"abd"[..], digest).is_err());
    }
}
