package net.ferrocraft.host;

import java.util.List;

/**
 * Everything the host asks of the mod loader. {@link Host} and {@link Minecraft} hold no loader class name; each
 * loader gives one implementation ({@link ForgeGlue} for Forge).
 */
interface Glue {
    /** The loader of this run. */
    static Glue current() {
        return ForgeGlue.INSTANCE;
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
