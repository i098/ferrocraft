//! Vanilla launch arguments, as the boot jar passes them.

use ferrocraft_net_client::Auth;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct Args {
    pub username: String,
    /// `--accessToken` (or `FERROCRAFT_ACCESS_TOKEN`) with `--uuid`: needed for online-mode servers.
    pub auth: Option<Auth>,
    pub assets_dir: Option<PathBuf>,
    /// `--server`/`--port`/`FERROCRAFT_SERVER`: join this server at once; without it the title screen opens.
    pub server: Option<String>,
    /// `--gameDir`, else `~/.minecraft`: holds `options.txt`, `servers.dat` and the sound assets.
    pub game_dir: PathBuf,
}

/// Parse `--username`, `--uuid`, `--accessToken`, `--assetsDir`, `--gameDir`, `--version`,
/// `--server` and `--port`; other vanilla flags are ignored. The Fabric mod hands the token over
/// in `FERROCRAFT_ACCESS_TOKEN`, so it stays out of the process list. A flag takes the next token as its value
/// unless that token is another `--flag`, so `--demo` and `--fullscreen` need no value.
/// `env_server` is the `FERROCRAFT_SERVER` fallback. `--server` accepts `host` or `host:port`.
pub fn parse(args: &[String], env_server: Option<String>) -> Result<Args, String> {
    let mut username = "Player".to_string();
    let mut assets_dir = None;
    let mut game_dir = None;
    let mut version = None;
    let mut uuid = None;
    let mut token = std::env::var("FERROCRAFT_ACCESS_TOKEN").ok();
    let mut host = None;
    let mut port = None;
    let mut it = args.iter().peekable();
    while let Some(flag) = it.next() {
        if !flag.starts_with("--") {
            continue;
        }
        let val = it.next_if(|v| !v.starts_with("--"));
        let need = || val.ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--username" => username = need()?.clone(),
            "--uuid" => uuid = Some(need()?.clone()),
            "--accessToken" => token = Some(need()?.clone()),
            "--assetsDir" => assets_dir = Some(PathBuf::from(need()?)),
            "--gameDir" => game_dir = Some(PathBuf::from(need()?)),
            "--version" => version = Some(need()?.clone()),
            "--server" => host = Some(need()?.clone()),
            "--port" => port = Some(need()?.parse::<u16>().map_err(|e| format!("--port: {e}"))?),
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
        assets_dir: resolve_assets(assets_dir, Some(&game_dir), version.as_deref()),
        server,
        game_dir,
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
    }

    #[test]
    fn auth_needs_token_and_uuid_and_never_prints_the_token() {
        let a = parse(
            &v("--accessToken sekrit --uuid 12345678-1234-1234-1234-123456789abc"),
            None,
        )
        .unwrap();
        assert_eq!(
            a.auth,
            Some(Auth::new(
                "sekrit".into(),
                "12345678123412341234123456789abc"
            ))
        );
        assert!(!format!("{a:?}").contains("sekrit"));
        assert_eq!(parse(&v("--uuid abc"), None).unwrap().auth, None);
    }
}
