# ferrocraft

A Minecraft 1.8.9 client rewritten in Rust. Goal: vanilla behavior on PvP servers like Hypixel, and original Forge mod packs run through a Java 8 mod host.

## Status
In development. The engine joins 1.8.9 servers in offline and online modes.
For online mode, sign in with your launcher.
The engine renders the world, moves, breaks and places blocks, and chats.
The mod host runs Forge mods that draw HUDs and screens (Keystrokes, OneConfig); see [Mod host](docs/mod-host.md).

## Install

No Rust toolchain needed. The installer downloads the release archive for your OS and runs it.

Close OneClient before you install Ferrocraft. Keep it closed until the installer finishes.

macOS and Linux (x86_64, or Apple silicon):

    curl -fsSL https://github.com/i098/ferrocraft/releases/latest/download/install.sh | sh

Windows (PowerShell):

    irm https://github.com/i098/ferrocraft/releases/latest/download/install.ps1 | iex

The installer downloads and verifies Polyfrost OneClient 2.12.0.
On Windows it reuses a compatible OneClient installation and stops if you must update an older one.
It installs the `Ferrocraft 1.8.9` Ornithe profile in the active OneClient data directory.
Open OneClient from the path that the installer prints.
Keep the default data directory if OneClient asks where to store its data.
Sign in once with a Microsoft account that owns Minecraft: Java Edition.
Select the profile name that the installer prints.
OneClient downloads the game and loader when you start the profile.
On Linux without FUSE, run the AppImage with `--appimage-extract-and-run`.

To use an existing launcher without a OneClient download, extract the archive for your OS from the [release page](https://github.com/i098/ferrocraft/releases).
Run `./ferrocraft-launcher install --minecraft-dir DIR` for the official launcher.
Use `./ferrocraft-launcher install --oneclient-dir DIR` with the OneClient data directory to target its 1.8.9 Fabric or Ornithe profiles.
Before reinstalling, remove the bootstrap mod from each profile where OneClient already manages a Ferrocraft install.
Other local accounts can see mod filenames, hashes, and profile paths if they have access to the staging paths or logs.
Ornithe keeps mod settings, saves, and options in the OneClient profile.
Each launch stages and extracts mod JARs in a private temporary directory.
Lunar Client and Badlion cannot start custom versions.

To run a OneClient install, open OneClient and launch the Ferrocraft profile.
For an official-only install, start the `ferrocraft-1.8.9` version in the official launcher.
Close OneClient before you uninstall a OneClient mod.
Run `~/.ferrocraft/ferrocraft-launcher uninstall` to remove the Ferrocraft files.
The uninstall command preserves OneClient and the profile's data.
If OneClient still manages the bootstrap mod, the command stops before it removes any files.
Remove the bootstrap mod in OneClient before you retry.
If the message identifies an old data folder after a move, clear that folder in OneClient Storage before you retry.

For an official launcher install, open the [Minecraft Launcher](https://www.minecraft.net/download).
Install it first if you do not have it.
Open **Installations**, select **New installation**, and select `ferrocraft-1.8.9` in the **Version** list.
Create the installation, then select it in the **Play** tab.

### Mods

Put compatible Forge 1.8.9 `.jar` or `.zip` files in `mods/` or `mods/1.8.9/` under the installation's **Game Directory**.
Use the folder that the installer printed if the **Game Directory** is blank.
Click **Play** to start the selected installation.
The mod host downloads Java 8 and Forge if they are not in its cache.
On Apple silicon, the Forge host needs Rosetta to run Java 8.
Press F8 to reload mods without disconnecting.

To build from source instead, see [crates/launcher](crates/launcher/README.md). Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed.

## Credits

Thanks to [Polyfrost](https://polyfrost.org/projects/oneclient/) and the OneLauncher contributors for OneClient.
Thanks to the [Ornithe](https://ornithemc.net/) and [Fabric](https://fabricmc.net/) contributors for the mod platform.
See [third-party credits and notices](THIRD_PARTY.md).

## Documentation

- [Architecture](docs/architecture.md)
- [Workspace layout](docs/layout.md)
- [Mod host](docs/mod-host.md)

## Tests

Enable the pre-push hook once per clone: `git config core.hooksPath .githooks`. It runs `cargo fmt` and `cargo clippy` on the crates that your pushed commits change. It refuses to run unless you push the clean checked-out HEAD (untracked files count as unclean).

Each ported method is tested against the real 1.8.9 jar. The test runs the real 1.8.9 client classes headless (the oracle) and compares each tick with the Rust port, bit for bit.

Install a JDK (8 or newer: `javac` and `java`) and set `JAVA_HOME`, or put them on `PATH`. Run `cargo test --workspace`. The first run downloads the client jar from Mojang's public manifest, plus the remapper and mappings the oracle needs. It caches them in `target/oracle` (override with `FERROCRAFT_ORACLE_DIR`). No login is needed.
