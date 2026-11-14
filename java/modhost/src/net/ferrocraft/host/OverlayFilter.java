package net.ferrocraft.host;

import java.util.HashSet;
import java.util.Set;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Decides, after every mod has seen the {@code Pre} event of a vanilla HUD element, who draws the element. Java owns it when a
 * mod cancelled the event, or when a mod changed the vanilla method that draws it and the change takes effect (see
 * {@link #changed}); Java then sends the element to Rust with {@link Sink#hud}, and Rust does not draw it. In every other case
 * Rust draws it, and the event is cancelled so that the JVM draws nothing. The ALL element stays open so GuiIngameForge posts
 * its closing event, where HUD mods draw; a mod that cancels ALL owns the whole HUD.
 *
 * <p>The bit of an element is the one of {@code crates/modhost/src/hud.rs}. Elements with no bit are always Rust's.
 */
public final class OverlayFilter {
    private static final int EVERY_BIT = 0b111;
    /** Index = bit number of crates/modhost/src/hud.rs. */
    private static final java.util.List<ElementType> OWNABLE = java.util.Arrays.asList(
            ElementType.HELMET, ElementType.CROSSHAIRS, ElementType.HOTBAR);
    private static final String SHOW_CROSSHAIR = "net/minecraft/client/gui/GuiIngame.func_175183_b()Z";

    private final Set<String> hooks = new HashSet<>(HookIndex.list());

    @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
    public void pre(RenderGameOverlayEvent.Pre e) {
        if (e.type == ElementType.ALL) {
            if (e.isCanceled()) Sink.hud(EVERY_BIT);
            return;
        }
        int bit = bit(e.type);
        if (bit != 0 && (e.isCanceled() || changed(e.type))) {
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

    /**
     * Whether a mod changed the vanilla method of the element so that vanilla would draw it differently. Only the crosshair has
     * such a method: {@code GuiIngame.showCrosshair}. The JVM cannot draw the vanilla crosshair texture, so the element goes to
     * Java only when the changed method hides it. Where it answers true vanilla draws the crosshair and Rust draws the same.
     */
    private boolean changed(ElementType type) {
        if (type != ElementType.CROSSHAIRS || !hooks.contains(SHOW_CROSSHAIR)) return false;
        Object mc = R.call(R.cls("net.minecraft.client.Minecraft"), "func_71410_x");
        return !Boolean.TRUE.equals(R.call(R.get(mc, "field_71456_v"), "func_175183_b"));
    }
}
