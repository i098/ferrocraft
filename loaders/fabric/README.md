# fabric

Fabric mod jar for launchers that cannot start a custom version, such as OneClient. Its clusters take only a Minecraft version and a loader.

Use it in a `1.8.9 Fabric` cluster (Legacy Fabric or Ornithe loader). The `preLaunch` entrypoint `net.ferrocraft.fabric.PreLaunch` starts the `ferrocraft-engine` binary with the game launch arguments (the access token goes in the `FERROCRAFT_ACCESS_TOKEN` environment variable of the engine, not in its arguments, so the engine can log in to online-mode servers), and `--assetsDir` set to the vanilla 1.8.9 jar. It then exits the JVM with the engine exit code. The vanilla window never opens.

## Build

    FABRIC_LOADER_JAR=path/to/fabric-loader.jar ./build.sh out/ferrocraft-fabric.jar

The loader jar is only a compile-time API. Any loader 0.14+ works. Needs JDK 8+.

## Install

1. Put `ferrocraft-fabric.jar` in the cluster `mods/` folder.
2. Put the engine binary where the mod finds it. Search order:
   `FERROCRAFT_ENGINE`, `<gameDir>/ferrocraft/ferrocraft-engine`, `~/.ferrocraft/ferrocraft-engine`.
3. Start the cluster. Set `FERROCRAFT_SERVER=host:port` to pick the server (default `127.0.0.1:25565`).

Owner: fc-oneclient-mod
