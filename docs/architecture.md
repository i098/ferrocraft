# Architecture

Ferrocraft is a Minecraft 1.8.9 client rewritten in Rust. It is designed to load original Forge mod packs through a Java 8 mod host running beside it.
Goal: the client behaves like vanilla on servers. Method: each ported method is tested against the real 1.8.9 jar.

![Architecture](architecture.svg)

## Rust client

The Rust process owns the game and every subsystem that matters for play.

- `crates/engine` - game loop, ECS, tick scheduling (core).
- `crates/protocol` - 1.8.9 protocol 47 codec and packets.
- `crates/world` - chunks, blocks, entities, physics (vanilla ports).
- `crates/render` - wgpu renderer with GUI, font and overlay draw targets.
- `crates/input-audio` - input and audio.
- `crates/launcher` - drop-in install and boot.
- `crates/modhost-api` - stable, versioned Rust-to-Java API used by every loader and adapter.

## Java mod host

A separate Java 8 process is designed to run the real Forge 1.8.9 machinery: LaunchWrapper, FMLTweaker, CoreModManager, ASM coremods, and the mods' own Mixin library, over generated stand-in Minecraft classes.
This is the planned way to load original mod packs unchanged. See [mod-host.md](mod-host.md).

## Communication

- A shared-memory ring buffer carries bulk data (render commands, chat lines, ticks).
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

Target: the official launcher picks the version up, and OneClient, Prism, MultiMC and CurseForge load the jar as a mod in a Forge 1.8.9 instance.
Lunar and Badlion start only their own builds; we found no supported way to load a custom version.
Nothing from Mojang or Forge ships with the project; all derived files are generated on the player's machine.

## Hard rules

- Vanilla behavior on servers only; no cheats, no anti-cheat bypass.
- Development and automated tests use local offline servers only.

Quality gate: run `sentrux gate .` before a PR; it must pass against `.sentrux/baseline.json`.
