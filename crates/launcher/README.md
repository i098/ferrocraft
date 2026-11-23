# crates/launcher

Owner: fc-dropin.

Drop-in install for the official launcher (and any launcher that reads a version json).

Run `ferrocraft-launcher install` from the extracted release archive so it can find the required files next to it.
See [Install](../../README.md#install) for automatic launcher detection and the default folder.
The installer records its output paths in `~/.ferrocraft/install-manifest.txt`, even if an install fails halfway.
It copies itself to `~/.ferrocraft` so `uninstall` works afterwards.
`ferrocraft-launcher uninstall` removes the recorded paths, including extra files in the recorded version and engine folders.
It skips invalid manifest entries with a warning and exits with a nonzero status.
An entry is invalid if it is a symlink or is not a Ferrocraft path in a recorded launcher folder.
The installer detects OneClient by name in the app-data folder.
Use `--oneclient-dir DIR` if OneClient is elsewhere.
The `--minecraft-dir` and `--oneclient-dir` flags limit the install to the specified launchers.
Use `--fabric-jar JAR` to select the Fabric mod jar.

- `ferrocraft-launcher install --minecraft-dir DIR --boot-jar JAR --engine BIN` writes, for the official launcher:
  - `versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json` (`inheritsFrom` `1.8.9`, `mainClass` `net.ferrocraft.boot.Main`)
  - `libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar` (listed before the vanilla jar on the classpath)
  - `ferrocraft/ferrocraft-engine` (the Rust engine binary; `ferrocraft-engine.exe` when the source file is `.exe`). The boot jar finds it from its own location under `<minecraft-dir>/libraries`, not from `--gameDir`.
  - `ferrocraft/ferro-host.jar`, if present next to the source engine binary. The release archive includes it.
    For a source install with Forge mods, [build the host jar](../../java/modhost/README.md#build) and put it next to the source engine binary.
- The engine is the `ferrocraft-engine` binary of `crates/engine`. The smoke test starts the local offline server in `harness/testserver`.
- `smoke/run.sh` starts the installed version with PortableMC under Xvfb in offline mode. It does not load mods.

Java sources live in `java/launcher`.
