# java/modhost-fabric

Owner: fc-mod-knot

Java side of the mod host for Fabric (Legacy Fabric 1.8.9 on Knot, Java 8; Ornithe, Java 25). The Forge host is `java/modhost`; this folder adds only what Knot needs. The host classes of `java/modhost` are the same.

## How it runs

- `Config::provision_fabric` (`crates/modhost/src/provision_fabric.rs`) gives the class path: this folder's jar `ferro-host-fabric.jar`, LaunchWrapper 1.12 (the transformers of the host implement its `IClassTransformer`), Knot with its libraries, and the game jar. `ModHost::start` runs `KnotClient` with `-javaagent:ferro-host-fabric.jar`, `-Dferro.tiny=<names.tiny>` and `-Dferro.gameDir=<game dir>` for `Loader::Fabric`.
- Knot defines the game classes itself, after Mixin ran. `Agent` is a Java agent: it sees the bytes that Knot hands to `defineClass`. The agent runs in the system class loader and hands each class of Knot's loader to `Agent.Knot`, as Knot's loader loads it. So `Host`, `Sink`, `HookIndex` and the transformers are one copy, the copy that the swapped `Main` starts.
- `Agent.Knot` gives each `net.minecraft` class the steps that the Forge tweakers give: `HookIndex.scan` (the hook table), `ShellTransformer` (the shells of `runTick`, `runGameLoop` and `startGame`) and `MinecraftBridgeTransformer`. Every other class gets `BridgeTransformer` (LWJGL, OneConfig).
- `Agent` replaces `net.minecraft.client.main.Main` with a stub: `Launch.minecraftHome = new File(System.getProperty("ferro.gameDir")); Host.run(args);`. The jar holds no `Main` of its own, because Knot would read a second `Main` and fail to find the game constructor. Only Knot's loader is swapped.
- Names: the host code writes SRG names and `Names` maps them to the loader's names. `Agent` turns `names.tiny` (Tiny v2, MCP names to intermediary names, from `java/modhost-prepare`) into the lines `Names` reads (a name, a tab, the loader's name; a class name has dots) and sets `ferro.names`.
- `FabricGlue` is `Glue.current()` when `FabricLoader` is on the class path. Knot's entrypoint patch runs in `Minecraft.<init>`, which the host does not run, so `loadMods` calls the `main` and then the `client` entrypoints. Fabric has no event bus, no overlay events and no client tick handler, so the other calls of `Glue` do nothing: the hooks of a Fabric mod are Mixins, and they run in the shells.

## Build

```sh
FERRO_JAVA_HOME=<JDK 8 or newer> FERRO_LIB=<folder of java/modhost/build.sh> FABRIC_LOADER_JAR=<fabric-loader-0.19.5.jar> java/modhost-fabric/build.sh
```

The jar is `target/modhost-fabric/ferro-host-fabric.jar` (`FERROCRAFT_MODHOST_JAR` or a file next to the executable also work). The compile classes are `java/modhost/src` without `net/minecraft/**`, and this folder's `src`.

## Run by hand

Start `Config::provision_fabric()` with the jar, `ferro-names.jar` (`java/modhost-prepare/run.sh names <dir>`) and Legacy Fabric API in `<HOME>/.ferrocraft/modhost-fabric/mods`; the JVM output shows `[ferro-agent] swap Main`, `[ferro-host] entrypoint main: <mod id>` for each mod and `[ferro-host] mods loaded`.

## Test

`test.sh` boots the host as the engine does (`FabricBootTest` is the peer, in place of the engine) with the Legacy Fabric mods of `java/modhost-accept/mods-fabric.txt`, or with the Ornithe mods of `java/modhost-accept/mods-ornithe.txt` when `FERRO_MODS_LIST` names it, and prints PASS or FAIL for the entrypoint of each mod, for `mods loaded`, for Mixin errors and for the hello. A JVM that stops before the hello is a FAIL. It needs `FERRO_JAVA_HOME`, `FERRO_LIB` and `FABRIC_LOADER_JAR` as `build.sh` does, and `FERRO_FABRIC_CONFIG`: a file of three lines made from the `Config` of `Config::provision_fabric` (its `java`, its `game_dir`, its class path, which already holds the client jar, the legacyfabric jar under `versions/`). The ignored test `cargo test -p ferrocraft-modhost provision_fabric -- --ignored` writes that file when `FERRO_FABRIC_CONFIG_OUT` is set (also set `HOME` to a clean dir, `FERROCRAFT_MODHOST_JAR` and `FERROCRAFT_NAMES_JAR`).

## Limits

- `FabricGlue.loadMods` does what Knot's entrypoint patch does in `Minecraft.<init>` (`prepareModInit`, which sets the game instance) and builds the item renderer that `startGame` builds, because the shell drops both. The renderer holds an empty model manager.
- `Chat`, `Events` and `Probe` post to Forge's event buses through `Glue.forgeBus()` and `Glue.fmlBus()`. Both are null on Fabric: the host keeps each chat line and posts no event.
- `BridgeTransformer` rules for Minecraft classes compare SRG method names and MCP descriptors. The class bytes of Knot have intermediary names, so those rules (`Gui.drawRect`, `FontRenderer` and the other draws, the player and entity patches) do not match yet, and a Fabric mod's draw call reaches no `Sink`. The LWJGL and OneConfig patches match, because their names are the same.
- `HookIndex.settle` finds no Mixin config list on Fabric (Fabric's Mixin has no `Config.getAllConfigs`): the hook table holds only the classes that were loaded when the hello was sent. It lists the changed methods under their intermediary names.
- `FabricGlue.newIngameGui` makes the vanilla `GuiIngame`. The HUD handover of the Forge host (`OverlayFilter`) has no Fabric part.
- Legacy Fabric Loader 0.19.5 only for the Legacy Fabric profile: 0.20.1 cannot patch 1.8.9 there. The Ornithe profile (`Config::provision_ornithe`: Java 25, Fabric Loader 0.20.1, gen2 names, the library upgrades of Ornithe's meta API) starts the public OneClient 1.8.9 bundles of `java/modhost-accept/mods-ornithe.txt` (`FERRO_MODS_LIST=... test.sh`). `pylon` is on the deny list: `ferro-pylon-stand-in.jar` replaces it, and the host jar holds stand-ins for LWJGL 2 `Display` and for `legacy-lwjgl3`, because the host owns the window and the input. Not tested live: drawing, input and the window of those mods.
