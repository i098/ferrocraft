//! Client session for Minecraft 1.8.9 (protocol 47): offline login, then a reader thread that
//! answers keepalive and turns play packets into [`NetEvent`] values.
//!
//! Only for servers with `online-mode=false`: no encryption, no Microsoft sign-in.

mod chat;
mod link;
mod play;

use ferrocraft_protocol::codec::VarInt;
use ferrocraft_protocol::play::serverbound as sb;
use ferrocraft_protocol::{handshake, login, PROTOCOL_VERSION};
use link::{Reader, Writer};
use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use chat::plain_text;
pub use world::chunk::Chunk;

/// What the server told the client, in packet order.
#[derive(Debug, Clone, PartialEq)]
pub enum NetEvent {
    JoinGame {
        entity_id: i32,
        gamemode: u8,
    },
    Chunk(Chunk),
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
    Respawn,
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
    /// Plain text of a chat message (the action bar is dropped).
    Chat(String),
    /// The server kicked the client or the connection ended. Always the last event.
    Disconnect(String),
}

pub struct Session {
    events: Receiver<NetEvent>,
    writer: Arc<Mutex<Writer>>,
    stream: TcpStream,
}

const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

impl Session {
    /// Connects, logs in with `username` (offline mode) and starts the reader thread.
    /// Returns once the server accepted the login; `JoinGame` arrives through [`Session::poll`].
    pub fn connect(addr: &str, username: &str) -> io::Result<Session> {
        let sock = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| invalid(format!("cannot resolve {addr}")))?;
        let stream = TcpStream::connect_timeout(&sock, LOGIN_TIMEOUT)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(LOGIN_TIMEOUT))?;
        let mut reader = Reader::new(stream.try_clone()?);
        let writer = Arc::new(Mutex::new(Writer::new(stream.try_clone()?)));
        login(&mut reader, &writer, sock, username)?;
        stream.set_read_timeout(None)?;

        let (tx, events) = mpsc::channel();
        let thread_writer = Arc::clone(&writer);
        std::thread::Builder::new()
            .name("net-client-reader".into())
            .spawn(move || play::run(reader, thread_writer, tx))?;
        Ok(Session {
            events,
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

    pub fn send_chat(&mut self, message: &str) -> io::Result<()> {
        send(
            &self.writer,
            sb::Packet::from(sb::ChatMessage {
                message: message.into(),
            }),
        )
    }
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
    sock: std::net::SocketAddr,
    username: &str,
) -> io::Result<()> {
    use login::clientbound::Packet as Login;
    let mut w = writer.lock().map_err(|_| invalid("writer poisoned"))?;
    w.send(&handshake::Serverbound::from(handshake::Handshake {
        protocol_version: VarInt(PROTOCOL_VERSION),
        server_address: sock.ip().to_string(),
        server_port: sock.port(),
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
            Login::EncryptionRequest(_) => {
                return Err(invalid(
                    "server requires online mode; use online-mode=false",
                ))
            }
        }
    }
}
