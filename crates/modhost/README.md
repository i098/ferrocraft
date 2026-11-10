# crates/modhost

Owner: fc-modhost-pvp

Rust side of the mod host. It starts the JVM that `java/modhost` prepares, and talks to it over a loopback TCP socket.

- `ModHost::start` spawns Temurin 8 with LaunchWrapper, the Forge 1.8.9 `FMLTweaker` and `net.ferrocraft.tweak.HostTweaker`, and returns at once. The JVM loads the mods from `GAME_DIR/mods`. A worker thread waits for its connection and stops, with a log line, if the process exits first.
- The worker then runs one frame per `ModHost::set_view` call (one per engine draw): it sends the window size, GUI scale, whether a screen is open, the mouse and the key and mouse events since the last frame (each marked with whether a screen was open when it arrived), and reads back whether a screen a mod opened is current, then the draw commands: filled rectangles, gradients and text from the mods' `Gui` and `FontRenderer` calls. When the JVM closes the socket the commands are cleared and the mod screen flag is reset.
- `ModHost::commands` returns the last frame. The engine draws it over the HUD, only where the HUD is shown. `ModHost::screen_open` is the mod screen flag: the engine treats it as an open screen, frees the cursor, and keeps key and mouse events from the game and from its own menus (they go to the JVM only).
- `ModHost::window_event` queues key, mouse button and wheel events from winit. `keys::lwjgl` maps a winit key to its LWJGL 2 key code.

The wire format is in `wire.rs` and in `java/modhost/src/net/ferrocraft/host/Sink.java` (commands) and `Host.java` (frames).
