# Architecture

Ferrocraft is a Minecraft 1.8.9 client rewritten in Rust that loads original Forge mod packs through a Java 8 mod host running beside it.
The client behaves like vanilla on servers, and every ported method passes differential tests against the real 1.8.9 jar.

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

A separate Java 8 process runs the real Forge 1.8.9 machinery: LaunchWrapper, FMLTweaker, CoreModManager, ASM coremods, and the mods' own Mixin library, over generated stand-in Minecraft classes.
This is how original mod packs load unchanged. See [mod-host.md](mod-host.md).

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

The official launcher picks the version up. OneClient, Prism, MultiMC and CurseForge can load the jar as a mod in a Forge 1.8.9 instance.
Lunar and Badlion cannot load custom versions, so they are out of scope.
Nothing from Mojang or Forge ships with the project; all derived files are generated on the player's machine.

## Hard rules

- Vanilla behavior on servers only; no cheats, no anti-cheat bypass.
- Never connect to a live server; test against local offline servers only.
