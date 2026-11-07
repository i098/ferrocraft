# ferrocraft
Minecraft Java client rewritten in Rust (1.8.9 only)

## Tests

Each ported behavior has a differential test. The test runs the real 1.8.9 client classes headless (the oracle) and compares every tick with the Rust port, bit for bit.

Install a JDK (8 or newer: `javac` and `java`) and set `JAVA_HOME`, or put them on `PATH`. Run `cargo test --workspace`. The first run downloads the client jar from Mojang's public manifest, plus the remapper and mappings the oracle needs. It caches them in `target/oracle` (override with `FERROCRAFT_ORACLE_DIR`). No login is needed.
