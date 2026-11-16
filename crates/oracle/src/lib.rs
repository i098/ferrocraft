//! Ground truth for differential tests: the real Minecraft 1.8.9 client classes, run headless.
//!
//! [`Oracle::get`] downloads the client jar and its libraries from Mojang's public version
//! manifest (no login), remaps the jar with Ornithe Calamus + Feather mappings, compiles
//! `java/oracle/Harness.java` against it, and caches everything under `target/oracle`
//! (override with `FERROCRAFT_ORACLE_DIR`). [`Oracle::run`] feeds the harness a script and
//! returns its output lines. Needs a JDK (`javac` and `java`) from `JAVA_HOME` or `PATH`.
//!
//! The remapped jar puts every class in one package (`net.minecraft.flat`). The jar was
//! obfuscated in one flat package, and package-private access breaks once Feather spreads the
//! classes over many packages. Simple names that collide keep their full path joined by `_`.

use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use sha1::{Digest, Sha1};

pub type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const VERSION: &str = "1.8.9";
const FLAT_PACKAGE: &str = "net/minecraft/flat/";

/// A pinned download whose SHA-1 is checked.
struct Pin {
    file: &'static str,
    url: &'static str,
    sha1: &'static str,
}

const TINY_REMAPPER: Pin = Pin {
    file: "tiny-remapper.jar",
    url:
        "https://maven.fabricmc.net/net/fabricmc/tiny-remapper/0.15.0/tiny-remapper-0.15.0-fat.jar",
    sha1: "062bd5cfe4874458b89b2fb5d7ccd09cce34df6e",
};
const CALAMUS: Pin = Pin {
    file: "calamus.jar",
    url: "https://maven.ornithemc.net/releases/net/ornithemc/calamus-intermediary-gen2/1.8.9/calamus-intermediary-gen2-1.8.9.jar",
    sha1: "0ba81322efe0c8515a4ab246dae65a5c59c20742",
};
const FEATHER: Pin = Pin {
    file: "feather.jar",
    url: "https://maven.ornithemc.net/releases/net/ornithemc/feather-gen2/1.8.9+build.2/feather-gen2-1.8.9+build.2-v2.jar",
    sha1: "e20b85bdb6899ed94ca2e7cb9579499fd077085b",
};

/// A prepared oracle: remapped jar, libraries and compiled harness.
pub struct Oracle {
    dir: PathBuf,
    java: PathBuf,
    classpath: OsString,
    session: Mutex<Option<Session>>,
}

/// One tick of harness output. Doubles are kept as raw bits so tests compare exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickState {
    pub pos: [u64; 3],
    pub vel: [u64; 3],
    pub on_ground: bool,
}

impl TickState {
    pub fn parse(line: &str) -> Result<TickState> {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 7 {
            return Err(format!("bad harness line: {line:?}").into());
        }
        let bits = |s: &str| u64::from_str_radix(s, 16);
        Ok(TickState {
            pos: [bits(f[0])?, bits(f[1])?, bits(f[2])?],
            vel: [bits(f[3])?, bits(f[4])?, bits(f[5])?],
            on_ground: f[6] == "1",
        })
    }
}

