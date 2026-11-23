# ferrocraft

A Minecraft 1.8.9 client rewritten in Rust. Goal: vanilla behavior on PvP servers like Hypixel, and original Forge mod packs run through a Java 8 mod host.

## Status
In development. The engine joins 1.8.9 servers in offline mode and online mode (`--accessToken` and `--uuid` from the launcher, or `FERROCRAFT_ACCESS_TOKEN`), renders the world, moves, breaks and places blocks, and chats. The mod host runs Forge mods that draw HUDs and screens (Keystrokes, OneConfig); see [Mod host](docs/mod-host.md).

## Install

No Rust toolchain needed. The installer downloads the release archive for your OS and runs it.

macOS and Linux (x86_64, or Apple silicon):

    curl -fsSL https://github.com/i098/ferrocraft/releases/latest/download/install.sh | sh

Windows (PowerShell):

    irm https://github.com/i098/ferrocraft/releases/latest/download/install.ps1 | iex

The installer installs into every launcher it finds:

- Official launcher: the `ferrocraft-1.8.9` version.
- OneClient clusters that run 1.8.9 with Fabric: the Fabric mod jar. The engine goes to `~/.ferrocraft`.
- No launcher found: the installer creates the default game folder and installs the `ferrocraft-1.8.9` version there.
  The installer prints the folder path.
  - Linux: `~/.minecraft`
  - macOS: `~/Library/Application Support/minecraft`
  - Windows: `%APPDATA%\.minecraft`, or `<home>\AppData\Roaming\.minecraft` if `APPDATA` is not set.
    The installer reads `<home>` from `HOME` or, if `HOME` is not set, `USERPROFILE`.
- Lunar Client and Badlion cannot start custom versions, so the installer skips them.

The installer prints what it did and the uninstall command: `~/.ferrocraft/ferrocraft-launcher uninstall`. If your launcher is in a custom folder, download the archive for your OS from the [release page](https://github.com/i098/ferrocraft/releases), extract it, and run `./ferrocraft-launcher install --minecraft-dir DIR` (or `--oneclient-dir DIR`) from the extracted folder.

### Mods

Put compatible Forge 1.8.9 mod jars in `mods/` in your game folder.
The client installs Java 8 and Forge when it first starts the mod host.
This first start needs an internet connection.
On Apple silicon, the Forge host needs Rosetta to run Java 8.

To build from source instead, see [crates/launcher](crates/launcher/README.md). Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed.

## Documentation

- [Architecture](docs/architecture.md)
- [Workspace layout](docs/layout.md)
- [Mod host](docs/mod-host.md)

## Tests

Enable the pre-push hook once per clone: `git config core.hooksPath .githooks`. It runs `cargo fmt` and `cargo clippy` on the crates that your pushed commits change. It refuses to run unless you push the clean checked-out HEAD (untracked files count as unclean).

Each ported method is tested against the real 1.8.9 jar. The test runs the real 1.8.9 client classes headless (the oracle) and compares each tick with the Rust port, bit for bit.

Install a JDK (8 or newer: `javac` and `java`) and set `JAVA_HOME`, or put them on `PATH`. Run `cargo test --workspace`. The first run downloads the client jar from Mojang's public manifest, plus the remapper and mappings the oracle needs. It caches them in `target/oracle` (override with `FERROCRAFT_ORACLE_DIR`). No login is needed.
