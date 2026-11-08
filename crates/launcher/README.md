# crates/launcher

Owner: fc-dropin.

Drop-in install for the official launcher (and any launcher that reads a version json).

`ferrocraft-launcher install` with no flags is the installer in the release archive. It uses the `ferrocraft-boot.jar`, `ferrocraft-fabric.jar` and `ferrocraft-engine` files next to it. It installs into every launcher it finds: the official launcher, OneClient clusters and Prism/MultiMC instances for 1.8.9 Fabric. It records what it wrote in `~/.ferrocraft/install-manifest.txt`; `ferrocraft-launcher uninstall` removes those files. OneClient has no documented data dir, so it is found by name under the app-data dir; pass `--oneclient-dir DIR` if it is elsewhere. A `--minecraft-dir`, `--oneclient-dir` or `--prism-dir` flag limits the install to the flagged launchers. `--fabric-jar JAR` picks the mod jar.

- `ferrocraft-launcher install --minecraft-dir DIR --boot-jar JAR --engine BIN` writes, for the official launcher:
  - `versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json` (`inheritsFrom` `1.8.9`, `mainClass` `net.ferrocraft.boot.Main`)
  - `libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar` (listed before the vanilla jar on the classpath)
  - `ferrocraft/ferrocraft-engine` (the Rust engine binary; `ferrocraft-engine.exe` when the source file is `.exe`). The boot jar finds it from its own location under `<minecraft-dir>/libraries`, not from `--gameDir`.
- The engine is the `ferrocraft-engine` binary of `crates/engine`. The smoke test starts the local offline server in `harness/testserver`.
- `smoke/run.sh` is the end-to-end proof: it runs the version with portablemc (offline, no sign-in) under Xvfb.

Java sources live in `java/launcher`.
