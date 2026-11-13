//! Client session for Minecraft 1.8.9 (protocol 47): login (offline, or online with an [`Auth`]
//! from the launcher), then a reader thread that answers keepalive and turns play packets into
//! [`NetEvent`] values.

mod actions;
mod auth;
mod chat;
mod entity;
mod events;
mod link;
mod pack;
mod play;
mod screens;
mod skin;

use ferrocraft_protocol::codec::{Bytes, Position, VarInt};
use ferrocraft_protocol::play::serverbound as sb;
use ferrocraft_protocol::{handshake, login, PROTOCOL_VERSION};
use link::{Reader, Writer};
use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use auth::Auth;
pub use chat::{formatted_text, plain_text, styled, Styled};
pub use events::{PlayerInfo, PlayerListEvent};
pub use ferrocraft_protocol::play::CombatEventKind;
pub use pack::PackStatus;
use screens::DEFAULT_PORT;
pub use screens::{
    base64, load_servers, ping, save_servers, split_address, PingError, ServerEntry, ServerStatus,
};
pub use skin::{fetch as fetch_skin, parse_textures, SkinInfo};
pub use world::chunk::Chunk;

/// What the server told the client, in packet order.
#[derive(Debug, Clone, PartialEq)]
pub enum NetEvent {
    JoinGame {
        entity_id: i32,
        gamemode: u8,
        hardcore: bool,
        difficulty: u8,
        max_players: u8,
        /// `reducedDebugInfo` game rule: F3 hides coordinates.
        reduced_debug_info: bool,
    },
    /// S3F custom payload: the channel and its raw bytes; the engine acts on `MC|BOpen` only.
    PluginMessage {
        channel: String,
        data: Vec<u8>,
    },
    Chunk(Box<Chunk>),
    UnloadChunk(i32, i32),
    BlockChange {
        x: i32,
        y: i32,
        z: i32,
        state: u16,
    },
    PlayerPosLook {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u8,
    },
    /// Respawn or dimension change: every loaded chunk is gone.
    Respawn {
        dimension_changed: bool,
    },
    /// Time Update: `time_of_day` is negative while the daylight cycle is frozen.
    TimeUpdate {
        world_age: i64,
        time_of_day: i64,
    },
    /// Change Game State (0x2B): `reason` 1 starts rain, 2 stops it, 7 sets rain strength, 8 thunder strength.
    GameState {
        reason: u8,
        value: f32,
    },
    /// Spawn Global Entity (0x2C) of type 1: a lightning bolt at the world position.
    Lightning {
        pos: [f64; 3],
    },
    /// Maps (0x34): colour bytes of the `columns` by `rows` area at `(x, z)` of map `id`.
    Map {
        id: i32,
        area: [usize; 4],
        data: Vec<u8>,
    },
    /// A chat message (positions 0 and 1) as styled runs.
    Chat(Vec<Styled>),
    /// An action bar message (position 2): the component's unformatted text.
    ActionBar(String),
    /// Game mode (0 survival, 1 creative, 2 adventure, 3 spectator) after a respawn or a
    /// game-state change.
    GameMode(u8),
    /// S43 Camera: the entity the view follows (spectating a mob, or back to the player).
    Camera(i32),
    /// Spawn, move, metadata or destroy of a remote entity.
    Entity(world::entities::EntityEvent),
    /// S42 CombatEvent. Kind `EntityDead` carries the death message of the receiving player.
    Combat(CombatEventKind),
    /// Metadata index 18 of a player: `EntityPlayer.getScore`.
    PlayerScore {
        entity_id: i32,
        score: i32,
    },
    /// S06 UpdateHealth.
    Health {
        health: f32,
        food: i32,
        saturation: f32,
    },
    /// S1D EntityEffect: `duration` 32767 is a maxed effect.
    EntityEffect {
        entity_id: i32,
        effect: world::effects::Effect,
    },
    /// S1E RemoveEntityEffect.
    RemoveEntityEffect {
        entity_id: i32,
        effect_id: i8,
    },
    /// S20 EntityProperties: the `generic.movementSpeed` entry, when the packet has one.
    MovementSpeed {
        entity_id: i32,
        speed: world::effects::Speed,
    },
    /// S1F SetExperience.
    Experience {
        bar: f32,
        level: i32,
        total: i32,
    },
    /// S09: the selected hotbar slot, 0 to 8.
    HeldItem(u8),
    /// S3A: the completions for the last `send_tab_complete`.
    TabComplete(Vec<String>),
    /// S2F: window -1 and slot -1 is the cursor stack.
    SetSlot {
        window: i8,
        slot: i16,
        item: world::inventory::Slot,
    },
    /// S30: every slot of a window.
    WindowItems {
        window: u8,
        items: Vec<world::inventory::Slot>,
    },
    /// S2D: `title` is plain text; an untranslated key such as `container.chest` stays the key.
    OpenWindow {
        id: u8,
        kind: String,
        title: String,
        slots: u8,
    },
    /// S2E.
    CloseWindow,
    /// S36: open the sign editor at this block.
    SignEditorOpen {
        pos: [i32; 3],
    },
    /// S31.
    WindowProperty {
        window: u8,
        property: i16,
        value: i16,
    },
    /// S32: not answered by the reader; send [`Session::confirm_transaction`] when `!accepted`.
    Transaction {
        window: u8,
        action: i16,
        accepted: bool,
    },
    /// S38 tab list change.
    PlayerList {
        action: PlayerListEvent,
    },
    /// S47: formatted text (§ codes).
    TabHeaderFooter {
        header: String,
        footer: String,
    },
    /// S45: `action` 0 title, 1 subtitle (`text` is formatted text), 2 times, 3 hide, 4 reset.
    Title {
        action: u8,
        text: Option<String>,
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    /// S3B: `mode` 0 create, 1 remove, 2 update display text.
    ScoreboardObjective {
        name: String,
        display: String,
        mode: u8,
    },
    /// S3C.
    ScoreUpdate {
        name: String,
        objective: String,
        value: i32,
        remove: bool,
    },
    /// S3D: `position` 0 list, 1 sidebar, 2 below name.
    DisplayScoreboard {
        position: u8,
        name: String,
    },
    /// S3E: `mode` 0 create, 1 remove, 2 update info, 3 add members, 4 remove members.
    /// `prefix`/`suffix` are empty in modes 1, 3 and 4.
    Team {
        name: String,
        mode: u8,
        prefix: String,
        suffix: String,
        members: Vec<String>,
    },
    /// S29: `pos` in blocks.
    Sound {
        name: String,
        pos: [f64; 3],
        volume: f32,
        pitch: u8,
    },
    /// S2A.
    Particle {
        id: i32,
        long_distance: bool,
        pos: [f32; 3],
        offset: [f32; 3],
        speed: f32,
        count: i32,
        data: Vec<i32>,
    },
    /// S28: world effect such as a door sound or a record.
    Effect {
        id: i32,
        pos: [i32; 3],
        data: i32,
        global: bool,
    },
    /// S27.
    Explosion {
        pos: [f32; 3],
        radius: f32,
        records: Vec<[i8; 3]>,
        motion: [f32; 3],
    },
    /// S39.
    Abilities {
        flags: u8,
        fly_speed: f32,
        walk_speed: f32,
    },
    /// S41.
    Difficulty(u8),
    /// S25.
    BlockBreakAnim {
        id: i32,
        pos: [i32; 3],
        stage: i8,
    },
    /// S44: the world border changed.
    WorldBorder(world::border::Update),
    /// S48: the server offers a resource pack. Answer with [`Session::accept_resource_pack`].
    ResourcePack {
        url: String,
        hash: String,
    },
    /// A pack from [`Session::accept_resource_pack`] is on disk and verified: apply it, then send
    /// [`Session::send_pack_status`] with [`PackStatus::Loaded`] (or `Failed` when it did not load).
    ResourcePackReady {
        hash: String,
        path: std::path::PathBuf,
    },
    /// The server kicked the client or the connection ended. Always the last event.
    Disconnect(String),
}

pub struct Session {
    events: Receiver<NetEvent>,
    tx: Sender<NetEvent>,
    writer: Arc<Mutex<Writer>>,
    stream: TcpStream,
}

const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

impl Session {
    /// Connects, logs in with `username` and starts the reader thread. A server in online mode
    /// needs `auth`; without it such a server is refused. Returns once the server accepted the
    /// login; `JoinGame` arrives through [`Session::poll`].
    pub fn connect(addr: &str, username: &str, auth: Option<&Auth>) -> io::Result<Session> {
        Self::connect_with_brand(addr, username, auth, "vanilla")
    }

