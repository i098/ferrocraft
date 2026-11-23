# crates/launcher

Owner: fc-dropin.

The installer supports Polyfrost OneClient and launchers that read a version JSON file.

`ferrocraft-launcher install` downloads the pinned official OneClient release and verifies its SHA-256 digest.
On Windows it reuses a compatible OneClient installation and stops if you must update an older one.
It installs the mod and engine for the Ornithe profile that Ferrocraft owns.
It reuses a marked profile or chooses a folder and display name that do not conflict with other profiles.
The default install does not add the mod to other OneClient profiles.
It also installs Ferrocraft into the official Minecraft directory if that directory exists.
Exit OneClient if it is running, then open it again to load the profile.
The installer reads the boot jar, mod jar, and engine next to its executable.
The release installer reads both Fabric host JARs from that directory, even when you use `--engine`.
An explicit `--minecraft-dir DIR` or `--oneclient-dir DIR` limits installation to the selected launchers and skips the download.
Use `--fabric-jar JAR` to select a different mod jar.
Before reinstalling, remove the bootstrap mod from each profile where OneClient already manages a Ferrocraft install.

The installer records Ferrocraft files in `~/.ferrocraft/install-manifest.txt` and copies itself to `~/.ferrocraft`.
The uninstall command removes those files, but leaves OneClient and the profile data.
If OneClient still manages the bootstrap mod, the command stops before it removes any files.
Remove the bootstrap mod in OneClient before you retry.
If the message identifies an old data folder after a move, clear that folder in OneClient Storage before you retry.
It warns and exits with an error if a manifest entry is not a permitted Ferrocraft path.

- `ferrocraft-launcher install --minecraft-dir DIR --boot-jar JAR --engine BIN` writes, for the official launcher:
  - `versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json` (`inheritsFrom` `1.8.9`, `mainClass` `net.ferrocraft.boot.Main`)
  - `libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar` (listed before the vanilla jar on the classpath)
  - `ferrocraft/ferrocraft-engine` (the Rust engine binary; `ferrocraft-engine.exe` when the source file is `.exe`). The boot jar finds it from its own location under `<minecraft-dir>/libraries`, not from `--gameDir`.
- The engine is the `ferrocraft-engine` binary of `crates/engine`. The smoke test starts the local offline server in `harness/testserver`.
- `smoke/run.sh` is the end-to-end proof: it runs the version with portablemc (offline, no sign-in) under Xvfb.

Java sources live in `java/launcher`.
