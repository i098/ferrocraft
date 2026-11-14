package net.ferrocraft.host;

import java.util.List;

/**
 * Everything the host asks of the mod loader. {@link Host} and {@link Minecraft} hold no loader class name; each
 * loader gives one implementation ({@link ForgeGlue} for Forge).
 */
interface Glue {
    /** The loader of this run: Fabric when Knot started it, else Forge. {@code FabricGlue} is in the Fabric host jar only. */
    static Glue current() {
        ClassLoader loader = Thread.currentThread().getContextClassLoader();
        try {
            Class.forName("net.fabricmc.loader.api.FabricLoader", false, loader);
        } catch (ClassNotFoundException e) {
            return ForgeGlue.INSTANCE;
        }
        try {
            java.lang.reflect.Field instance = Class.forName("net.ferrocraft.host.FabricGlue", true, loader).getDeclaredField("INSTANCE");
            instance.setAccessible(true);
            return (Glue) instance.get(null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("Fabric Loader is here but FabricGlue is not: the class path needs the Fabric host jar", e);
        }
    }

    /** Forge's event bus, or null on a loader without one (Fabric): what the host would post to it, it skips there. */
    static Object forgeBus() {
        return current() == ForgeGlue.INSTANCE ? R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS") : null;
    }

    /** FML's event bus, or null as {@link #forgeBus}. */
    static Object fmlBus() {
        return current() == ForgeGlue.INSTANCE ? R.call(R.call(R.cls("net.minecraftforge.fml.common.FMLCommonHandler"), "instance"), "bus") : null;
    }

    /** Ends the JVM; the loader may trap every other exit. */
    void halt(int code);

    /** Registers an event handler of the host with the loader's event bus. */
    void register(Object handler);

    /** Registers the filter of the vanilla HUD elements that Java owns, see {@link OverlayFilter}. */
    void registerOverlayFilter();

    /** Starts the loader's mods in the order Minecraft.<init> and startGame call it. */
    void loadMods(Object mc, List<Object> packs, Object resources);

    /** The in-game GUI object, with Rust's HUD elements switched off, see {@link Minecraft}. */
    Object newIngameGui(Object mc);

    void preClientTick();

    void postClientTick();

    void renderTickStart(float partial);

    void renderTickEnd(float partial);

    /** The mouse event of the input queue; true if a mod cancelled it. */
    boolean mouseEventCancelled();

    /** The mouse event ({@code button}, with its state) and the key event, for mods that listen to the loader's input events. */
    void mouseInput(int button, boolean down);

    void keyInput(int key, boolean down);
}
