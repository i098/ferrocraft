//! Vanilla launch arguments, as the boot jar passes them.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct Args {
    pub username: String,
    pub assets_dir: Option<PathBuf>,
    pub server: String,
}

const DEFAULT_SERVER: &str = "127.0.0.1:25565";

/// Parse `--username`, `--assetsDir`, `--gameDir`, `--version`, `--server` and `--port`; other
/// vanilla flags (`--accessToken`, ...) are ignored. A flag takes the next token as its value
/// unless that token is another `--flag`, so `--demo` and `--fullscreen` need no value.
/// `env_server` is the `FERROCRAFT_SERVER` fallback. `--server` accepts `host` or `host:port`.
pub fn parse(args: &[String], env_server: Option<String>) -> Result<Args, String> {
    let mut username = "Player".to_string();
    let mut assets_dir = None;
    let mut game_dir = None;
    let mut version = None;
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
            "--assetsDir" => assets_dir = Some(PathBuf::from(need()?)),
            "--gameDir" => game_dir = Some(PathBuf::from(need()?)),
            "--version" => version = Some(need()?.clone()),
            "--server" => host = Some(need()?.clone()),
            "--port" => port = Some(need()?.parse::<u16>().map_err(|e| format!("--port: {e}"))?),
            _ => {}
        }
    }
    let server = match (host.or(env_server), port) {
        (Some(h), Some(p)) => format!("{}:{p}", h.split(':').next().unwrap_or(&h)),
        (Some(h), None) if h.contains(':') => h,
        (Some(h), None) => format!("{h}:25565"),
        (None, Some(p)) => format!("127.0.0.1:{p}"),
        (None, None) => DEFAULT_SERVER.to_string(),
    };
    Ok(Args {
        username,
        assets_dir: resolve_assets(assets_dir, game_dir.as_deref(), version.as_deref()),
        server,
    })
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
        assert_eq!(a.server, DEFAULT_SERVER);
        assert!(parse(&v("--username"), None).is_err());
        assert_eq!(
            parse(&v("--server mc.local"), None).unwrap().server,
            "mc.local:25565"
        );
        assert_eq!(
            parse(&v("--server mc.local:1 --port 2"), None)
                .unwrap()
                .server,
            "mc.local:2"
        );
        assert_eq!(parse(&[], Some("h:9".into())).unwrap().server, "h:9");
        assert!(parse(&v("--port x"), None).is_err());
    }
}
