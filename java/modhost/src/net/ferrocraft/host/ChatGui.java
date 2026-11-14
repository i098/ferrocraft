package net.ferrocraft.host;

import java.util.List;

/**
 * Java keeps the chat when a mod changed it. Vanilla chat is {@code GuiNewChat}, {@code GuiChat} and the chat packet
 * handler of {@code NetHandlerPlayClient}. When the hook table holds a method of one of them, Rust no longer prints or
 * draws the lines: {@link Chat} runs the real {@code NetHandlerPlayClient.handleChat} on each received line, which posts
 * Forge's event and prints to the real {@code GuiNewChat}, where the mods' code runs, and {@link #render} lets the CHAT
 * overlay draw it through the draw commands. Rust still reads the keys and sends the typed line; it tells this class
 * when its input line opens and closes.
 *
 * <p>Not run: {@code EntityPlayerSP.sendChatMessage}. A mod that filters what the player sends does not see the typed
 * line, because Rust sends it.
 */
final class ChatGui {
    private static final long TICK_NANOS = 50_000_000L;
    private static final String[] CHANGED = {
        "net/minecraft/client/gui/GuiNewChat.",
        "net/minecraft/client/gui/GuiChat.",
        "net/minecraft/client/network/NetHandlerPlayClient.func_147251_a(", // handleChat
    };
    private static Boolean owned;

    private Object stand;
    private Object handler;
    private boolean open;
    private long lastTick = System.nanoTime();

    /** Whether a hook of {@code hooks} (the lines of {@link HookIndex#list}) changes the chat. */
    static boolean owns(List<String> hooks) {
        for (String hook : hooks) {
            for (String prefix : CHANGED) {
                if (hook.startsWith(prefix)) return true;
            }
        }
        return false;
    }

    /** Whether Java keeps the chat; fixed once the hook table is complete, which is before the first line. */
    static synchronized boolean owned() {
        if (owned == null) {
            owned = owns(HookIndex.list());
            if (owned) Host.log("a mod changed the chat: Java keeps and draws it");
        }
        return owned;
    }

    static synchronized void own(boolean value) {
        owned = value;
    }

    /**
     * What the server's chat packet does: {@code NetHandlerPlayClient.handleChat}, on a handler that has only the
     * Minecraft singleton. Its thread check passes because this thread is the game thread.
     */
    void handle(Object mc, int type, Object message) {
        if (handler == null) {
            handler = R.alloc("net.minecraft.client.network.NetHandlerPlayClient");
            R.set(handler, "field_147299_f", mc); // gameController
            for (java.lang.reflect.Field f : mc.getClass().getDeclaredFields()) {
                if (f.getType() == Thread.class && !java.lang.reflect.Modifier.isStatic(f.getModifiers())) {
                    R.set(mc, f.getName(), Thread.currentThread()); // mcThread
                }
            }
        }
        R.call(handler, "func_147251_a", R.neu("net.minecraft.network.play.server.S02PacketChat", message, (byte) type));
    }

    void setOpen(boolean value) {
        open = value;
    }

    /**
     * The overlay as GuiIngameForge draws it. With the chat open, {@code currentScreen} is a GuiChat while the overlay
     * runs, which is what {@code GuiNewChat.getChatOpen} reads; and the HUD tick counter runs, which fades the lines.
     */
    void render(Object mc, float partial) {
        if (!owned()) {
            Minecraft.renderOverlay(mc, partial);
            return;
        }
        Object ingame = R.get(mc, "field_71456_v");
        long ticks = (System.nanoTime() - lastTick) / TICK_NANOS;
        lastTick += ticks * TICK_NANOS;
        R.set(ingame, "field_73837_f", (Integer) R.get(ingame, "field_73837_f") + (int) ticks); // updateCounter
        Object before = R.get(mc, "field_71462_r"); // currentScreen
        if (open) {
            if (stand == null) stand = R.alloc("net.minecraft.client.gui.GuiChat");
            R.set(mc, "field_71462_r", stand);
        }
        try {
            Minecraft.renderOverlay(mc, partial);
        } finally {
            if (open) R.set(mc, "field_71462_r", before);
        }
    }
}
