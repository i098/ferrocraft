//! What the menus need from the network: the status ping (`ServerPinger`), the `servers.dat`
//! list (`ServerList`) and the death-screen respawn (`C16PacketClientStatus`).

use crate::link::{Reader, Writer};
use crate::{invalid, send, Session};
use ferrocraft_protocol::codec::{Reader as WireReader, VarInt, Wire};
use ferrocraft_protocol::nbt::{Nbt, Tag};
use ferrocraft_protocol::play::serverbound as sb;
use ferrocraft_protocol::{handshake, status, PROTOCOL_VERSION};
use serde_json::Value;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

/// Netty's `ReadTimeoutHandler(30)` in vanilla `NetworkManager`.
const TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_PORT: u16 = 25565;

/// One server-list ping answer (`ServerData` fields the multiplayer screen draws).
#[derive(Debug, Clone, PartialEq)]
pub struct ServerStatus {
    pub version_name: String,
    pub protocol: i32,
    pub online: i32,
    pub max: i32,
    /// Description with `§` formatting codes, like `IChatComponent.getFormattedText`.
    pub motd: String,
    /// Base64 of the 64x64 PNG server icon, as `servers.dat` stores it.
    pub favicon: Option<String>,
    pub ping_ms: i64,
    /// `players.sample` names, the tooltip over the player count.
    pub sample: Vec<String>,
}

#[derive(Debug)]
pub enum PingError {
    /// `UnknownHostException`: "Can't resolve hostname".
    UnknownHost,
    /// Anything else: "Can't connect to server".
    Failed(io::Error),
}

/// Vanilla `ServerAddress.func_78860_a`: `host[:port]`, `[v6]:port`, default port 25565.
/// shortcut: no `_minecraft._tcp` SRV lookup, add it when a test server needs SRV.
pub fn split_address(addr: &str) -> (String, u16) {
    let addr = addr.trim();
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((host, tail)) = rest.split_once(']') {
            let port = tail.strip_prefix(':').and_then(|p| p.parse().ok());
            return (host.to_owned(), port.unwrap_or(DEFAULT_PORT));
        }
    }
    match addr.split_once(':') {
        Some((host, port)) if !port.contains(':') => {
            (host.to_owned(), port.parse().unwrap_or(DEFAULT_PORT))
        }
        _ => (addr.to_owned(), DEFAULT_PORT),
    }
}

