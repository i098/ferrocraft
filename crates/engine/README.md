# engine

Owner: fc-engine

The `ferrocraft-engine` binary: the game loop of the Ferrocraft 1.8.9 client.
It joins a server with `net-client`, keeps the chunks in `world::chunk::ChunkStore`,
runs `world::Player` physics at 20 TPS, sends the position back, and draws with `render`.
Input and chat come from `input-audio`.

Launch arguments: `--username`, `--assetsDir`, `--server`, `--port`.
The `FERROCRAFT_SERVER` variable is the fallback server. The default is `127.0.0.1:25565`.
