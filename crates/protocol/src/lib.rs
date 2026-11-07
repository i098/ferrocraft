//! Minecraft Java 1.8.9 (protocol 47) wire protocol.

#[macro_use]
pub mod codec;
pub mod frame;
pub mod nbt;
pub mod packets;
pub mod play;

pub use codec::{Error, PacketData, PacketSet, Reader, Result, Wire};
pub use frame::Connection;
pub use packets::{handshake, login, status};

/// Protocol version number of Minecraft 1.8.9.
pub const PROTOCOL_VERSION: i32 = 47;
