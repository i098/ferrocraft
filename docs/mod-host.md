# Mod host: Forge 1.8.9 on a Rust client

The mod host is a Java 8 process beside the Rust client. It is designed to run the real Forge 1.8.9 (FML + LaunchWrapper) and the mods' own Mixin library without change.
The only part we write is the Minecraft classes that sit under them: a generated stand-in jar.

![Mod host](mod-host.svg)

## ferro-prepare

Runs once on the player's machine. It reads the player's own 1.8.9 client jar and the Forge universal jar, and produces:

- `ferro-mc-1.8.9.jar` - stand-in Minecraft jar, obfuscated (notch) names, Forge patches already applied.
- `ferro-forge.jar` - the Forge universal jar without `binpatches.pack.lzma`, so FML applies no patches at boot.
- `hook-index.json` - per-method body hash and site list for the harness.

Nothing from Mojang or Forge ships with the project.

## Stand-in body tiers

Every class keeps its class shape (fields, methods, signatures, hierarchy). Bodies follow one policy:

| Tier | Policy | Classes |
|---|---|---|
| T0 | Real | Pure data and utility: `ChatComponentText`, `BlockPos`, `MathHelper`, `NBT*`, `ItemStack` data |
| T1 | Native | State mirror: `Minecraft`, `WorldClient`, `EntityPlayerSP`, `Entity*`, `NetHandlerPlayClient` |
| T2 | Real | GUI and render: `GuiScreen`, `GuiIngameForge`, `GuiNewChat`, `FontRenderer`, `Gui` |
| T3 | Native | World render: `RenderGlobal`, `EntityRenderer` (world part), entity renderers |

T2 real bodies make the most-mixed client classes and their Forge events work at once; each can later move to Rust with a harness test.

## Boot

The JVM starts like a Forge client:

```
java -Dfml.ignoreInvalidMinecraftCertificates=true \
  -classpath launchwrapper.jar:...+ferro-forge.jar:ferro-mc-1.8.9.jar:ferro-runtime.jar \
  net.minecraft.launchwrapper.Launch --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker
```

`ferro-runtime.jar` supplies `net.minecraft.client.main.Main`. It opens the channel to Rust and drives the vanilla entry points in the order Forge's patches call them (`beginMinecraftLoading`, `finishMinecraftLoading`, ...), so mods see the normal FML lifecycle.

## Mixin and ASM

Mixin reads any class bytes the class loader provides, so it should work on stand-ins unchanged. Three modes, chosen per method:

- Mode A (shape-only, default): stand-in body is native. HEAD, RETURN, TAIL, `@Accessor`, `@Shadow`, `@Overwrite` apply to the shape; CallbackInfo maps to skip-native.
- Mode B (real-on-demand): when a mod Mixin needs a body site (INVOKE, REDIRECT, MODIFY_VARIABLE), the host swaps in the real remapped body of that method before Mixin runs.
- Mode C (Rust site tables, long term): the Rust port declares the same sites as the real body; the Java handler is called there.

ASM coremods are designed to run unchanged through FML. A transformer that matches shape works on any tier; one that matches instructions needs a real body (T0, T2, or mode B). The bridge wrapper is applied last, after all coremods and Mixin.

## Events and rendering crossing to Rust

- Hook table: Rust ports are annotated `#[vanilla_method("net/minecraft/client/Minecraft.func_71411_J()V")]`. If a mod or Mixin changed that method, Rust calls Java; otherwise the flag is unset.
- Forge events (render, input, tick, chat, world, GUI) are planned to fire from Rust at the patched call sites, to be tested against the oracle's trace.
- Java draws through a small LWJGL 2 / GL shim that writes a command stream. It goes to Rust over a loopback socket, and Rust replays it on the wgpu renderer.
- Rust owns the window and feeds `Keyboard`/`Mouse` shim events so `GuiScreen.handleInput` and Forge `KeyInputEvent` are designed to work unchanged.

## Acceptance

The planned first acceptance test is the OneClient/Polyfrost 1.8.9 PvP mod set: Keystrokes, CPS, Toggle sprint, HUD/overlay mods, and OneConfig itself.
Non-rendering mods must work; stand-in classes route Gui/FontRenderer/overlay drawing to the Rust renderer.

## Main risks

- Body-dependent Mixin points need mode B then mode C.
- Mirror state must stay coherent for real bodies (write barrier + harness diff).
- LWJGL shim fidelity for mods that call odd GL functions.

Step 1 of that acceptance is a static check, `java/modhost-accept`. It lists, per mod jar, the vanilla 1.8.9 classes and `func_`/`field_` members the mod names, and checks they exist. It does not run the mods. See its README.
Step 2 runs the mods: `crates/modhost` and `java/modhost` start the JVM, load the mods and draw their rectangles, gradients, text, images, SVG files and clip rectangles on the Rust renderer. They patch the vanilla classes at load time; the generated stand-in jar is not built yet. Shown to run: Keystrokes and OneConfig.
Step 3 adds input state. Each game tick the Rust client sends the held keys and mouse buttons to the JVM. `Player` runs the vanilla `EntityPlayerSP.onLivingUpdate` input half on a stand-in player. Rust reads back the sprint and sneak state that the mods set, and sends the usual `EntityAction` packets. Shown to run: EvergreenHUD CPS counts clicks (shown live: five left clicks 150 ms apart raise the text from `CPS: 0 | 0` to `CPS: 5 | 0`, and it falls back to `0 | 0` after a second; ten clicks 110 ms apart reach `CPS: 8 | 0` because EvergreenHUD polls `Mouse.isButtonDown` once per rendered frame, as in vanilla, so a click shorter than a frame is not counted), and PolySprint keeps sprint on (the server sees `start_sprinting`). EvergreenHUD ships the CPS display off, as in vanilla: open OneConfig (Right Shift), open EvergreenHUD and click the name row of the CPS card to switch it on. Other EvergreenHUD displays that need a world (for example Biome) log errors, because the stand-in world has no chunks. Rust owns the flight toggle as vanilla does: a second jump press within 6 ticks flips flight for a player that may fly, landing ends it, and each change sends the `PlayerAbilities` packet (shown live in creative mode). Rust also moves a flying player as vanilla does: no gravity, the jump and sneak keys change vertical speed by 3 times the fly speed, and sprinting doubles the air speed (checked tick by tick against the 1.8.9 jar in the `world` tests).
