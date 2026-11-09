# engine

Owner: fc-engine

The `ferrocraft-engine` binary: the game loop of the Ferrocraft 1.8.9 client.
It opens the title screen and joins a server from the menus or from the launch arguments with `net-client`, keeps the chunks in `world::chunk::ChunkStore`,
runs `world::Player` physics at 20 TPS, sends the position back, and draws with `render`.
The HUD comes from `gui`. The `input-audio` actions drive it: F1 hides it, F3 shows the debug screen, and the hotbar keys and the mouse wheel select the slot.
Each tick it also runs block breaking (`world::dig`): the left button digs the targeted block, the engine sends `PlayerDigging` and `Animation`, removes the block locally, and draws the selection outline. The server's `BlockChange` stays the truth.
Input and chat come from `input-audio`.

Launch arguments: `--username`, `--assetsDir`, `--gameDir`, `--version`, `--server`, `--port`.
The renderer reads textures and the font from a client jar. The boot jar passes the classpath jar that holds `ascii.png` as a file value of `--assetsDir`, and the engine uses it as is. Otherwise the engine looks for `versions/1.8.9/1.8.9.jar` (or `versions/<--version>/<--version>.jar`) under the assets dir's parent, `--gameDir` or its parent. It logs a warning when no font is found.
The `FERROCRAFT_SERVER` variable is the fallback for `--server`. When `--server`, `--port` and `FERROCRAFT_SERVER` are all unset, the title screen opens and nothing connects.
The debug text shows `N fps (X ms)`. Set `FERROCRAFT_VSYNC=0` to turn off vsync (Immediate or Mailbox present mode). The default is vsync on.
