# engine

Owner: fc-engine

The `ferrocraft-engine` binary: the game loop of the Ferrocraft 1.8.9 client.
It joins a server with `net-client`, keeps the chunks in `world::chunk::ChunkStore`,
runs `world::Player` physics at 20 TPS, sends the position back, and draws with `render`.
Input and chat come from `input-audio`.

Launch arguments: `--username`, `--assetsDir`, `--gameDir`, `--version`, `--server`, `--port`.
The renderer reads textures and the font from a client jar. The boot jar passes the classpath jar that holds `ascii.png` as a file value of `--assetsDir`, and the engine uses it as is. Otherwise the engine looks for `versions/1.8.9/1.8.9.jar` (or `versions/<--version>/<--version>.jar`) under the assets dir's parent, `--gameDir` or its parent. It logs a warning when no font is found.
The `FERROCRAFT_SERVER` variable is the fallback server. The default is `127.0.0.1:25565`.
