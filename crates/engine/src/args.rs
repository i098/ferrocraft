//! Vanilla launch arguments, as the boot jar passes them.

use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Args {
    pub username: String,
    pub assets_dir: Option<PathBuf>,
    pub server: String,
}

const DEFAULT_SERVER: &str = "127.0.0.1:25565";

/// Parse `--username`, `--assetsDir`, `--server` and `--port`; other vanilla flags
/// (`--gameDir`, `--version`, `--accessToken`, ...) take one value and are ignored.
/// `env_server` is the `FERROCRAFT_SERVER` fallback. `--server` accepts `host` or `host:port`.
pub fn parse(args: &[String], env_server: Option<String>) -> Result<Args, String> {
    let mut username = "Player".to_string();
    let mut assets_dir = None;
    let mut host = None;
    let mut port = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if !flag.starts_with("--") {
            continue;
        }
        let val = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--username" => username = val.clone(),
            "--assetsDir" => assets_dir = Some(PathBuf::from(val)),
            "--server" => host = Some(val.clone()),
            "--port" => port = Some(val.parse::<u16>().map_err(|e| format!("--port: {e}"))?),
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
        assets_dir,
        server,
    })
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
        assert_eq!(a.server, DEFAULT_SERVER);
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
