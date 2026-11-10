# java/modhost

Java 8 side of the mod host. The Rust engine starts this JVM (`crates/modhost`) and reads its draw commands over a loopback socket.

## How it runs

- `net.minecraft.client.main.Main` replaces the vanilla entry point. It starts no window and no GL. It builds a stand-in `Minecraft` (`Minecraft.java`), loads the mods through Forge's `FMLClientHandler`, then runs `Host`.
- `HostTweaker` registers `BridgeTransformer` first, so LWJGL and OneConfig classes are patched before they load. `HostLateTweaker` queues `HostLastTweaker`, which registers `MinecraftBridgeTransformer` after FML's deobfuscation tweaker: Minecraft classes keep notch names until then.
- `BridgeTransformer` rewrites four things: LWJGL `Keyboard`, `Mouse` and `Display` read `Input`; GL calls and `FontRenderer.drawString` become commands in `Sink`; OneConfig's `NanoVGHelper` and `GLPlatform` calls go to `Nvg`; a few `Entity` methods that need a real world return fixed values.
- `Host` posts Forge's `RenderGameOverlayEvent`, so HUD mods draw. `OverlayFilter` cancels the vanilla elements, because Rust draws those.
- `Sink` collects rectangles, gradients and text. `Host` sends them as one frame per round.

## Build

```sh
FERRO_JAVA_HOME=<Java 8 JDK> FERRO_LIB=<folder with lw.jar asm-all-5.0.3.jar forge.jar guava-17.0.jar> java/modhost/build.sh
```

The class path of the JVM is `java/modhost/cls`, then the libraries, then the Forge jar and the Minecraft 1.8.9 jar.
Set `FERROCRAFT_MODHOST_JAVA`, `FERROCRAFT_MODHOST_CP` and `FERROCRAFT_MODHOST_GAMEDIR` for the engine. Put the mod jars (for example Keystrokes) in `GAMEDIR/mods`.

## Limits

- The Minecraft classes are the vanilla 1.8.9 classes from the player's jar, patched at load time. The generated stand-in jar from `ferro-prepare` is not built yet.
- The stand-in player, world and options are mirrors of Rust state. Only what the HUD mods read is filled in.
- Mods that draw with GL calls other than rectangles, gradients and text are not forwarded yet.
