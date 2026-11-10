# crates/modhost

Owner: fc-modhost-pvp

Rust side of the mod host. It starts the JVM that `java/modhost` prepares, and talks to it over a loopback TCP socket.

- `ModHost::start` spawns Temurin 8 with LaunchWrapper, the Forge 1.8.9 `FMLTweaker` and `net.ferrocraft.tweak.HostTweaker`. The JVM loads the mods from `GAME_DIR/mods`.
- A worker thread sends the key, mouse and window state each round and reads back the draw commands of the frame: filled rectangles, gradients and text from the mods' `Gui` and `FontRenderer` calls.
- `ModHost::commands` returns the last frame. The engine draws it over the HUD.
- `ModHost::window_event` keeps the key and mouse state from winit events. `keys::lwjgl` maps a winit key to its LWJGL 2 key code.

The wire format is in `wire.rs` and in `java/modhost/src/net/ferrocraft/host/Sink.java` (commands) and `Host.java` (frames).
