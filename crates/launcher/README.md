# crates/launcher

Owner: fc-dropin.

The installer supports Polyfrost OneClient and launchers that read a version JSON file.

`ferrocraft-launcher install` downloads the pinned official OneClient release and verifies its SHA-256 digest.
On Windows it reuses an existing OneClient installation.
It creates a `Ferrocraft 1.8.9` Ornithe profile and installs the mod jar and engine.
It also installs Ferrocraft into existing compatible launchers that it finds.
The installer reads the boot jar, mod jar, and engine next to its executable.
An explicit `--minecraft-dir DIR` or `--oneclient-dir DIR` limits installation to the selected launchers and skips the download.
Use `--fabric-jar JAR` to select a different mod jar.

The installer records Ferrocraft files in `~/.ferrocraft/install-manifest.txt` and copies itself to `~/.ferrocraft`.
The uninstall command removes those files, but leaves OneClient and the profile data.
If OneClient may manage a recorded mod, the command stops before it removes any files.
It warns and exits with an error if a manifest entry is not a permitted Ferrocraft path.

- `ferrocraft-launcher install --minecraft-dir DIR --boot-jar JAR --engine BIN` writes, for the official launcher:
  - `versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json` (`inheritsFrom` `1.8.9`, `mainClass` `net.ferrocraft.boot.Main`)
  - `libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar` (listed before the vanilla jar on the classpath)
  - `ferrocraft/ferrocraft-engine` (the Rust engine binary; `ferrocraft-engine.exe` when the source file is `.exe`). The boot jar finds it from its own location under `<minecraft-dir>/libraries`, not from `--gameDir`.
- The engine is the `ferrocraft-engine` binary of `crates/engine`. The smoke test starts the local offline server in `harness/testserver`.
- `smoke/run.sh` is the end-to-end proof: it runs the version with portablemc (offline, no sign-in) under Xvfb.

Java sources live in `java/launcher`.
