package net.ferrocraft.host;

import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Decides, after every mod has seen the {@code Pre} event of a vanilla HUD element, who draws the element. Java owns it when a
 * mod cancelled the event; Java then sends the element to Rust with {@link Sink#hud}, and Rust does not draw it. In every other case
 * Rust draws it, and the event is cancelled so that the JVM draws nothing. The ALL element stays open so GuiIngameForge posts
 * its closing event, where HUD mods draw; a mod that cancels ALL owns the whole HUD. The CHAT element stays when a mod changed the chat, see {@link ChatGui}.
 *
 * <p>The bit of an element is the one of {@code crates/modhost/src/hud.rs}. Elements with no bit are always Rust's, the crosshair too until issue #282 lets Java draw the vanilla texture.
 */
public final class OverlayFilter {
    private static final int EVERY_BIT = 0b11;
    /** Index = bit number of crates/modhost/src/hud.rs. */
    private static final java.util.List<ElementType> OWNABLE = java.util.Arrays.asList(
            ElementType.HELMET, ElementType.HOTBAR);

    @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
    public void pre(RenderGameOverlayEvent.Pre e) {
        boolean modChat = e.type == ElementType.CHAT && ChatGui.owned();
        if (e.type == ElementType.ALL) {
            if (e.isCanceled()) Sink.hud(EVERY_BIT);
            return;
        }
        if (modChat) return;
        int bit = bit(e.type);
        if (bit != 0 && e.isCanceled()) {
            Sink.hud(bit);
        } else {
            e.setCanceled(true);
        }
    }

    /** The wire bit of an element Rust can leave out, or 0. */
    private static int bit(ElementType type) {
        int i = OWNABLE.indexOf(type);
        return i < 0 ? 0 : 1 << i;
    }
}
