package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.List;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.Chat;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.client.event.RenderGameOverlayEvent.Text;
import net.minecraftforge.fml.common.eventhandler.Event;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.IEventListener;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Decides, after every mod has seen the {@code Pre} event of a vanilla HUD element, who draws the element. Java owns it when a
 * mod cancelled the event; Java then sends the element to Rust with {@link Sink#hud}, and Rust does not draw it. In every other case
 * Rust draws it, and the event is cancelled so that the JVM draws nothing. The ALL element stays open so GuiIngameForge posts
 * its closing event, where HUD mods draw; a mod that cancels ALL owns the whole HUD. The CHAT element stays when a mod changed the chat, see {@link ChatGui}.
 *
 * <p>The bit of an element is the one of {@code crates/modhost/src/hud.rs}. Elements with no bit are always Rust's. A mod crosshair (PolyCrosshair) draws a {@code DynamicTexture} with {@code Gui.drawScaledCustomSizeModalRect}; {@link GlState} sends that quad to Rust as an image.
 */
public final class OverlayFilter {
    private static final int EVERY_BIT = 0b11111;
    /** Index = bit number of crates/modhost/src/hud.rs. */
    private static final java.util.List<ElementType> OWNABLE = java.util.Arrays.asList(
            ElementType.HELMET, ElementType.HOTBAR, ElementType.DEBUG, ElementType.PLAYER_LIST, ElementType.CROSSHAIRS);

    @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
    public void pre(RenderGameOverlayEvent.Pre e) {
        boolean modChat = e.type == ElementType.CHAT && ChatGui.owned();
        if (e.type == ElementType.ALL) {
            Sink.crosshairDrawn(0);
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

    public static void invoke(IEventListener listener, Event event) {
        if (!OverlayHooks.active() || !(event instanceof RenderGameOverlayEvent) || listener instanceof EventPriority) {
            listener.invoke(event);
            return;
        }
        boolean canceled = event.isCancelable() && event.isCanceled();
        Chat chat = event instanceof Chat ? (Chat) event : null;
        int x = chat == null ? 0 : chat.posX;
        int y = chat == null ? 0 : chat.posY;
        Text text = event instanceof Text ? (Text) event : null;
        List<String> left = text == null ? null : new ArrayList<>(text.left);
        List<String> right = text == null ? null : new ArrayList<>(text.right);
        Sink.Mark mark = OverlayHooks.checkpoint();
        try {
            listener.invoke(event);
        } catch (Exception failure) {
            if (event.isCancelable()) event.setCanceled(canceled);
            if (chat != null) {
                chat.posX = x;
                chat.posY = y;
            }
            if (text != null) {
                text.left.clear();
                text.left.addAll(left);
                text.right.clear();
                text.right.addAll(right);
            }
            OverlayHooks.failed(failure, mark);
            return;
        }
        ElementType type = ((RenderGameOverlayEvent) event).type;
        if (type == ElementType.CROSSHAIRS) {
            Sink.crosshairDrawn(mark.textures);
        }
    }

    /** The wire bit of an element Rust can leave out, or 0. */
    private static int bit(ElementType type) {
        int i = OWNABLE.indexOf(type);
        return i < 0 ? 0 : 1 << i;
    }
}
