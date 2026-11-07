# crates/launcher

Owner: fc-dropin.

Drop-in install for the official launcher (and any launcher that reads a version json).

- `ferrocraft-launcher install --minecraft-dir DIR --boot-jar JAR --engine BIN` writes:
  - `versions/ferrocraft-1.8.9/ferrocraft-1.8.9.json` (`inheritsFrom` `1.8.9`, `mainClass` `net.ferrocraft.boot.Main`)
  - `libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar` (listed before the vanilla jar on the classpath)
  - `ferrocraft/ferrocraft-engine` (the Rust engine binary)
- `ferrocraft-engine-window` is a stand-in engine that opens a window. The `engine` crate replaces it later.
- `smoke/run.sh` is the end-to-end proof: it runs the version with portablemc (offline, no sign-in) under Xvfb.

Java sources live in `java/launcher`.
