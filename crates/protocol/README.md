# ferrocraft-protocol

Owner: fc-protocol

Minecraft Java 1.8.9 (protocol 47) wire codec. It has no game logic.

- `codec`: VarInt, VarLong, strings, positions, slots, entity metadata, and the `packet!` macros.
- `nbt`: uncompressed NBT as sent inside packets.
- `frame`: length prefix, zlib compression, and AES-128/CFB8 encryption.
- `packets`, `play`: handshake, status, login, and play packets, both directions.

Tests:

- `cargo test -p ferrocraft-protocol` runs the round-trip tests.
- Set `FC_SERVER_JAR` (and `FC_JAVA` if `java` is not on the PATH) to also run `tests/server.rs`.
  It starts a local offline vanilla 1.8.9 server and never contacts a live server.
