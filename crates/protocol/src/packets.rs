//! Handshake, status and login packets.

use crate::codec::{Bytes, VarInt};

pub mod handshake {
    use super::*;

    // `next_state`: 1 = status, 2 = login.
    packet!(Handshake = 0x00 {
        protocol_version: VarInt,
        server_address: String,
        server_port: u16,
        next_state: VarInt,
    });

    packet_set!(Serverbound { Handshake });
}

pub mod status {
    pub mod clientbound {
        packet!(Response = 0x00 { json: String });
        packet!(Pong = 0x01 { time: i64 });
        packet_set!(Packet { Response, Pong });
    }

    pub mod serverbound {
        packet!(Request = 0x00 {});
        packet!(Ping = 0x01 { time: i64 });
        packet_set!(Packet { Request, Ping });
    }
}

pub mod login {
    pub mod clientbound {
        use super::super::*;

        packet!(Disconnect = 0x00 { reason: String });
        packet!(EncryptionRequest = 0x01 {
            server_id: String,
            public_key: Bytes,
            verify_token: Bytes,
        });
        packet!(LoginSuccess = 0x02 { uuid: String, username: String });
        packet!(SetCompression = 0x03 { threshold: VarInt });
        packet_set!(Packet {
            Disconnect,
            EncryptionRequest,
            LoginSuccess,
            SetCompression
        });
    }

    pub mod serverbound {
        use super::super::*;

        packet!(LoginStart = 0x00 { name: String });
        packet!(EncryptionResponse = 0x01 { shared_secret: Bytes, verify_token: Bytes });
        packet_set!(Packet {
            LoginStart,
            EncryptionResponse
        });
    }
}