impl Oracle {
    /// Prepare the oracle once per process; later calls return the same result.
    pub fn get() -> &'static Oracle {
        static ORACLE: LazyLock<Result<Oracle, String>> =
            LazyLock::new(|| Oracle::prepare().map_err(|e| e.to_string()));
        ORACLE
            .as_ref()
            .unwrap_or_else(|e| panic!("oracle setup failed: {e}"))
    }

    fn prepare() -> Result<Oracle> {
        let dir = match std::env::var_os("FERROCRAFT_ORACLE_DIR") {
            Some(d) => PathBuf::from(d),
            None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/oracle"),
        };
        fs::create_dir_all(&dir)?;
        let java = tool("java")?;
        let javac = tool("javac")?;

        let remapper = fetch_pinned(&dir, &TINY_REMAPPER)?;
        let calamus = fetch_pinned(&dir, &CALAMUS)?;
        let feather = fetch_pinned(&dir, &FEATHER)?;

        let libs_dir = dir.join("libs");
        fs::create_dir_all(&libs_dir)?;
        let (client_pin, libs) = version_files()?;
        let client = fetch(&dir.join("client.jar"), &client_pin.0, Some(&client_pin.1))?;
        let mut cp = Vec::new();
        for (name, url, sha) in &libs {
            cp.push(fetch(&libs_dir.join(name), url, Some(sha))?);
        }

        // Not `named.jar`: the remapper wrote a directory there (see `tmp` below), and CI
        // caches restored it as an empty tree, which hid the missing classes.
        let named = dir.join("mapped.jar");
        if !named.exists() {
            let inter = dir.join("inter.jar");
            let calamus_tiny = extract_mappings(&calamus, &dir.join("calamus.tiny"), false)?;
            let feather_tiny = extract_mappings(&feather, &dir.join("feather.tiny"), true)?;
            remap(
                &java,
                &remapper,
                &client,
                &inter,
                &calamus_tiny,
                "official",
                "intermediary",
            )?;
            // The remapper writes a directory unless the output path ends in `.jar`.
            let tmp = dir.join("mapped.tmp.jar");
            remap(
                &java,
                &remapper,
                &inter,
                &tmp,
                &feather_tiny,
                "intermediary",
                "named",
            )?;
            fs::rename(tmp, &named)?;
        }

        cp.insert(0, named);
        // The harness reads class files with the ASM that the remapper jar bundles.
        cp.push(remapper.clone());
        let classes = dir.join("classes");
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../java/oracle/Harness.java");
        let class = classes.join("Harness.class");
        if !newer(&class, &src) {
            fs::create_dir_all(&classes)?;
            let out = Command::new(javac)
                .arg("-nowarn")
                .arg("-cp")
                .arg(std::env::join_paths(&cp)?)
                .arg("-d")
                .arg(&classes)
                .arg(&src)
                .output()?;
            check("javac", &out)?;
        }
        cp.insert(0, classes);
        Ok(Oracle {
            dir,
            java,
            classpath: std::env::join_paths(&cp)?,
            session: Mutex::new(None),
        })
    }

    /// Run the harness on `script` (see `java/oracle/Harness.java`) and return its output lines.
    ///
    /// One JVM serves every call of the process; calls take turns. A crash, a harness error
    /// or a call that takes longer than [`CALL_TIMEOUT`] returns an error and drops the JVM,
    /// so the next call boots a new one.
    pub fn run(&self, script: &str) -> Result<Vec<String>> {
        let mut slot = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(self.spawn()?);
        }
        let result = slot.as_mut().map(|s| s.call(script));
        match result {
            Some(Ok(lines)) => Ok(lines),
            Some(Err(e)) => {
                if let Some(mut dead) = slot.take() {
                    let _ = dead.child.kill();
                    let _ = dead.child.wait();
                }
                Err(e)
            }
            None => unreachable!("session was just filled"),
        }
    }

    fn spawn(&self) -> Result<Session> {
        // C1 only and the serial GC cut the CPU time of the one JVM boot.
        let mut child = Command::new(&self.java)
            .args(["-XX:TieredStopAtLevel=1", "-XX:+UseSerialGC"])
            .arg("-cp")
            .arg(&self.classpath)
            .arg("Harness")
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut stderr = child.stderr.take().ok_or("no stderr")?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        // Drain stderr so a chatty JVM never blocks on a full pipe; keep the tail for errors.
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = stderr.read(&mut buf) {
                let mut log = sink.lock().unwrap_or_else(|e| e.into_inner());
                log.extend_from_slice(&buf[..n]);
                let excess = log.len().saturating_sub(STDERR_TAIL);
                log.drain(..excess);
            }
        });
        Ok(Session {
            child,
            stdin,
            lines,
            log,
        })
    }
}

/// Longest wait for one harness call. Boot, a call and a loaded host all fit well inside it.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const STDERR_TAIL: usize = 16 * 1024;

/// A running harness JVM.
struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<std::io::Result<String>>,
    log: Arc<Mutex<Vec<u8>>>,
}

