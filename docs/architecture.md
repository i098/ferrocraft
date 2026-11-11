# Architecture

Ferrocraft is a Minecraft 1.8.9 client rewritten in Rust. It is designed to load original Forge mod packs through a Java 8 mod host running beside it.
Goal: the client behaves like vanilla on servers. Method: each ported method is tested against the real 1.8.9 jar.

![Architecture](architecture.svg)

## Rust client

The Rust process owns the game and every subsystem that matters for play.

- `crates/engine` - game loop, ECS, tick scheduling (core).
- `crates/protocol` - 1.8.9 protocol 47 codec and packets.
- `crates/net-client` - 1.8.9 client session: offline and online login, keepalive, play events.
- `crates/world` - chunks, blocks, entities, physics (vanilla ports).
- `crates/render` - wgpu renderer with GUI, font and overlay draw targets.
- `crates/input-audio` - input and audio.
- `crates/launcher` - drop-in install and boot.
- `crates/modhost` - Rust side of the Java mod host: starts the JVM, feeds it input, draws its commands.

## Java mod host

A separate Java 8 process is designed to run the real Forge 1.8.9 machinery: LaunchWrapper, FMLTweaker, CoreModManager, ASM coremods, and the mods' own Mixin library, over generated stand-in Minecraft classes.
This is the planned way to load original mod packs unchanged. See [mod-host.md](mod-host.md).

## Communication

- Planned: a shared-memory ring buffer carries bulk data (render commands, chat lines, ticks). The mod host step 2 uses a loopback TCP socket instead (see `crates/modhost`).
- Synchronous hook calls (cancellable events, Mixin redirects) use spin-then-futex on shared memory.
- A per-method hook table decides when Rust hands execution to Java; unhooked methods cost one relaxed atomic load.

## Drop-in install

The installer writes one `versions` entry, mirroring the Forge 1.8.9 installer:

```
.minecraft/versions/ferrocraft-1.8.9/
  ferrocraft-1.8.9.json   inheritsFrom 1.8.9; mainClass LaunchWrapper; --tweakClass
  ferrocraft-1.8.9.jar
.minecraft/mods/         normal Forge 1.8.9 mods
```

Target: the official launcher picks the version up, and CurseForge loads the jar as a mod in a Forge 1.8.9 instance. OneClient only takes a Minecraft version plus a loader, so it uses the Fabric mod in `loaders/fabric/README.md`, which starts the engine binary.
Lunar and Badlion start only their own builds; we found no supported way to load a custom version.
Nothing from Mojang or Forge ships with the project; all derived files are generated on the player's machine.

## Hard rules

- Vanilla behavior on servers only; no cheats, no anti-cheat bypass.
- Development and automated tests use local servers only. Default tests run offline. One ignored opt-in test (FC_ONLINE=1) signs in through Mojang's session server against a local online-mode server; no test joins a live public server.

Quality gate: run `sentrux gate .` before a PR; it must pass against `.sentrux/baseline.json`.
