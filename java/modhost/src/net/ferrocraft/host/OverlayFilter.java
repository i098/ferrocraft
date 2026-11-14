package net.ferrocraft.host;

import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Cancels the HUD elements Rust draws itself, after every mod has seen them. The ALL element stays
 * so GuiIngameForge posts its closing event, where HUD mods draw. The CHAT element stays when a mod changed the chat, see {@link ChatGui}.
 */
public final class OverlayFilter {
    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void pre(RenderGameOverlayEvent.Pre e) {
        boolean modChat = e.type == RenderGameOverlayEvent.ElementType.CHAT && ChatGui.owned();
        if (e.type != RenderGameOverlayEvent.ElementType.ALL && !modChat) e.setCanceled(true);
    }
}
