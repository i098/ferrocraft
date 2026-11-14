package net.ferrocraft.host;

import java.util.List;

/** {@link Glue} for Forge 1.8.9: MinecraftForge's event bus, FMLCommonHandler and FMLClientHandler. */
final class ForgeGlue implements Glue {
    static final ForgeGlue INSTANCE = new ForgeGlue();

    private ForgeGlue() {}

    private static Object fml() {
        return R.call(R.cls("net.minecraftforge.fml.common.FMLCommonHandler"), "instance");
    }

    @Override
    public void halt(int code) {
        R.call(fml(), "exitJava", code, true);
    }

    /** Without an active mod container Forge logs an error and uses the Minecraft one. */
    @Override
    public void register(Object handler) {
        R.call(R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS"), "register", handler);
    }

    @Override
    public void registerOverlayFilter() {
        register(new OverlayFilter());
    }

    @Override
    public void loadMods(Object mc, List<Object> packs, Object resources) {
        Object handler = R.call(R.cls("net.minecraftforge.fml.client.FMLClientHandler"), "instance");
        R.call(handler, "beginMinecraftLoading", mc, packs, resources);
        R.call(handler, "finishMinecraftLoading");
    }

    /** Rust draws the vanilla HUD elements; the helmet, crosshair and hotbar flags stay on so their Pre events fire for mods. */
    @Override
    public Object newIngameGui(Object mc) {
        Class<?> gui = R.cls("net.minecraftforge.client.GuiIngameForge");
        for (String flag : new String[] {"renderPortal", "renderBossHealth", "renderHealth", "renderArmor", "renderFood", "renderHealthMount", "renderAir", "renderJumpBar", "renderExperiance", "renderObjective"}) {
            R.set(gui, flag, false);
        }
        return R.neu("net.minecraftforge.client.GuiIngameForge", mc);
    }

    @Override
    public void preClientTick() {
        R.call(fml(), "onPreClientTick");
    }

    @Override
    public void postClientTick() {
        R.call(fml(), "onPostClientTick");
    }

    @Override
    public void renderTickStart(float partial) {
        R.call(fml(), "onRenderTickStart", partial);
    }

    @Override
    public void renderTickEnd(float partial) {
        R.call(fml(), "onRenderTickEnd", partial);
    }

    @Override
    public boolean mouseEventCancelled() {
        return Boolean.TRUE.equals(R.call(R.cls("net.minecraftforge.client.ForgeHooksClient"), "postMouseEvent"));
    }

    @Override
    public void mouseInput(int button, boolean down) {
        R.call(fml(), "fireMouseInput");
        postOneConfig("MouseInputEvent", button, down);
    }

    @Override
    public void keyInput(int key, boolean down) {
        R.call(fml(), "fireKeyInput");
        postOneConfig("KeyInputEvent", key, down);
    }

    /**
     * OneConfig's own input events, which its mixin posts from runTick. The constructor is filled by
     * parameter type: an int is the key or button, a boolean the state.
     */
    private static void postOneConfig(String event, int code, boolean down) {
        String cls = "cc.polyfrost.oneconfig.events.event." + event;
        Class<?>[] types = R.cls(cls).getDeclaredConstructors()[0].getParameterTypes();
        Object[] args = new Object[types.length];
        for (int i = 0; i < types.length; i++) {
            args[i] = types[i] == int.class ? (Object) code : types[i] == boolean.class ? (Object) down : null;
        }
        R.call(R.get(R.cls("cc.polyfrost.oneconfig.events.EventManager"), "INSTANCE"), "post", R.neu(cls, args));
    }
}
