package net.ferrocraft.host;

import java.io.File;
import java.util.ArrayList;
import java.util.List;

/**
 * The Minecraft singleton that mods see, built without Minecraft's own startup (no window, no GL,
 * no server connection). Rust owns those; this holds what mods read: settings, resources, fonts, size.
 */
final class Minecraft {
    private static List<Object> packs;
    private static Object resources;
    private static Object screen;

    private Minecraft() {}

    static Object create(File gameDir) {
        R.call(R.cls("net.minecraft.init.Bootstrap"), "func_151354_b"); // register blocks and items
        Class<?> mcClass = R.cls("net.minecraft.client.Minecraft");
        Object mc = R.alloc("net.minecraft.client.Minecraft");
        R.set(mcClass, "field_71432_P", mc); // theMinecraft
        R.set(mc, "field_71412_D", gameDir); // mcDataDir

        Object settings = R.neu("net.minecraft.client.settings.GameSettings");
        R.set(mc, "field_71474_y", settings); // gameSettings

        Object metadata = R.neu("net.minecraft.client.resources.data.IMetadataSerializer");
        R.set(mc, "field_110452_an", metadata);
        resources = R.neu("net.minecraft.client.resources.SimpleReloadableResourceManager", metadata);
        Object vanilla = R.neu("net.minecraft.client.resources.DefaultResourcePack", new java.util.HashMap<String, File>());
        R.call(resources, "func_110545_a", vanilla); // reloadResourcePack
        packs = new ArrayList<>();
        packs.add(vanilla);
        R.set(mc, "field_110449_ao", packs); // defaultResourcePacks
        R.set(mc, "field_110451_am", resources); // mcResourceManager
        Object repository = R.neu("net.minecraft.client.resources.ResourcePackRepository", new File(gameDir, "resourcepacks"), new File(gameDir, "server-resource-packs"), vanilla, metadata, settings);
        R.set(mc, "field_110448_aq", repository); // mcResourcePackRepository

        Object languages = R.neu("net.minecraft.client.resources.LanguageManager", metadata, "en_US");
        R.call(resources, "func_110542_a", languages); // registerReloadListener
        R.set(mc, "field_135017_as", languages); // mcLanguageManager
        Object textures = R.neu("net.minecraft.client.renderer.texture.TextureManager", resources);
        R.set(mc, "field_71446_o", textures); // renderEngine
        Object ascii = R.neu("net.minecraft.util.ResourceLocation", "textures/font/ascii.png");
        Object font = R.neu("net.minecraft.client.gui.FontRenderer", settings, ascii, textures, false);
        R.set(mc, "field_71466_p", font); // fontRendererObj
        // Entity renderers are Rust's; mods only register theirs into this map.
        Object renderManager = R.alloc("net.minecraft.client.renderer.entity.RenderManager");
        R.set(renderManager, "field_78729_o", new java.util.HashMap<Object, Object>()); // entityRenderMap
        R.set(mc, "field_175616_W", renderManager);
        mirrorState(mc, settings);
        return mc;
    }

    /**
     * Stand-in player, world and HUD objects. Rust owns the real state; mods read these through
     * Minecraft.getMinecraft(), and GuiIngameForge.renderGameOverlay needs them to run.
     */
    private static void mirrorState(Object mc, Object settings) {
        R.set(settings, "field_74347_j", false); // fancyGraphics: the vignette reads the world border
        R.set(settings, "field_92117_D", false); // heldItemTooltips: the tooltip code needs a full player controller
        R.set(mc, "field_71424_I", R.neu("net.minecraft.profiler.Profiler")); // mcProfiler
        Object world = R.alloc("net.minecraft.client.multiplayer.WorldClient");
        R.set(world, "field_96442_D", R.neu("net.minecraft.scoreboard.Scoreboard")); // worldScoreboard
        R.set(mc, "field_71441_e", world); // theWorld
        Object player = R.alloc("net.minecraft.client.entity.EntityPlayerSP");
        R.set(player, "field_146106_i", R.neu("com.mojang.authlib.GameProfile", java.util.UUID.nameUUIDFromBytes("ferrocraft".getBytes()), "Player")); // gameProfile
        R.set(player, "field_71100_bB", R.neu("net.minecraft.util.FoodStats")); // foodStats
        R.set(mc, "field_71439_g", player); // thePlayer
        R.set(mc, "field_175622_Z", player); // renderViewEntity
        Object controller = R.alloc("net.minecraft.client.multiplayer.PlayerControllerMP");
        R.set(controller, "field_78779_k", R.get(R.cls("net.minecraft.world.WorldSettings$GameType"), "SURVIVAL")); // currentGameType
        R.set(mc, "field_71442_b", controller); // playerController
        Object er = R.alloc("net.minecraft.client.renderer.EntityRenderer");
        R.set(er, "field_78531_r", mc); // EntityRenderer.mc: setupOverlayRendering reads the window size from it
        R.set(mc, "field_71460_t", er); // entityRenderer
        // Rust draws the vanilla HUD elements; only the overlay events must fire for mods.
        Class<?> forgeGui = R.cls("net.minecraftforge.client.GuiIngameForge");
        for (String flag : new String[] {"renderHelmet", "renderPortal", "renderHotbar", "renderCrosshairs", "renderBossHealth", "renderHealth", "renderArmor", "renderFood", "renderHealthMount", "renderAir", "renderJumpBar", "renderExperiance", "renderObjective"}) {
            R.set(forgeGui, flag, false);
        }
        R.set(mc, "field_71456_v", R.neu("net.minecraftforge.client.GuiIngameForge", mc)); // ingameGUI
    }

