package net.ferrocraft.host;

import java.util.List;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.entrypoint.EntrypointContainer;
import net.fabricmc.loader.impl.FabricLoaderImpl;
import net.minecraft.launchwrapper.Launch;

/**
 * {@link Glue} for Fabric (Legacy Fabric 1.8.9 on Knot). Fabric Loader has no event bus and no client handler: the mods hook
 * the game with Mixin, so the tick, input and render calls of {@link Glue} have nothing to call here. The hooks that a mod
 * put in the loop methods run through the shells of {@link ShellTransformer} instead.
 */
final class FabricGlue implements Glue {
    static final FabricGlue INSTANCE = new FabricGlue();

    private FabricGlue() {}

    /** Knot does not trap exits. */
    @Override
    public void halt(int code) {
        System.exit(code);
    }

    /** Nothing to register: there is no event bus to put a handler on. */
    @Override
    public void register(Object handler) {}

    /** Rust draws the vanilla HUD elements, and Fabric has no overlay events for a filter to cancel. */
    @Override
    public void registerOverlayFilter() {}

    /**
     * What the entrypoint patch of Knot does in {@code Minecraft.<init>}, which the host does not run: it calls {@code prepareModInit}, which sets the game instance
     * (Legacy Fabric API's key binds read it), then the {@code main} entrypoints, then the {@code client} ones. {@code preLaunch} already ran before the swapped {@code Main}.
     */
    @Override
    public void loadMods(Object mc, List<Object> packs, Object resources) {
        FabricLoaderImpl.INSTANCE.prepareModInit(Launch.minecraftHome.toPath(), mc);
        // startGame builds the item renderer; its body is dropped here, and Legacy Fabric API's startGame handler registers item models into it.
        // The model manager is empty: Rust draws the items, the registrations only need a registry to land in.
        Object models = R.alloc("net.minecraft.client.resources.model.ModelManager");
        R.set(models, "field_174958_a", R.neu("net.minecraft.util.RegistrySimple")); // modelRegistry
        R.set(mc, "field_175621_X", R.neu("net.minecraft.client.renderer.entity.RenderItem", R.get(mc, "field_71446_o"), models)); // renderItem
        FabricLoader loader = FabricLoader.getInstance();
        for (EntrypointContainer<ModInitializer> c : loader.getEntrypointContainers("main", ModInitializer.class)) {
            Host.log("entrypoint main: " + c.getProvider().getMetadata().getId());
            c.getEntrypoint().onInitialize();
        }
        for (EntrypointContainer<ClientModInitializer> c : loader.getEntrypointContainers("client", ClientModInitializer.class)) {
            Host.log("entrypoint client: " + c.getProvider().getMetadata().getId());
            c.getEntrypoint().onInitializeClient();
        }
    }

    @Override
    public Object newIngameGui(Object mc) {
        return R.neu("net.minecraft.client.gui.GuiIngame", mc);
    }

    @Override
    public void preClientTick() {}

    @Override
    public void postClientTick() {}

    @Override
    public void renderTickStart(float partial) {}

    @Override
    public void renderTickEnd(float partial) {}

    @Override
    public boolean mouseEventCancelled() {
        return false;
    }

    @Override
    public void mouseInput(int button, boolean down) {}

    @Override
    public void keyInput(int key, boolean down) {}
}
