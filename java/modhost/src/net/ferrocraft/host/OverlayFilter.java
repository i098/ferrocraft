package net.ferrocraft.host;

import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Cancels the HUD elements Rust draws itself, after every mod has seen them. The ALL element stays
 * so GuiIngameForge posts its closing event, where HUD mods draw.
 */
public final class OverlayFilter {
    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void pre(RenderGameOverlayEvent.Pre e) {
        if (e.type != RenderGameOverlayEvent.ElementType.ALL) e.setCanceled(true);
    }
}
