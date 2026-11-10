# java/modhost

Java 8 side of the mod host. The Rust engine starts this JVM (`crates/modhost`) and reads its draw commands over a loopback socket.

## How it runs

- `net.minecraft.client.main.Main` replaces the vanilla entry point. It starts no window and no GL. It builds a stand-in `Minecraft` (`Minecraft.java`), loads the mods through Forge's `FMLClientHandler`, then runs `Host`.
- `HostTweaker` registers `BridgeTransformer` first, so LWJGL and OneConfig classes are patched before they load. `HostLateTweaker` queues `HostLastTweaker`, which registers `MinecraftBridgeTransformer` after FML's deobfuscation tweaker: Minecraft classes keep notch names until then.
- `BridgeTransformer` rewrites four things: LWJGL `Keyboard`, `Mouse` and `Display` read `Input` (polled state and the event queues behind `next()`); GL calls and `FontRenderer.drawString` become commands in `Sink`; OneConfig's `NanoVGHelper` and `GLPlatform` calls go to `Nvg`; a few `Entity` methods that need a real world return fixed values. It also drops the two `NanoVGHelper` methods typed on `NVGColor` (OneConfig's isolated LWJGL 3), because the game class loader cannot load that class next to LWJGL 2 and `Nvg`'s proxy of the interface needs every method type.
- `Host` runs the input loops of `Minecraft.runTick` inside each 20 Hz tick, between OneConfig's `TickEvent` START and END (its `TickDelay`, and so `GuiUtils.displayScreen`, waits for them): for every mouse and key event it posts Forge's `MouseEvent`, sets `KeyBinding` state (mouse buttons are code - 100), posts FML's `InputEvent` and OneConfig's `KeyInputEvent` or `MouseInputEvent`.
- OneConfig builds its renderer classes (`NanoVGHelper`, `ScissorHelper`, `AssetHelper`, `FontHelper`, `TinyFD`) in an isolated LWJGL 3 class loader that cannot start here, so `BridgeTransformer` makes each `LwjglManager` accessor return an `Nvg.stand` proxy of the interface. Calls with no `Nvg` method of the same name and argument count do nothing. `Nvg.setupAndDraw` without the Minecraft-scale flag draws in window pixels, as NanoVG does, so it scales the Sink's GUI units by the inverse GUI scale.
- `Host` posts OneConfig's `RenderEvent` START and END around each frame: `GuiUtils` takes the click edge of the left button and the frame time for GUI animations from it.
- Rust sends whether a screen (menu, container, chat) is open. `Host` mirrors it into `Minecraft.currentScreen` with a stand-in `GuiIngameMenu` and calls `KeyBinding.unPressAllKeys()` on each change, as `displayGuiScreen` does. Events that arrived while a screen was open are not game input: no `KeyBinding` press, no `InputEvent`; only their releases clear the key bind. Polled `Keyboard` and `Mouse` state and `Keyboard.next()` still see them.
- A screen a mod opens (OneConfig's GUI) goes through `Minecraft.displayGuiScreen`, which `BridgeTransformer` replaces with `Host.displayScreen`: it closes the old screen, releases the key binds and sizes the new one. `Minecraft.setIngameFocus`, which `GuiScreen.keyTyped` calls after Escape, does nothing. While the mod screen is current, `Host` calls its `handleInput` and `updateScreen` each tick instead of the game input loops, so the events reach it through the patched `Keyboard` and `Mouse`, and draws it after the overlay. Rust's mirror does not replace it. Each reply frame starts with a byte that says a mod screen is current: Rust then frees the cursor, sends no game input, and lets Escape reach the screen, which closes itself as `GuiScreen.keyTyped` does.
- `Host` also posts Forge's `RenderGameOverlayEvent`, so HUD mods draw. `OverlayFilter` cancels the vanilla elements, because Rust draws those.
- `Sink` collects rectangles, gradients and text. `Host` sends them as one frame per round. The window size is in pixels and the GUI scale is the setting, so `ScaledResolution` in the JVM gives the same scale as Rust.

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
- Mod screens are drawn only where the mod HUD is drawn (in a world); the window and sound handler parts of `displayGuiScreen` are skipped.
- OneConfig's scissors do not clip: the wire has no clip rectangle, so widgets that rely on a scissor draw past their box. Images, SVGs and file dialogs (`AssetHelper`, `TinyFD`) do nothing.
- `EntityRenderer.loadShader` does nothing: no shader group loads, so OneConfig's background blur never turns on.