    /** What Minecraft.<init> and startGame call in Forge's patched order. */
    static void loadMods(Object mc) {
        Object fml = R.call(R.cls("net.minecraftforge.fml.client.FMLClientHandler"), "instance");
        R.call(fml, "beginMinecraftLoading", mc, packs, resources);
        R.call(fml, "finishMinecraftLoading");
    }

    static void renderOverlay(Object mc, float partial) {
        R.call(R.get(mc, "field_71456_v"), "func_175180_a", partial); // GuiIngame.renderGameOverlay
    }

    /**
     * Mirrors Rust's open screen into Minecraft.currentScreen, which mods read, and releases every
     * key bind as Minecraft.displayGuiScreen does. The stand-in screen has no behaviour; a screen a
     * mod opened stays.
     */
    static void setScreen(Object mc, boolean open) {
        Object current = R.get(mc, "field_71462_r"); // currentScreen
        if (current != null && current != screen) return;
        if (open && screen == null) screen = R.alloc("net.minecraft.client.gui.GuiIngameMenu");
        R.set(mc, "field_71462_r", open ? screen : null);
        R.call(R.cls("net.minecraft.client.settings.KeyBinding"), "func_74506_a"); // unPressAllKeys
    }

    /** The screen a mod opened, or null: the stand-in for Rust's screens does not count. */
    static Object modScreen(Object mc) {
        Object current = R.get(mc, "field_71462_r");
        return current == screen ? null : current;
    }

    /** Minecraft.displayGuiScreen: close the old screen, release the key binds, size the new one. */
    static void displayScreen(Object next) {
        Object mc = R.get(R.cls("net.minecraft.client.Minecraft"), "field_71432_P");
        Object old = R.get(mc, "field_71462_r");
        if (old != null && old != screen) R.call(old, "func_146281_b"); // GuiScreen.onGuiClosed
        R.set(mc, "field_71462_r", next);
        if (next == null) return;
        R.call(R.cls("net.minecraft.client.settings.KeyBinding"), "func_74506_a");
        Object size = R.neu("net.minecraft.client.gui.ScaledResolution", mc);
        R.call(next, "func_146280_a", mc, R.call(size, "func_78326_a"), R.call(size, "func_78328_b")); // setWorldAndResolution
    }

    /** What runGameLoop does for a screen: draw it with the mouse in scaled GUI units. */
    static void drawScreen(Object mc, Object modScreen, float partial) {
        Object size = R.neu("net.minecraft.client.gui.ScaledResolution", mc);
        int w = (Integer) R.call(size, "func_78326_a");
        int h = (Integer) R.call(size, "func_78328_b");
        int mx = Input.mouseX() * w / Input.width();
        int my = h - Input.mouseY() * h / Input.height() - 1;
        R.call(modScreen, "func_73863_a", mx, my, partial); // GuiScreen.drawScreen
    }

    static void resize(Object mc, int w, int h, int scale, boolean unicode) {
        R.set(mc, "field_71443_c", w); // displayWidth
        R.set(mc, "field_71440_d", h); // displayHeight
        Object settings = R.get(mc, "field_71474_y");
        R.set(settings, "field_74335_Z", scale); // guiScale
        R.set(settings, "field_151455_aw", unicode); // forceUnicodeFont
    }
}