/// Status ping: handshake (next state 1), request, response, then ping/pong for the latency.
/// Blocks; the multiplayer screen runs one per entry on its own thread like vanilla's executor.
pub fn ping(addr: &str) -> Result<ServerStatus, PingError> {
    let (host, port) = split_address(addr);
    let sock = (host.as_str(), port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or(PingError::UnknownHost)?;
    let run = || -> io::Result<ServerStatus> {
        let stream = TcpStream::connect_timeout(&sock, TIMEOUT)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        let mut reader = Reader::new(stream.try_clone()?);
        let mut writer = Writer::new(stream);
        writer.send(&handshake::Serverbound::from(handshake::Handshake {
            protocol_version: VarInt(PROTOCOL_VERSION),
            server_address: host.clone(),
            server_port: port,
            next_state: VarInt(1),
        }))?;
        writer.send(&status::serverbound::Packet::from(
            status::serverbound::Request {},
        ))?;
        let status::clientbound::Packet::Response(r) = reader.recv()? else {
            return Err(invalid("expected status response"));
        };
        let mut out = parse_status(&r.json).ok_or_else(|| invalid("bad status json"))?;
        let sent = Instant::now();
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        writer.send(&status::serverbound::Packet::from(
            status::serverbound::Ping { time },
        ))?;
        if let status::clientbound::Packet::Pong(_) = reader.recv()? {
            out.ping_ms = sent.elapsed().as_millis() as i64;
        }
        Ok(out)
    };
    run().map_err(PingError::Failed)
}

fn parse_status(json: &str) -> Option<ServerStatus> {
    let v: Value = serde_json::from_str(json).ok()?;
    let int = |p: &str| v.pointer(p).and_then(Value::as_i64).unwrap_or(0) as i32;
    let favicon = v
        .get("favicon")
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("data:image/png;base64,"))
        .map(str::to_owned);
    let sample = v
        .pointer("/players/sample")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("name")?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut motd = String::new();
    if let Some(d) = v.get("description") {
        formatted(d, "", &mut motd);
    }
    Some(ServerStatus {
        version_name: v
            .pointer("/version/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        protocol: int("/version/protocol"),
        online: int("/players/online"),
        // `-1` draws vanilla's `???` when the server sends no `players` block.
        max: v
            .pointer("/players/max")
            .and_then(Value::as_i64)
            .map_or(-1, |n| n as i32),
        motd,
        favicon,
        ping_ms: 0,
        sample,
    })
}

/// `IChatComponent.getFormattedText`: each part gets its style codes, then a reset.
fn formatted(v: &Value, inherited: &str, out: &mut String) {
    match v {
        Value::String(s) => {
            out.push_str(inherited);
            out.push_str(s);
        }
        Value::Array(items) => items.iter().for_each(|i| formatted(i, inherited, out)),
        Value::Object(o) => {
            let mut style = inherited.to_owned();
            if let Some(c) = o.get("color").and_then(Value::as_str).and_then(color_code) {
                style = format!("§{c}");
            }
            for (key, code) in [
                ("obfuscated", 'k'),
                ("bold", 'l'),
                ("strikethrough", 'm'),
                ("underlined", 'n'),
                ("italic", 'o'),
            ] {
                if o.get(key).and_then(Value::as_bool) == Some(true) {
                    style.push('§');
                    style.push(code);
                }
            }
            let text = o.get("text").and_then(Value::as_str).unwrap_or("");
            if !text.is_empty() {
                out.push_str(&style);
                out.push_str(text);
                out.push_str("§r");
            }
            if let Some(extra) = o.get("extra") {
                formatted(extra, &style, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn color_code(name: &str) -> Option<char> {
    const NAMES: [&str; 16] = [
        "black",
        "dark_blue",
        "dark_green",
        "dark_aqua",
        "dark_red",
        "dark_purple",
        "gold",
        "gray",
        "dark_gray",
        "blue",
        "green",
        "aqua",
        "red",
        "light_purple",
        "yellow",
        "white",
    ];
    let i = NAMES.iter().position(|n| *n == name)?;
    char::from_digit(i as u32, 16)
}

/// Standard base64 (with padding or without); `None` on any other byte.
pub fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// One `servers.dat` entry (`ServerData` as `ServerList` stores it).
#[derive(Debug, Clone, PartialEq)]
pub struct ServerEntry {
    pub name: String,
    pub ip: String,
    /// Base64 PNG icon as saved by vanilla.
    pub icon: Option<String>,
    /// `None` = prompt, the 1.8.9 default.
    pub accept_textures: Option<bool>,
}

/// Read `servers.dat` (uncompressed NBT). A missing or broken file gives an empty list, like vanilla.
pub fn load_servers(path: &Path) -> Vec<ServerEntry> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(nbt) = Nbt::read(&mut WireReader::new(&bytes)) else {
        return Vec::new();
    };
    let list = nbt.root.iter().find_map(|(k, t)| match (k.as_str(), t) {
        ("servers", Tag::List(l)) => Some(l),
        _ => None,
    });
    list.into_iter()
        .flatten()
        .filter_map(|t| match t {
            Tag::Compound(c) => {
                let s = |key: &str| {
                    c.iter().find_map(|(k, t)| match t {
                        Tag::String(v) if k == key => Some(v.clone()),
                        _ => None,
                    })
                };
                let accept = c.iter().find_map(|(k, t)| match t {
                    Tag::Byte(b) if k == "acceptTextures" => Some(*b != 0),
                    _ => None,
                });
                Some(ServerEntry {
                    name: s("name").unwrap_or_default(),
                    ip: s("ip").unwrap_or_default(),
                    icon: s("icon"),
                    accept_textures: accept,
                })
            }
            _ => None,
        })
        .collect()
}

/// Write `servers.dat` in vanilla's layout, through a temporary file as `CompressedStreamTools.safeWrite` does.
pub fn save_servers(path: &Path, servers: &[ServerEntry]) -> io::Result<()> {
    let list = servers
        .iter()
        .map(|s| {
            let mut c = vec![
                ("name".to_owned(), Tag::String(s.name.clone())),
                ("ip".to_owned(), Tag::String(s.ip.clone())),
            ];
            if let Some(icon) = &s.icon {
                c.push(("icon".to_owned(), Tag::String(icon.clone())));
            }
            if let Some(a) = s.accept_textures {
                c.push(("acceptTextures".to_owned(), Tag::Byte(a as i8)));
            }
            Tag::Compound(c)
        })
        .collect();
    let nbt = Nbt {
        name: String::new(),
        root: vec![("servers".to_owned(), Tag::List(list))],
    };
    let mut out = Vec::new();
    nbt.write(&mut out);
    let tmp = path.with_extension("dat_tmp");
    std::fs::write(&tmp, out)?;
    std::fs::rename(tmp, path)
}

impl Session {
    /// `C16PacketClientStatus(PERFORM_RESPAWN)` from the death screen's Respawn button.
    pub fn respawn(&mut self) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::ClientStatus {
                action_id: VarInt(0),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_forms() {
        assert_eq!(split_address("localhost"), ("localhost".into(), 25565));
        assert_eq!(split_address("a.b:25566"), ("a.b".into(), 25566));
        assert_eq!(split_address("[::1]:7"), ("::1".into(), 7));
        assert_eq!(split_address("x:bad"), ("x".into(), 25565));
    }

    #[test]
    fn status_json_with_component_motd_and_icon() {
        let j = r#"{"version":{"name":"1.8.9","protocol":47},"players":{"max":20,"online":1,"sample":[{"name":"Bob","id":"0"}]},"description":{"text":"Hi ","extra":[{"text":"there","color":"gold","bold":true}]},"favicon":"data:image/png;base64,iVBORw0="}"#;
        let s = parse_status(j).expect("parses");
        assert_eq!((s.protocol, s.online, s.max), (47, 1, 20));
        assert_eq!(s.motd, "Hi §r§6§lthere§r");
        assert_eq!(s.favicon.as_deref(), Some("iVBORw0="));
        assert_eq!(parse_status("{}").unwrap().max, -1);
        assert_eq!(s.sample, ["Bob"]);
        assert_eq!(
            parse_status(r#"{"description":"plain"}"#).unwrap().motd,
            "plain"
        );
    }

    #[test]
    fn servers_dat_round_trip() {
        let dir = std::env::temp_dir().join(format!("fc-servers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("servers.dat");
        let list = vec![ServerEntry {
            name: "Minecraft Server".into(),
            ip: "127.0.0.1".into(),
            icon: None,
            accept_textures: Some(true),
        }];
        save_servers(&path, &list).unwrap();
        assert_eq!(load_servers(&path), list);
        assert!(load_servers(&dir.join("missing.dat")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
