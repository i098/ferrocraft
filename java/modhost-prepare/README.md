# java/modhost-prepare

Owner: fc-modhost-pvp

Step 1 of `ferro-prepare` (see `docs/mod-host.md`): it makes the two jars the mod host boots on, from files the player already has. Nothing Mojang or Forge made is committed.

```sh
FERRO_JAVA_HOME=<Java 8 JDK> FERRO_LIB=<folder with forge.jar, lw.jar, guava, asm-all, lzma, log4j> \
  java/modhost-prepare/run.sh <client-1.8.9.jar> <out-dir>
```

It writes to `out-dir`:

- `ferro-mc-1.8.9.jar` - the player's client jar with Forge's binpatches applied, so FML patches nothing at boot. Notch names stay. `META-INF/` is dropped, because the Mojang signature of every patched class would fail.
- `ferro-forge.jar` - the Forge universal jar without `binpatches.pack.lzma`, so FML logs "The binary patch set is missing" and skips its patch step.

The patch step is Forge's own `ClassPatchManager`, called with `Side.CLIENT`, the same code FML runs at boot. The output is the next input of the stand-in jar work: the class shapes are already the patched ones, so only the bodies change later.

`java/modhost/test.sh` with `FERRO_PREPARE_OUT=<out-dir>` boots the mods on these two jars. The Minecraft classes then load from `ferro-mc-1.8.9.jar`, which `ModBootTest` prints.

## Fabric name table

`run.sh names <out-dir>` (any JDK 8 or newer in `FERRO_JAVA_HOME`, and `curl`) packs `NameTable` with mapping-io 0.9.2 (sha1 checked) into `<out-dir>/ferro-names.jar`. `Config::provision_fabric` (`crates/modhost/src/provision_fabric.rs`) runs it on Java 8 with the MCP 1.8.9 SRG zip and the Legacy Fabric intermediary jar. It writes `~/.ferrocraft/modhost-fabric/names.tiny`, a Tiny v2 file from Forge (MCP) names to intermediary names, for example `field_71439_g` to `field_10310`.

## Limits

- The bodies are the real vanilla ones. The stand-in bodies (T0 to T3) are not written yet.
- `hook-index.json` is not written yet.
