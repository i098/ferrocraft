# ferrocraft
Minecraft Java client rewritten in Rust (1.8.9 only)

## Tests

Each ported behavior has a differential test. The test runs the real 1.8.9 client classes headless (the oracle) and compares every tick with the Rust port, bit for bit.

Install a JDK (8 or newer) and set `JAVA_HOME`. Run `cargo test --workspace`. The first run downloads the client jar from Mojang's public manifest and caches it in `target/oracle`. No login is needed.