    /// Like [`Session::connect`], with the `MC|Brand` value sent after `JoinGame`: the hosted
    /// `ClientBrandRetriever.getClientModName()` when the mod host runs, `vanilla` otherwise.
    pub fn connect_with_brand(
        addr: &str,
        username: &str,
        auth: Option<&Auth>,
        brand: &str,
    ) -> io::Result<Session> {
        let (host, port) = resolve_target(addr, srv_lookup);
        let sock = (host.as_str(), port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| invalid(format!("cannot resolve {host}:{port}")))?;
        let stream = TcpStream::connect_timeout(&sock, LOGIN_TIMEOUT)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(LOGIN_TIMEOUT))?;
        let mut reader = Reader::new(stream.try_clone()?);
        let writer = Arc::new(Mutex::new(Writer::new(stream.try_clone()?)));
        login(&mut reader, &writer, &host, port, username, auth)?;
        stream.set_read_timeout(None)?;

        let (tx, events) = mpsc::channel();
        let thread_writer = Arc::clone(&writer);
        let reader_tx = tx.clone();
        let brand = brand.to_owned();
        std::thread::Builder::new()
            .name("net-client-reader".into())
            .spawn(move || play::run(reader, thread_writer, reader_tx, &brand))?;
        Ok(Session {
            events,
            tx,
            writer,
            stream,
        })
    }

