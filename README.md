# ferrocraft

A Minecraft 1.8.9 client rewritten in Rust: vanilla behavior on PvP servers like Hypixel, and original Forge mod packs run through a Java 8 mod host.

## Status
In development. Scaffold slice in flight; protocol and mod-host slices next. Not yet playable.

## Quick start

No release yet. Development needs the Rust toolchain (edition 2021) and Java 8 for the mod host. Read the docs below first.

## Documentation

- [Architecture](docs/architecture.md)
- [Workspace layout](docs/layout.md)
- [Mod host](docs/mod-host.md)

## Tests

Each ported behavior has a differential test. The test runs the real 1.8.9 client classes headless (the oracle) and compares every tick with the Rust port, bit for bit.

Install a JDK (8 or newer: `javac` and `java`) and set `JAVA_HOME`, or put them on `PATH`. Run `cargo test --workspace`. The first run downloads the client jar from Mojang's public manifest, plus the remapper and mappings the oracle needs. It caches them in `target/oracle` (override with `FERROCRAFT_ORACLE_DIR`). No login is needed.
