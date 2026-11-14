# java/modhost

Owner: fc-modhost-pvp

Java 8 side of the mod host. The Rust engine starts this JVM (`crates/modhost`) and reads its draw commands over a loopback socket.

## How it runs

- `net.minecraft.client.main.Main` replaces the vanilla entry point. It starts no window and no GL. It builds a stand-in `Minecraft` (`Minecraft.java`), loads the mods through Forge's `FMLClientHandler`, then runs `Host`.
- `HostTweaker` registers `BridgeTransformer` first, so LWJGL and OneConfig classes are patched before they load. `HostLateTweaker` queues `HostLastTweaker`, which registers `MinecraftBridgeTransformer` after FML's deobfuscation tweaker (Minecraft classes keep notch names until then), and `HookIndex$Scanner` in `getLaunchArguments`, after Mixin's transformer has moved to the end of the list.
- `BridgeTransformer` rewrites four things: LWJGL `Keyboard` and `Mouse` read `Input` (polled state and the event queues behind `next()`); GL calls and `FontRenderer.drawString` become commands in `Sink`; OneConfig's `NanoVGHelper` and `GLPlatform` calls go to `Nvg`; a few `Entity` methods that need a real world return fixed values. It also drops the two `NanoVGHelper` methods typed on `NVGColor` (OneConfig's isolated LWJGL 3), because the game class loader cannot load that class next to LWJGL 2 and `Nvg`'s proxy of the interface needs every method type.
- `Host` runs the input loops of `Minecraft.runTick` inside each 20 Hz tick, between OneConfig's `TickEvent` START and END (its `TickDelay`, and so `GuiUtils.displayScreen`, waits for them): for every mouse and key event it posts Forge's `MouseEvent`, sets `KeyBinding` state (mouse buttons are code - 100), posts FML's `InputEvent` and OneConfig's `KeyInputEvent` or `MouseInputEvent`.
- OneConfig builds its renderer classes (`NanoVGHelper`, `ScissorHelper`, `AssetHelper`, `FontHelper`, `TinyFD`) in an isolated LWJGL 3 class loader that cannot start here, so `BridgeTransformer` makes each `LwjglManager` accessor return an `Nvg.stand` proxy of the interface. Calls with no `Nvg` method of the same name and argument count do nothing. `Nvg.setupAndDraw` without the Minecraft-scale flag draws in window pixels, as NanoVG does, so it scales the Sink's GUI units by the inverse GUI scale.
- `Host` posts OneConfig's `RenderEvent` START and END around each frame: `GuiUtils` takes the click edge of the left button and the frame time for GUI animations from it.
- Rust sends whether a screen (menu, container, chat) is open. `Host` mirrors it into `Minecraft.currentScreen` with a stand-in `GuiIngameMenu` and calls `KeyBinding.unPressAllKeys()` on each change, as `displayGuiScreen` does. Events that arrived while a screen was open are not game input: no `KeyBinding` press, no `InputEvent`; only their releases clear the key bind. Polled `Keyboard` and `Mouse` state and `Keyboard.next()` still see them.
- A screen a mod opens (OneConfig's GUI) goes through `Minecraft.displayGuiScreen`, which `BridgeTransformer` replaces with `Host.displayScreen`: it closes the old screen, releases the key binds and sizes the new one. `Minecraft.setIngameFocus` (which `GuiScreen.keyTyped` calls after Escape) and `Minecraft.dispatchKeypresses` (which `GuiScreen.handleKeyboardInput` calls after each key, for F11, F2 and the Twitch keys) do nothing. `Keyboard.getEventCharacter` returns the character Rust sends with the key press, so a mod screen can type into a text field. While the mod screen is current, `Host` calls its `handleInput` and `updateScreen` each tick instead of the game input loops, so the events reach it through the patched `Keyboard` and `Mouse`, and draws it after the overlay. Rust's mirror does not replace it. Each reply frame starts with a byte that says a mod screen is current: Rust then frees the cursor, sends no game input, and lets Escape reach the screen, which closes itself as `GuiScreen.keyTyped` does.
- `Host` also posts Forge's `RenderGameOverlayEvent`, so HUD mods draw. `OverlayFilter` cancels every vanilla element, because Rust draws those. The exception is the pumpkin overlay and the hotbar: when a mod cancels its event (or `ALL`), Java keeps it, sends it as wire command `H` (`crates/modhost/src/hud.rs`), and Rust does not draw it. Rust always draws the crosshair until issue #282.
- Rust mirrors each chat line to `Chat`, which posts Forge's `ClientChatReceivedEvent` and answers keep, cancel or the changed text. `§` codes of a styled line are part of the text.
- `Mirror` takes wire message `M` (`crates/modhost/src/mirror.rs`) before a frame and puts the engine values where HUD mods read them: `Minecraft.debugFPS`, the player's position, rotation, health, `InventoryPlayer` (hotbar, main slots and armor), potion effects, the `NetworkPlayerInfo` that holds the ping, `currentServerData` and the biome that `World.getBiomeGenForCoords` returns at the player (one biome for the whole stand-in chunk).
- `HookIndex` is the hook table: after Mixin ran, its `Scanner` lists the vanilla methods that a mod changed (a body that calls a `@MixinMerged` method of its own class, or an `@Overwrite`). `Host` sends the list with its hello, after the brand; the wire format is in `crates/modhost/src/hooks.rs`.
- `Sink` collects rectangles, gradients, text, images and clip rectangles. `Host` sends them as one frame per round. The window size is in pixels and the GUI scale is the setting, so `ScaledResolution` in the JVM gives the same scale as Rust.
- OneConfig's `drawImage`, `drawRoundImage` and `drawSvg` (`Nvg`) read the file from the class path (the `Class` argument first) and send it once, by name, as a define command; each draw is an image command with its box and tint. Rust decodes a PNG as a texture and rasterizes an SVG at the size it is drawn, in the tint's color when the call has one. `ScissorHelper` scissors become clip commands: the box on the screen, cut to the scissors still in force; Rust cuts every later rectangle, gradient, image and glyph to it (glyph and image edges keep their texture and color position).

## Build

```sh
FERRO_JAVA_HOME=<Java 8 JDK> FERRO_LIB=<folder with lw.jar asm-all-5.2.jar forge.jar guava-17.0.jar> java/modhost/build.sh
```

The class path of the JVM is `java/modhost/cls`, then the libraries, then the Forge jar and the Minecraft 1.8.9 jar.
Set `FERROCRAFT_MODHOST_JAVA`, `FERROCRAFT_MODHOST_CP` and `FERROCRAFT_MODHOST_GAMEDIR` for the engine. Put the mod jars (for example Keystrokes) in `GAMEDIR/mods`.

## Test

After `build.sh`, run `java/modhost/test.sh` with the same `FERRO_JAVA_HOME` and `FERRO_LIB`; the folder also holds the Minecraft 1.8.9 jar and the other Java 8 libraries, and needs `lzma-0.0.1.jar` and `trove4j-3.0.3.jar` there too. It boots FML with the host tweakers and runs `test/` classes in place of the host entry point. `HookIndexTest` checks the hook table. `PlayerTest` runs the stand-in player through a double-tap of jump with `allowFlying`, which must toggle flight without a packet send or an exception.

Set `FERRO_MODS` to a folder of mod jars to add the mod boot test, `ModBootTest`: the same FML boot, with FML loading those jars out of `GAMEDIR/mods`. It requires the 22 twins (the `oneconfig` loader plus 21 mods), Keystrokes and `mixinbooter` (the mod ids in `test.sh` as `ferro.mods`) to be loaded and in state `AVAILABLE`, which FML sets after the mod's init and complete events ran. It needs network on a cold cache, because the OneConfig bootstrap downloads its loader. It prints the jar the Minecraft classes load from. `java/modhost-accept/run.sh <cache>` downloads the acceptance mods into `<cache>/mods`. A cache from an earlier run holds the old EvergreenHUD, PolySprint and VanillaHUD jar names: clear it before `FERRO_MODS` points at it, or FML sees each of those mods twice. The mod boot needs `FERRO_LIB` to hold asm-all 5.2 (not Forge's 5.0.3) and maven-artifact 3.3.9, which MixinBooter uses. `FERRO_LIB` must hold only that one ASM jar, because `test.sh` puts every jar of the folder on the class path; `chatblock` requires MixinBooter.

