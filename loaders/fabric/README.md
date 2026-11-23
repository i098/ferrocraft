# fabric

Fabric mod jar for launchers that cannot start a custom version, such as OneClient. Its clusters take only a Minecraft version and a loader.

Use it in a `1.8.9 Fabric` cluster (Legacy Fabric or Ornithe loader).
The `preLaunch` entry point starts the engine with the game arguments and sends the access token through stdin.
It sets `--assetsDir` to the vanilla 1.8.9 jar.
The vanilla window does not open.

## Build

    FABRIC_LOADER_JAR=path/to/fabric-loader.jar ./build.sh out/ferrocraft-fabric.jar

The loader jar is only a compile-time API. Any loader 0.14+ works. Needs JDK 8+.

## Install

1. Put `ferrocraft-fabric.jar` in the cluster `mods/` folder.
2. Put the engine binary where the mod finds it. Search order:
   `FERROCRAFT_ENGINE`, `<gameDir>/ferrocraft/ferrocraft-engine`, `~/.ferrocraft/ferrocraft-engine`.
3. Start the cluster. Set `FERROCRAFT_SERVER=host:port` to pick the server (default `127.0.0.1:25565`).

Owner: fc-oneclient-mod
