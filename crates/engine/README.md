# engine

Owner: fc-engine

The `ferrocraft-engine` binary: the game loop of the Ferrocraft 1.8.9 client.
It opens the title screen and joins a server from the menus or from the launch arguments with `net-client`, keeps the chunks in `world::chunk::ChunkStore`,
runs `world::Player` physics at 20 TPS, sends the position back, and draws with `render`.
The HUD comes from `gui`. The `input-audio` actions drive it: F1 hides it, F3 shows the debug screen, and the hotbar keys and the mouse wheel select the slot.
Each tick it also runs block breaking (`world::dig`): the left button digs the targeted block, the engine sends `PlayerDigging` and `Animation`, removes the block locally, and draws the selection outline (`outline.rs`: hidden by a hidden HUD, and gated by gamemode: adventure needs CanDestroy or CanPlaceOn, spectator draws inventory blocks only). The server's `BlockChange` stays the truth.
The right button places blocks (`world::place`): the engine sends `PlayerBlockPlacement` (C08) with the face and the cursor, applies the same placement rules and orientation locally, and swings the arm only on success. Changing the held slot or stack ends an item use in progress (bow, food).
Input and chat come from `input-audio`.

Launch arguments: `--username`, `--assetsDir`, `--gameDir`, `--version`, `--server`, `--port`, `--width` (default 854), `--height` (default 480), `--fullscreen`. A bad `--width` or `--height` value exits with code 2.
For online mode, pass `--uuid` and `--access-token-stdin`, then write the UTF-8 token to stdin and close it.
Direct launches can use `FERROCRAFT_ACCESS_TOKEN` as a fallback.
The renderer reads textures and the font from a client jar. The boot jar passes the classpath jar that holds `ascii.png` as a file value of `--assetsDir`, and the engine uses it as is. Otherwise the engine looks for `versions/1.8.9/1.8.9.jar` (or `versions/<--version>/<--version>.jar`) under the assets dir's parent, `--gameDir` or its parent. It logs a warning when no font is found. Resource packs listed in `resourcePacks` in `options.txt` layer over the jar.
A server resource pack is offered through a Yes/No prompt screen unless the `servers.dat` entry decides it; see `src/app/pack.rs`.
The `FERROCRAFT_SERVER` variable is the fallback for `--server`. When `--server`, `--port` and `FERROCRAFT_SERVER` are all unset, the title screen opens and nothing connects.
`ferrocraft_gui::debug_data::fps_line` owns the F3 fps line format. Vsync follows the in-game VSync option (`enableVsync` in `options.txt`, default on). `FERROCRAFT_VSYNC` overrides it: `0` turns vsync off (Immediate or Mailbox present mode), any other value turns it on.
The engine runs the mod host in the background.
It uses a complete `FERROCRAFT_MODHOST_*` configuration first.
Otherwise, it uses the OneClient cluster from `FERROCRAFT_CLUSTER` or the manifest in `--gameDir`.
A Fabric launch with neither configuration does not start a mod host.
For a plain game folder, the engine checks `mods/` and `mods/1.8.9/` for JAR and ZIP files.
The engine installs Java 8 and Forge in its cache.
It copies the archives to a separate game folder for each JVM and removes that folder after the JVM exits.
The engine checks the selected `mods/` folder twice a second.
One second after the last jar change, or when you press F8, the engine restarts the host without disconnecting.
If a host start is in progress, the engine waits for it to finish before it restarts the host.
Player skins: the `textures` profile property of a spawned player gives a skin URL and an optional cape URL. The engine downloads them in the background (trusted hosts only, see `ferrocraft_net_client::parse_textures`), caches them in `<gameDir>/assets/skins`, and draws the default skin (no cape) until they arrive. The tab list draws each listed player's head (face and hat layer) from the same skin; see `skins.rs` and `engine/tests/skin_server.rs`.