    /// Events received since the last call, oldest first. Never blocks.
    pub fn poll(&mut self) -> Vec<NetEvent> {
        self.events.try_iter().collect()
    }

    pub fn send_position_look(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> io::Result<()> {
        let p = sb::PlayerPositionAndLook {
            x,
            y,
            z,
            yaw,
            pitch,
            on_ground,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// `PlayerDigging`: `status` 0 start, 1 abort, 2 finish; `face` is the `EnumFacing` index.
    pub fn send_dig(&mut self, status: u8, pos: [i32; 3], face: u8) -> io::Result<()> {
        let [x, y, z] = pos;
        let p = sb::PlayerDigging {
            status,
            location: Position { x, y, z },
            face,
        };
        send(&self.writer, sb::Packet::from(p))
    }

    /// `Animation`: the arm swing.
    pub fn send_swing(&mut self) -> io::Result<()> {
        send(&self.writer, sb::Packet::from(sb::Animation {}))
    }

    /// `C14PacketTabComplete`: the chat text before the cursor, no looked-at block.
    pub fn send_tab_complete(&mut self, text: &str) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::TabComplete {
                text: text.to_owned(),
                looked_at: ferrocraft_protocol::codec::Opt(None),
            }),
        )
    }

    pub fn send_chat(&mut self, message: &str) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::ChatMessage {
                message: truncate_chat(message),
            }),
        )
    }
}

fn truncate_chat(message: &str) -> String {
    let mut units = 0;
    message
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= 100
        })
        .collect()
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

pub(crate) fn send(writer: &Mutex<Writer>, packet: sb::Packet) -> io::Result<()> {
    writer
        .lock()
        .map_err(|_| invalid("writer poisoned"))?
        .send(&packet)
}