impl Session {
    fn call(&mut self, script: &str) -> Result<Vec<String>> {
        self.stdin
            .write_all(script.as_bytes())
            .and_then(|()| {
                let end: &[u8] = if script.ends_with('\n') {
                    b"%%run\n"
                } else {
                    b"\n%%run\n"
                };
                self.stdin.write_all(end)
            })
            .and_then(|()| self.stdin.flush())
            .map_err(|e| self.fail(format!("cannot send the script: {e}")))?;
        let deadline = Instant::now() + CALL_TIMEOUT;
        let mut out = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(Ok(line)) if line == "%%done" => return Ok(out),
                Ok(Ok(line)) => match line.strip_prefix("%%error ") {
                    Some(hex) => {
                        return Err(self.fail(format!("harness error:\n{}", unhex(hex))).into())
                    }
                    None => out.push(line),
                },
                Ok(Err(e)) => return Err(self.fail(format!("cannot read output: {e}")).into()),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(self
                        .fail(format!(
                            "no answer within {}s (hung)",
                            CALL_TIMEOUT.as_secs()
                        ))
                        .into())
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let status = self
                        .child
                        .wait()
                        .map_or_else(|e| e.to_string(), |s| s.to_string());
                    return Err(self.fail(format!("JVM exited ({status})")).into());
                }
            }
        }
    }

    fn fail(&self, what: String) -> String {
        std::thread::sleep(Duration::from_millis(50)); // let the stderr thread catch up
        let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "oracle harness: {what}\nstderr:\n{}",
            String::from_utf8_lossy(&log)
        )
    }
}

fn unhex(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn newer(out: &Path, src: &Path) -> bool {
    match (
        fs::metadata(out).and_then(|m| m.modified()),
        fs::metadata(src).and_then(|m| m.modified()),
    ) {
        (Ok(o), Ok(s)) => o >= s,
        _ => false,
    }
}

fn check(what: &str, out: &std::process::Output) -> Result<()> {
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "{what} failed ({}):\n{}{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .into())
}

/// Find `java` or `javac` in `JAVA_HOME/bin`, then on `PATH`.
fn tool(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::var_os("JAVA_HOME")
        .map(|h| Path::new(&h).join("bin").join(name))
        .into_iter()
        .chain(std::env::split_paths(&path).map(|p| p.join(name)))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("`{name}` not found: set JAVA_HOME to a JDK (8 or newer)").into())
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    Ok(ureq::get(url).call()?.body_mut().read_to_vec()?)
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Download `url` to `path` unless the file is there; verify the SHA-1 when given.
fn fetch(path: &Path, url: &str, sha1: Option<&str>) -> Result<PathBuf> {
    if !path.exists() {
        let bytes = http_get(url)?;
        if let Some(want) = sha1 {
            let got = sha1_hex(&bytes);
            if got != want {
                return Err(format!("{url}: sha1 {got}, expected {want}").into());
            }
        }
        let tmp = path.with_extension("part");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, path)?;
    }
    Ok(path.to_owned())
}

fn fetch_pinned(dir: &Path, pin: &Pin) -> Result<PathBuf> {
    fetch(&dir.join(pin.file), pin.url, Some(pin.sha1))
}

type Download = (String, String);
type Library = (String, String, String);

/// Client jar `(url, sha1)` and library `(file, url, sha1)` list from Mojang's manifest.
/// Libraries that need natives or OS rules are skipped; headless physics does not touch them.
fn version_files() -> Result<(Download, Vec<Library>)> {
    let manifest: serde_json::Value = serde_json::from_slice(&http_get(MANIFEST)?)?;
    let entry = manifest["versions"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v["id"] == VERSION))
        .ok_or("1.8.9 missing from the version manifest")?;
    let version: serde_json::Value =
        serde_json::from_slice(&http_get(entry["url"].as_str().ok_or("no url")?)?)?;
    let text = |v: &serde_json::Value| {
        v.as_str()
            .map(str::to_owned)
            .ok_or("missing string in version json")
    };
    let client = &version["downloads"]["client"];
    let client = (text(&client["url"])?, text(&client["sha1"])?);
    let mut libs = Vec::new();
    for lib in version["libraries"].as_array().ok_or("no libraries")? {
        let art = &lib["downloads"]["artifact"];
        if lib.get("rules").is_some() || lib.get("natives").is_some() || art.is_null() {
            continue;
        }
        let url = text(&art["url"])?;
        let name = url.rsplit('/').next().unwrap_or_default().to_owned();
        libs.push((name, url, text(&art["sha1"])?));
    }
    Ok((client, libs))
}

