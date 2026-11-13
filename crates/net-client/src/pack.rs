//! Server resource packs (`S48`): the vanilla `NetHandlerPlayClient.handleResourcePack` and
//! `ResourcePackRepository.downloadResourcePack`; the engine asks the player first (see `engine::app::pack`).

use crate::{send, NetEvent, Session};
use ferrocraft_protocol::codec::VarInt;
use ferrocraft_protocol::play::serverbound as sb;
use sha1::{Digest, Sha1};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `ResourcePackRepository`: the largest pack it downloads (50 MiB).
const MAX_BYTES: u64 = 52_428_800;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// `C19PacketResourcePackStatus.Action`; the ordinal is the wire value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackStatus {
    Loaded = 0,
    Declined = 1,
    Failed = 2,
    Accepted = 3,
}

/// Where a pack comes from.
#[derive(Debug, PartialEq, Eq)]
enum Source {
    /// `level://<name>`: a file below the `saves` folder.
    Level(String),
    Http(String),
}

/// Sorts a server's URL; `None` for a URL vanilla refuses: a `level://` path that climbs out of `saves` or
/// is not a `resources.zip`, and any scheme but http and https.
fn source(url: &str) -> Option<Source> {
    if let Some(rest) = url.strip_prefix("level://") {
        let name = percent_decode(rest)?;
        let ok = !name.contains("..") && name.ends_with("/resources.zip") && !name.starts_with('/');
        return ok.then_some(Source::Level(name));
    }
    (url.starts_with("http://") || url.starts_with("https://"))
        .then(|| Source::Http(url.to_owned()))
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A SHA-1 as the server sends it: 40 lowercase hex digits.
fn is_sha1(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `checkHash`: an empty hash is not checked; anything else must be the file's SHA-1.
fn hash_matches(hash: &str, bytes: &[u8]) -> bool {
    hash.is_empty() || (is_sha1(hash) && hash == sha1_hex(bytes))
}

/// The verified pack file: the cached copy named by `hash` when it still matches, else a download of
/// at most 50 MiB stored under `dir`.
fn download(url: &str, hash: &str, dir: &Path) -> io::Result<PathBuf> {
    let file = dir.join(if is_sha1(hash) { hash } else { "legacy" });
    if is_sha1(hash) {
        if let Ok(bytes) = std::fs::read(&file) {
            if hash_matches(hash, &bytes) {
                return Ok(file);
            }
        }
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build()
        .into();
    let mut bytes = Vec::new();
    agent
        .get(url)
        .call()
        .map_err(io::Error::other)?
        .into_body()
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("resource pack over 50 MiB"));
    }
    if !hash_matches(hash, &bytes) {
        return Err(io::Error::other("resource pack hash mismatch"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&file, &bytes)?;
    Ok(file)
}

impl Session {
    /// C19 `ResourcePackStatus`.
    pub fn send_pack_status(&mut self, hash: &str, status: PackStatus) -> io::Result<()> {
        send(&self.writer, status_packet(hash, status))
    }

    /// Answers a [`NetEvent::ResourcePack`]: sends `Accepted` and gets the pack in the background, then a
    /// [`NetEvent::ResourcePackReady`] follows; a refused or failed pack gets `Failed` instead.
    /// `game_dir` holds `saves` (for `level://`) and receives `server-resource-packs`.
    pub fn accept_resource_pack(
        &mut self,
        url: &str,
        hash: &str,
        game_dir: &Path,
    ) -> io::Result<()> {
        let Some(source) = source(url) else {
            return self.send_pack_status(hash, PackStatus::Failed);
        };
        let level = match &source {
            Source::Level(name) => Some(game_dir.join("saves").join(name)),
            Source::Http(_) => None,
        };
        if level.as_ref().is_some_and(|p| !p.is_file()) {
            return self.send_pack_status(hash, PackStatus::Failed);
        }
        self.send_pack_status(hash, PackStatus::Accepted)?;
        let (writer, tx) = (self.writer.clone(), self.tx.clone());
        let (hash, dir) = (hash.to_owned(), game_dir.join("server-resource-packs"));
        std::thread::Builder::new()
            .name("net-client-resource-pack".into())
            .spawn(move || {
                let got = match (level, source) {
                    (Some(path), _) => Ok(path),
                    (None, Source::Http(url)) => download(&url, &hash, &dir),
                    (None, Source::Level(_)) => unreachable!("a level pack has a path"),
                };
                match got {
                    Ok(path) => {
                        let _ = tx.send(NetEvent::ResourcePackReady { hash, path });
                    }
                    Err(_) => {
                        let _ = send(&writer, status_packet(&hash, PackStatus::Failed));
                    }
                }
            })?;
        Ok(())
    }
}

fn status_packet(hash: &str, status: PackStatus) -> sb::Packet {
    sb::Packet::from(sb::ResourcePackStatus {
        hash: hash.to_owned(),
        result: VarInt(status as i32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn urls_vanilla_refuses_are_refused() {
        assert_eq!(
            source("level://My%20World/resources.zip"),
            Some(Source::Level("My World/resources.zip".into()))
        );
        assert_eq!(source("level://../x/resources.zip"), None);
        assert_eq!(source("level://a%2f..%2fb/resources.zip"), None);
        assert_eq!(source("level://world/other.zip"), None);
        assert_eq!(source("level:///etc/resources.zip"), None);
        assert_eq!(source("file:///etc/passwd"), None);
        assert_eq!(
            source("https://example.net/p.zip"),
            Some(Source::Http("https://example.net/p.zip".into()))
        );
    }

    /// One HTTP answer from a loopback server.
    fn serve(body: &'static [u8]) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/pack.zip", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0; 1024];
            let _ = s.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(body);
        });
        url
    }

    #[test]
    fn download_checks_the_hash_and_keeps_the_file() {
        let dir = std::env::temp_dir().join(format!("fc-pack-{}", std::process::id()));
        let good = sha1_hex(b"PK pack");
        let file = download(&serve(b"PK pack"), &good, &dir).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"PK pack");
        // The cached copy answers without a server.
        assert_eq!(download("http://127.0.0.1:1/x", &good, &dir).unwrap(), file);
        let bad = sha1_hex(b"other");
        assert!(download(&serve(b"PK pack"), &bad, &dir).is_err());
        assert!(!dir.join(&bad).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
