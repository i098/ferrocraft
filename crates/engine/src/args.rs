//! Vanilla launch arguments, as the boot jar passes them.

use ferrocraft_net_client::Auth;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct Args {
    pub username: String,
    /// Launcher token and `--uuid`, for online-mode servers.
    pub auth: Option<Auth>,
    pub assets_dir: Option<PathBuf>,
    /// The folder that holds `--assetsDir` (`assets/indexes` and `assets/objects` sit under it).
    pub assets_root: Option<PathBuf>,
    /// `--server`/`--port`/`FERROCRAFT_SERVER`: join this server at once; without it the title screen opens.
    pub server: Option<String>,
    /// `--gameDir`, else `~/.minecraft`: holds `options.txt` and `servers.dat`; a fallback place for `assets/` after `assets_root`.
    pub game_dir: PathBuf,
    /// `--width` and `--height`: the initial window size.
    pub width: u32,
    pub height: u32,
    /// `--fullscreen`.
    pub fullscreen: bool,
}

impl Default for Args {
    /// Vanilla defaults: `Main` declares `--width` 854 and `--height` 480.
    fn default() -> Self {
        Args {
            username: "Player".to_string(),
            auth: None,
            assets_dir: None,
            assets_root: None,
            server: None,
            game_dir: default_game_dir(),
            width: 854,
            height: 480,
            fullscreen: false,
        }
    }
}

pub fn launch_token(
    args: &[String],
    input: impl Read,
    fallback: Option<String>,
) -> Result<Option<String>, String> {
    reject_token_argument(args)?;
    if !args.iter().any(|arg| arg == "--access-token-stdin") {
        return Ok(fallback);
    }
    let mut token = String::new();
    input
        .take(8193)
        .read_to_string(&mut token)
        .map_err(|_| "could not read the launcher token".to_string())?;
    if token.len() > 8192 {
        return Err("launcher token exceeds 8192 bytes".into());
    }
    Ok(Some(token))
}

fn reject_token_argument(args: &[String]) -> Result<(), String> {
    if args
        .iter()
        .any(|arg| arg == "--accessToken" || arg.starts_with("--accessToken="))
    {
        return Err("pass the access token through stdin, not argv".into());
    }
    Ok(())
}

/// `env_server` is the `FERROCRAFT_SERVER` fallback. `--server` accepts `host` or `host:port`.
pub fn parse(
    args: &[String],
    env_server: Option<String>,
    token: Option<String>,
) -> Result<Args, String> {
    let d = Args::default();
    let mut username = d.username;
    let mut assets_dir = None;
    let mut game_dir = None;
    let mut version = None;
    let mut uuid = None;
    let mut host = None;
    let mut port = None;
    let (mut width, mut height, mut fullscreen) = (d.width, d.height, d.fullscreen);
    let mut it = args.iter().peekable();
    while let Some(flag) = it.next() {
        if !flag.starts_with("--") {
            continue;
        }
        let val = it.next_if(|v| !v.starts_with("--"));
        let need = || val.ok_or(format!("{flag} needs a value"));
        // `OptionParser` rejects a non-integer; `Minecraft.<init>` turns a size below 1 into 1.
        let size = |v: &String| {
            v.parse::<i32>()
                .map(|n| n.max(1) as u32)
                .map_err(|e| format!("{flag}: {e}"))
        };
        match flag.as_str() {
            "--username" => username = need()?.clone(),
            "--uuid" => uuid = Some(need()?.clone()),
            "--assetsDir" => assets_dir = Some(PathBuf::from(need()?)),
            "--gameDir" => game_dir = Some(PathBuf::from(need()?)),
            "--version" => version = Some(need()?.clone()),
            "--server" => host = Some(need()?.clone()),
            "--port" => port = Some(need()?.parse::<u16>().map_err(|e| format!("--port: {e}"))?),
            "--width" => width = size(need()?)?,
            "--height" => height = size(need()?)?,
            "--fullscreen" => fullscreen = true,
            _ => {}
        }
    }
    let server = server_address(host.or(env_server), port);
    let game_dir = game_dir.clone().unwrap_or_else(default_game_dir);
    Ok(Args {
        username,
        auth: token
            .zip(uuid)
            .filter(|(t, _)| !t.is_empty())
            .map(|(t, u)| Auth::new(t, &u)),
        assets_root: assets_dir
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf),
        assets_dir: resolve_assets(assets_dir, Some(&game_dir), version.as_deref()),
        server,
        game_dir,
        width,
        height,
        fullscreen,
    })
}

fn default_game_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("APPDATA"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".minecraft")
}

fn server_address(host: Option<String>, port: Option<u16>) -> Option<String> {
    match (host, port) {
        (Some(h), Some(p)) => Some(format!("{}:{p}", h.split(':').next().unwrap_or(&h))),
        (Some(h), None) if h.contains(':') => Some(h),
        (Some(h), None) => Some(format!("{h}:25565")),
        (None, Some(p)) => Some(format!("127.0.0.1:{p}")),
        (None, None) => None,
    }
}