Set `FERRO_PREPARE_OUT` to the folder `java/modhost-prepare/run.sh` wrote, and both boots run on `ferro-mc-1.8.9.jar` and `ferro-forge.jar` instead of the player's client and Forge jars. The generated Minecraft jar is unsigned, so the test passes `-Dfml.ignoreInvalidMinecraftCertificates=true`.

## Limits

- The Minecraft classes are the vanilla 1.8.9 classes from the player's jar, patched at load time. `java/modhost-prepare` makes the same patched jar once, so FML then patches nothing; the stand-in bodies are not written yet.
- The stand-in player, world and options are mirrors of Rust state. Only what the HUD mods read is filled in.
- Mods that draw with GL calls other than rectangles, gradients, text, images, SVG files and clip rectangles are not forwarded yet.
- Mod screens are drawn only where the mod HUD is drawn (in a world); the window and sound handler parts of `displayGuiScreen` are skipped.
- File dialogs (`TinyFD`) and `AssetHelper` do nothing. A click on a mod card above its name row opens the mod's config page. Rounded corners draw square (also on images), `drawLine` draws horizontal and vertical lines only, and an image tint is a multiply, not NanoVG's alpha-only paint.
- `EntityRenderer.loadShader` does nothing: no shader group loads, so OneConfig's background blur never turns on.