fn login(
    reader: &mut Reader,
    writer: &Mutex<Writer>,
    host: &str,
    port: u16,
    username: &str,
    auth: Option<&Auth>,
) -> io::Result<()> {
    use login::clientbound::Packet as Login;
    let mut w = writer.lock().map_err(|_| invalid("writer poisoned"))?;
    w.send(&handshake::Serverbound::from(handshake::Handshake {
        protocol_version: VarInt(PROTOCOL_VERSION),
        server_address: host.to_owned(),
        server_port: port,
        next_state: VarInt(2),
    }))?;
    w.send(&login::serverbound::Packet::from(
        login::serverbound::LoginStart {
            name: username.into(),
        },
    ))?;
    drop(w);
    loop {
        match reader.recv::<Login>()? {
            Login::SetCompression(c) => {
                reader.set_compression(c.threshold.0);
                writer
                    .lock()
                    .map_err(|_| invalid("writer poisoned"))?
                    .set_compression(c.threshold.0);
            }
            Login::LoginSuccess(_) => return Ok(()),
            Login::Disconnect(d) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    plain_text(&d.reason),
                ))
            }
            Login::EncryptionRequest(r) => {
                let auth = auth.ok_or_else(|| {
                    invalid("server requires online mode: start with --accessToken and --uuid")
                })?;
                let h =
                    auth::answer_request(auth, &r.server_id, &r.public_key.0, &r.verify_token.0)?;
                // The response itself goes out in clear; everything after it is encrypted.
                let mut w = writer.lock().map_err(|_| invalid("writer poisoned"))?;
                w.send(&login::serverbound::Packet::from(
                    login::serverbound::EncryptionResponse {
                        shared_secret: Bytes(h.encrypted_secret),
                        verify_token: Bytes(h.encrypted_token),
                    },
                ))?;
                w.enable_encryption(&h.secret);
                reader.enable_encryption(&h.secret);
            }
        }
    }
}

/// Vanilla 1.8.9 `ServerAddress.getServerAddress`: the typed host and port, replaced by the
/// `_minecraft._tcp` SRV target only when the port is the default one.
fn resolve_target(addr: &str, srv: impl FnOnce(&str) -> Option<(String, u16)>) -> (String, u16) {
    let (host, port) = split_address(addr);
    if port == DEFAULT_PORT {
        if let Some((target, srv_port)) = srv(&host) {
            return (target.trim_end_matches('.').to_owned(), srv_port);
        }
    }
    (host, port)
}

fn srv_lookup(host: &str) -> Option<(String, u16)> {
    let resolver = hickory_resolver::Resolver::from_system_conf().ok()?;
    let records = resolver
        .srv_lookup(format!("_minecraft._tcp.{host}."))
        .ok()?;
    let srv = records.iter().next()?;
    Some((srv.target().to_utf8(), srv.port()))
}

#[cfg(test)]
mod chat_limit_tests {
    #[test]
    fn truncates_to_100_utf16_units() {
        assert_eq!(super::truncate_chat(&"a".repeat(150)), "a".repeat(100));
        assert_eq!(super::truncate_chat("hi"), "hi");
        let s = format!("{}\u{1F600}", "a".repeat(99));
        assert_eq!(super::truncate_chat(&s), "a".repeat(99));
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::resolve_target;

    fn no_srv(_: &str) -> Option<(String, u16)> {
        None
    }

    #[test]
    fn typed_host_is_kept() {
        assert_eq!(
            resolve_target("mc.example.net", no_srv),
            ("mc.example.net".into(), 25565)
        );
        assert_eq!(resolve_target("[::1]:25566", no_srv), ("::1".into(), 25566));
    }

    #[test]
    fn srv_applies_on_default_port_and_trims_trailing_dot() {
        let r = resolve_target("example.net", |h| {
            assert_eq!(h, "example.net");
            Some(("play.example.net.".into(), 25599))
        });
        assert_eq!(r, ("play.example.net".into(), 25599));
    }

    #[test]
    fn explicit_port_skips_srv() {
        let r = resolve_target("example.net:25566", |_| panic!("SRV must not be queried"));
        assert_eq!(r, ("example.net".into(), 25566));
    }
}
