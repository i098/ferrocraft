//! Connects to a vanilla 1.8.9 server that the test starts itself in offline mode.
//!
//! Set `FC_SERVER_JAR` to a 1.8.9 `server.jar` (and `FC_JAVA` if `java` is not on the PATH).
//! Without `FC_SERVER_JAR` the test prints a notice and passes. It never contacts a live server.

use ferrocraft_protocol::codec::*;
use ferrocraft_protocol::play::{clientbound as cb, serverbound as sb};
use ferrocraft_protocol::{handshake, login, status, Connection, PROTOCOL_VERSION};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

struct Server {
    child: Child,
    stdin: ChildStdin,
    port: u16,
    dir: std::path::PathBuf,
}

impl Server {
    fn start(jar: &str) -> Server {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let dir =
            std::env::temp_dir().join(format!("ferrocraft-server-{}-{port}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
        std::fs::write(
            dir.join("server.properties"),
            format!(
                "online-mode=false\nserver-ip=127.0.0.1\nserver-port={port}\nlevel-type=FLAT\n\
                 generate-structures=false\nspawn-monsters=false\nspawn-animals=false\n\
                 view-distance=3\nmax-players=2\nenable-command-block=false\n"
            ),
        )
        .unwrap();
        let java = std::env::var("FC_JAVA").unwrap_or_else(|_| "java".into());
        let mut child = Command::new(java)
            .args(["-Xmx512M", "-jar", jar, "nogui"])
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start java");
        let stdin = child.stdin.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let out = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                eprintln!("server: {line}");
                let done = line.contains("Done (");
                let _ = tx.send(done);
            }
        });
        let started =
            std::iter::from_fn(|| rx.recv_timeout(Duration::from_secs(120)).ok()).any(|done| done);
        assert!(started, "server did not finish starting");
        Server {
            child,
            stdin,
            port,
            dir,
        }
    }

    fn command(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn connect(&self) -> Connection<TcpStream> {
        let s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        s.set_nodelay(true).unwrap();
        Connection::new(s)
    }

    fn handshake(&self, conn: &mut Connection<TcpStream>, next: i32) {
        conn.send(&handshake::Serverbound::from(handshake::Handshake {
            protocol_version: VarInt(PROTOCOL_VERSION),
            server_address: "127.0.0.1".into(),
            server_port: self.port,
            next_state: VarInt(next),
        }))
        .unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn variant_name(p: &impl std::fmt::Debug) -> String {
    format!("{p:?}").split('(').next().unwrap().to_string()
}

#[test]
fn vanilla_server_status_login_and_play() {
    let Ok(jar) = std::env::var("FC_SERVER_JAR") else {
        eprintln!("FC_SERVER_JAR not set: skipping the vanilla server test");
        return;
    };
    let mut server = Server::start(&jar);

    // Status. The vanilla server answers "{}" for a moment after "Done", so retry.
    let mut resp = None;
    for _ in 0..50 {
        let mut conn = server.connect();
        server.handshake(&mut conn, 1);
        conn.send(&status::serverbound::Packet::from(
            status::serverbound::Request {},
        ))
        .unwrap();
        let status::clientbound::Packet::Response(r) = conn.recv().unwrap() else {
            panic!("not a response")
        };
        if r.json != "{}" {
            resp = Some(r);
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let resp = resp.expect("server never filled its status");
    assert!(resp.json.contains("\"protocol\":47"), "{}", resp.json);
    assert!(resp.json.contains("1.8.9"), "{}", resp.json);
    let mut conn = server.connect();
    server.handshake(&mut conn, 1);
    conn.send(&status::serverbound::Packet::from(
        status::serverbound::Request {},
    ))
    .unwrap();
    conn.recv::<status::clientbound::Packet>().unwrap();
    conn.send(&status::serverbound::Packet::from(
        status::serverbound::Ping { time: 1234567 },
    ))
    .unwrap();
    let status::clientbound::Packet::Pong(pong) = conn.recv().unwrap() else {
        panic!("not a pong")
    };
    assert_eq!(pong.time, 1234567);

    // Login: the server enables compression before LoginSuccess.
    let mut conn = server.connect();
    server.handshake(&mut conn, 2);
    conn.send(&login::serverbound::Packet::from(
        login::serverbound::LoginStart {
            name: "Ferro".into(),
        },
    ))
    .unwrap();
    let mut saw_compression = false;
    loop {
        match conn.recv::<login::clientbound::Packet>().unwrap() {
            login::clientbound::Packet::SetCompression(c) => {
                assert_eq!(c.threshold.0, 256);
                conn.set_compression(c.threshold.0);
                saw_compression = true;
            }
            login::clientbound::Packet::LoginSuccess(ok) => {
                assert_eq!(ok.username, "Ferro");
                assert_eq!(ok.uuid.len(), 36);
                break;
            }
            other => panic!("unexpected login packet {other:?}"),
        }
    }
    assert!(saw_compression);

    let commands = [
        "op Ferro",
        "gamemode 1 Ferro",
        "title Ferro title {\"text\":\"Hello\"}",
        "title Ferro subtitle {\"text\":\"World\"}",
        "worldborder set 100 5",
        "worldborder warning distance 3",
        "scoreboard objectives add obj dummy Objective",
        "scoreboard objectives setdisplay sidebar obj",
        "scoreboard players set Ferro obj 5",
        "scoreboard teams add red",
        "scoreboard teams option red friendlyfire true",
        "scoreboard teams join red Ferro",
        "execute Ferro ~ ~ ~ summon Pig ~ ~2 ~",
        "execute Ferro ~ ~ ~ summon ArmorStand ~2 ~2 ~",
        "execute Ferro ~ ~ ~ summon Item ~ ~2 ~2 {Item:{id:\"minecraft:diamond_sword\",Count:1b,tag:{ench:[{id:16,lvl:5}]}}}",
        "give Ferro minecraft:diamond_sword 1 0 {ench:[{id:16,lvl:5}],display:{Name:\"Test\"}}",
        "effect Ferro 1 30 1",
        "xp 7 Ferro",
        "weather rain",
        "time set 6000",
        "execute Ferro ~ ~ ~ particle heart ~ ~2 ~ 0 0 0 0 5",
        "execute Ferro ~ ~ ~ playsound random.levelup Ferro ~ ~ ~",
        "execute Ferro ~ ~ ~ setblock ~ ~2 ~ minecraft:chest",
        "execute Ferro ~ ~ ~ setblock ~1 ~2 ~ minecraft:standing_sign",
        "difficulty 3",
        "kill Ferro",
    ];
    // Like the vanilla client, send nothing in play until JoinGame arrives: the server
    // switches to the play state on its own tick, after it sent LoginSuccess.

    let mut seen = BTreeSet::new();
    let mut sent_chat = false;
    let mut chat_echo = false;
    let mut tab_matches = false;
    let mut dead = false;
    let mut sentinel = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(40);
    while std::time::Instant::now() < deadline {
        let frame = conn.recv_frame().unwrap();
        let packet =
            cb::Packet::decode(&frame).unwrap_or_else(|e| panic!("decode {frame:02x?}: {e}"));
        assert_eq!(
            packet.encode(),
            frame,
            "re-encode differs for {}",
            variant_name(&packet)
        );
        seen.insert(variant_name(&packet));
        match packet {
            cb::Packet::JoinGame(_) => {
                conn.send(&sb::Packet::from(sb::PluginMessage {
                    channel: "MC|Brand".into(),
                    data: Rest(b"ferrocraft".to_vec()),
                }))
                .unwrap();
                conn.send(&sb::Packet::from(sb::ClientSettings {
                    locale: "en_US".into(),
                    view_distance: 3,
                    chat_mode: 0,
                    chat_colors: true,
                    skin_parts: 0x7F,
                }))
                .unwrap();
                commands.iter().for_each(|c| server.command(c));
            }
            cb::Packet::KeepAlive(k) => conn
                .send(&sb::Packet::from(sb::KeepAlive { id: k.id }))
                .unwrap(),
            cb::Packet::PlayerPositionAndLook(p) => {
                conn.send(&sb::Packet::from(sb::PlayerPositionAndLook {
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    yaw: p.yaw,
                    pitch: p.pitch,
                    on_ground: true,
                }))
                .unwrap();
                if !sent_chat {
                    sent_chat = true;
                    for p in [
                        sb::Packet::from(sb::ChatMessage {
                            message: "hello from ferrocraft".into(),
                        }),
                        sb::Packet::from(sb::TabComplete {
                            text: "/gamemo".into(),
                            looked_at: Opt(None),
                        }),
                        sb::Packet::from(sb::HeldItemChange { slot: 2 }),
                        sb::Packet::from(sb::Animation {}),
                        sb::Packet::from(sb::EntityAction {
                            entity_id: VarInt(0),
                            action_id: VarInt(0),
                            jump_boost: VarInt(0),
                        }),
                        sb::Packet::from(sb::PlayerPosition {
                            x: p.x,
                            y: p.y,
                            z: p.z,
                            on_ground: true,
                        }),
                        sb::Packet::from(sb::PlayerLook {
                            yaw: 10.0,
                            pitch: 5.0,
                            on_ground: true,
                        }),
                        sb::Packet::from(sb::Player { on_ground: true }),
                        sb::Packet::from(sb::PlayerAbilities {
                            flags: 0,
                            flying_speed: 0.05,
                            walking_speed: 0.1,
                        }),
                    ] {
                        conn.send(&p).unwrap();
                    }
                }
            }
            cb::Packet::ChatMessage(c) => {
                chat_echo |= c.json.contains("hello from ferrocraft");
                sentinel |= c.json.contains("ferro-sentinel");
            }
            cb::Packet::TabComplete(t) => tab_matches |= t.matches.iter().any(|m| m == "/gamemode"),
            cb::Packet::UpdateHealth(h) if h.health <= 0.0 && !dead => {
                dead = true;
                server.command("say ferro-sentinel");
                conn.send(&sb::Packet::from(sb::ClientStatus {
                    action_id: VarInt(0),
                }))
                .unwrap();
            }
            _ => {}
        }
        if sentinel && seen.contains("Respawn") {
            break;
        }
    }
    eprintln!("play packets seen: {seen:?}");

    assert!(chat_echo, "server did not echo the chat message");
    assert!(tab_matches, "tab completion reply missing /gamemode");
    for name in [
        "JoinGame",
        "SpawnPosition",
        "PlayerPositionAndLook",
        "PlayerAbilities",
        "ServerDifficulty",
        "PluginMessage",
        "MapChunkBulk",
        "WindowItems",
        "SetSlot",
        "UpdateHealth",
        "TimeUpdate",
        "EntityProperties",
        "ChangeGameState",
        "PlayerListItem",
        "Title",
        "WorldBorder",
        "ScoreboardObjective",
        "DisplayScoreboard",
        "UpdateScore",
        "Teams",
        "SpawnMob",
        "EntityMetadata",
        "SetExperience",
        "EntityEffect",
        "UpdateSign",
        "UpdateHealth",
        "Respawn",
        "HeldItemChange",
        "EntityStatus",
        "SoundEffect",
    ] {
        assert!(
            seen.contains(name),
            "server never sent {name}; saw {seen:?}"
        );
    }
}