/// Launchers pass `<minecraft-dir>/assets`, which holds hashed objects, not the textures and
/// font. The renderer reads those from the vanilla client jar, `versions/1.8.9/1.8.9.jar` (or
/// `versions/<--version>/<--version>.jar`) under the assets dir's parent, `--gameDir` or its parent.
fn resolve_assets(
    assets: Option<PathBuf>,
    game_dir: Option<&Path>,
    version: Option<&str>,
) -> Option<PathBuf> {
    if assets.as_deref().is_some_and(Path::is_file) {
        return assets;
    }
    let roots = [
        assets.as_deref().and_then(Path::parent),
        game_dir,
        game_dir.and_then(Path::parent),
    ];
    for root in roots.into_iter().flatten() {
        for v in ["1.8.9"].into_iter().chain(version) {
            let jar = root.join("versions").join(v).join(format!("{v}.jar"));
            if jar.is_file() {
                return Some(jar);
            }
        }
    }
    assets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[String], env_server: Option<String>) -> Result<Args, String> {
        super::parse(args, env_server, None)
    }

    fn v(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn vanilla_args_and_server_forms() {
        let a = parse(
            &v("--username Bob --version 1.8.9 --gameDir /g --assetsDir /a"),
            None,
        )
        .unwrap();
        assert_eq!(a.username, "Bob");
        assert_eq!(a.assets_dir, Some(PathBuf::from("/a")));
        // The language list reads `pack.mcmeta` from the index under `--assetsDir`, not `--gameDir`.
        let a = parse(&v("--gameDir /g --assetsDir /x/mc/assets"), None).unwrap();
        assert_eq!(a.assets_root, Some(PathBuf::from("/x/mc")));
        let mc = std::env::temp_dir().join(format!("fc-args-{}", std::process::id()));
        std::fs::create_dir_all(mc.join("assets")).unwrap();
        std::fs::create_dir_all(mc.join("versions/1.8.9")).unwrap();
        std::fs::write(mc.join("versions/1.8.9/1.8.9.jar"), b"").unwrap();
        let jar = mc.join("versions/1.8.9/1.8.9.jar");
        let flag = format!("--assetsDir {}", mc.join("assets").display());
        assert_eq!(
            parse(&v(&flag), None).unwrap().assets_dir,
            Some(jar.clone())
        );
        let flag = format!("--gameDir {} --assetsDir /elsewhere/assets", mc.display());
        assert_eq!(
            parse(&v(&flag), None).unwrap().assets_dir,
            Some(jar.clone())
        );
        let own = mc.join("own.jar");
        std::fs::write(&own, b"").unwrap();
        let flag = format!("--gameDir {} --assetsDir {}", mc.display(), own.display());
        assert_eq!(parse(&v(&flag), None).unwrap().assets_dir, Some(own));
        std::fs::remove_dir_all(mc).unwrap();
        let a = parse(&v("--fullscreen --username Foo --demo"), None).unwrap();
        assert_eq!(a.username, "Foo");
        assert_eq!(a.server, None);
        assert!(parse(&v("--username"), None).is_err());
        assert_eq!(
            parse(&v("--server mc.local"), None).unwrap().server,
            Some("mc.local:25565".into())
        );
        assert_eq!(
            parse(&v("--server mc.local:1 --port 2"), None)
                .unwrap()
                .server,
            Some("mc.local:2".into())
        );
        assert_eq!(
            parse(&[], Some("h:9".into())).unwrap().server,
            Some("h:9".into())
        );
        assert!(parse(&v("--port x"), None).is_err());
        assert_eq!(parse(&v("--width 0 --height -5"), None).unwrap().width, 1);
        assert_eq!(parse(&v("--width 0 --height -5"), None).unwrap().height, 1);
    }

    #[test]
    fn window_args_values_defaults_and_bad_values() {
        let d = parse(&v("--username A --server 127.0.0.1"), None).unwrap();
        assert_eq!((d.width, d.height, d.fullscreen), (854, 480, false));
        let w = parse(
            &v("--username A --width 1280 --height 720 --fullscreen"),
            None,
        )
        .unwrap();
        assert_eq!((w.width, w.height, w.fullscreen), (1280, 720, true));
        assert_eq!(parse(&v("--width 640"), None).unwrap().height, 480);
        for bad in ["--width x", "--width", "--height 1.5"] {
            assert!(parse(&v(bad), None).is_err(), "{bad}");
        }
    }

    #[test]
    fn auth_needs_token_and_uuid_and_never_prints_the_token() {
        let argv = v("--access-token-stdin --uuid 12345678-1234-1234-1234-123456789abc");
        let token = launch_token(&argv, &b"sekrit"[..], Some("fallback".into())).unwrap();
        let a = super::parse(&argv, None, token).unwrap();
        assert_eq!(
            a.auth,
            Some(Auth::new(
                "sekrit".into(),
                "12345678123412341234123456789abc"
            ))
        );
        assert!(!format!("{a:?}").contains("sekrit"));
        assert_eq!(parse(&v("--uuid abc"), None).unwrap().auth, None);
        assert!(reject_token_argument(&v("--accessToken sekrit --uuid abc")).is_err());
        assert!(reject_token_argument(&v("--accessToken=sekrit --uuid abc")).is_err());
    }
}