/// Pull `mappings/mappings.tiny` out of a mappings jar; optionally flatten the named packages.
fn extract_mappings(jar: &Path, out: &Path, flatten: bool) -> Result<PathBuf> {
    if !out.exists() {
        let mut zip = zip::ZipArchive::new(Cursor::new(fs::read(jar)?))?;
        let mut text = String::new();
        zip.by_name("mappings/mappings.tiny")?
            .read_to_string(&mut text)?;
        if flatten {
            text = flatten_packages(&text);
        }
        let tmp = out.with_extension("part");
        fs::write(&tmp, text)?;
        fs::rename(tmp, out)?;
    }
    Ok(out.to_owned())
}

/// Rewrite the second-namespace class names of a tiny v2 file into one package.
/// Class lines are `c<TAB>from<TAB>to`; member lines are indented and refer to `from` only.
fn flatten_packages(tiny: &str) -> String {
    let simple = |name: &str| name.rsplit('/').next().unwrap_or(name).to_owned();
    let mut count: HashMap<String, usize> = HashMap::new();
    for line in tiny.lines().filter(|l| l.starts_with("c\t")) {
        if let Some(to) = line.split('\t').nth(2) {
            *count.entry(simple(to)).or_default() += 1;
        }
    }
    let mut out = String::with_capacity(tiny.len());
    for line in tiny.lines() {
        let mut fields: Vec<&str> = line.split('\t').collect();
        let flat;
        if fields[0] == "c" && fields.len() >= 3 {
            let to = fields[2];
            let name = if count[&simple(to)] > 1 {
                to.replace('/', "_")
            } else {
                simple(to)
            };
            flat = format!("{FLAT_PACKAGE}{name}");
            fields[2] = &flat;
        }
        out.push_str(&fields.join("\t"));
        out.push('\n');
    }
    out
}

fn remap(
    java: &Path,
    remapper: &Path,
    input: &Path,
    output: &Path,
    tiny: &Path,
    from: &str,
    to: &str,
) -> Result<()> {
    let out = Command::new(java)
        .arg("-jar")
        .arg(remapper)
        .args([input, output, tiny])
        .args([from, to])
        .output()?;
    check("tiny-remapper", &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_keeps_members_and_disambiguates_collisions() {
        let tiny = "tiny\t2\t0\tintermediary\tnamed\n\
                    c\ta\tx/y/Foo\n\
                    \tm\t()V\tm_1\ttick\n\
                    c\tb\tx/z/Box\n\
                    c\tc\tx/w/Box\n";
        let got = flatten_packages(tiny);
        let want = "tiny\t2\t0\tintermediary\tnamed\n\
                    c\ta\tnet/minecraft/flat/Foo\n\
                    \tm\t()V\tm_1\ttick\n\
                    c\tb\tnet/minecraft/flat/x_z_Box\n\
                    c\tc\tnet/minecraft/flat/x_w_Box\n";
        assert_eq!(got, want);
    }

    #[test]
    fn a_bad_script_fails_with_the_java_error_and_the_next_call_still_works() {
        let oracle = Oracle::get();
        let err = oracle.run("nonsense\n").unwrap_err().to_string();
        assert!(err.contains("IllegalArgumentException"), "{err}");
        assert_eq!(oracle.run("props 0\n").unwrap().len(), 1);
    }

    #[test]
    fn parses_tick_state() {
        let s = TickState::parse("3fe0000000000000 4050000000000000 0000000000000000 0000000000000000 bfb41205c28f5c29 0000000000000000 1").unwrap();
        assert_eq!(f64::from_bits(s.pos[0]), 0.5);
        assert_eq!(f64::from_bits(s.pos[1]), 64.0);
        assert!(s.on_ground);
    }
}
