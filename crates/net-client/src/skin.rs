//! Player skins: the `textures` profile property, the download and the on-disk cache.

use crate::screens::base64;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;

/// Largest skin download kept; a real skin is under 20 KB.
const MAX_BYTES: u64 = 1 << 20;

/// Where a profile's skin lives and which arm model it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinInfo {
    pub url: String,
    pub slim: bool,
}

/// Decodes the base64 JSON of a `textures` property: `textures.SKIN.url` and `metadata.model`.
/// Only hosts that `SkinManager` trusts (`.minecraft.net`, `.mojang.com`) are accepted. The host in
/// `FERROCRAFT_SKIN_HOST` is accepted too, for the local test server.
pub fn parse_textures(value: &str) -> Option<SkinInfo> {
    let json: Value = serde_json::from_slice(&base64(value)?).ok()?;
    let skin = json.get("textures")?.get("SKIN")?;
    let url = skin.get("url")?.as_str()?.to_string();
    if !trusted(&url) {
        return None;
    }
    let slim = skin.pointer("/metadata/model").and_then(Value::as_str) == Some("slim");
    Some(SkinInfo { url, slim })
}

fn trusted(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.contains('@') {
        return false;
    }
    let host = host.split(':').next().unwrap_or("");
    host.ends_with(".minecraft.net")
        || host.ends_with(".mojang.com")
        || std::env::var("FERROCRAFT_SKIN_HOST").is_ok_and(|h| h == host)
}

/// The PNG bytes of `url`, from `cache_dir` when present, else downloaded and stored there.
pub fn fetch(url: &str, cache_dir: &Path) -> Option<Vec<u8>> {
    let hash: String = Sha1::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let file = cache_dir.join(format!("{hash}.png"));
    if let Ok(bytes) = std::fs::read(&file) {
        return Some(bytes);
    }
    let mut bytes = Vec::new();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .ok()?
        .into_body()
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if std::fs::create_dir_all(cache_dir).is_ok() {
        let _ = std::fs::write(&file, &bytes);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in s.as_bytes().chunks(3) {
            let n = c
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn textures_property_gives_url_and_model() {
        let url = "http://textures.minecraft.net/texture/abc";
        let plain = format!(r#"{{"textures":{{"SKIN":{{"url":"{url}"}}}}}}"#);
        let slim =
            format!(r#"{{"textures":{{"SKIN":{{"url":"{url}","metadata":{{"model":"slim"}}}}}}}}"#);
        let info = |j: &str| parse_textures(&b64(j)).unwrap();
        assert_eq!(
            info(&plain),
            SkinInfo {
                url: url.into(),
                slim: false
            }
        );
        assert!(info(&slim).slim);
    }

    #[test]
    fn untrusted_host_is_refused() {
        for url in [
            "http://evil.example/texture/abc",
            "http://textures.minecraft.net.evil.example/x",
            "http://textures.minecraft.net@evil.example/x",
        ] {
            let j = format!(r#"{{"textures":{{"SKIN":{{"url":"{url}"}}}}}}"#);
            assert_eq!(parse_textures(&b64(&j)), None, "{url}");
        }
    }
}
