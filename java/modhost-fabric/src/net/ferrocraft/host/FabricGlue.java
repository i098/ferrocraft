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
public final class FabricGlue implements Glue {
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
     * The game directory, and what Ornithe's OSL does at the head of {@code Main.main} (its mixin there is gone with the swapped {@code Main}): the {@code init}
     * and {@code client-init} entrypoints, which register the synced registries that Bootstrap fills. No OSL, no call.
     */
    public static void initEntrypoints() throws ReflectiveOperationException {
        // The host reads the game directory from LaunchWrapper's field, which no one else sets under Knot; set here, in the host's own loader.
        Launch.minecraftHome = new java.io.File(System.getProperty("ferro.gameDir"));
        initKey("init", "net.ornithemc.osl.entrypoints.api.ModInitializer", "init");
        initKey("client-init", "net.ornithemc.osl.entrypoints.api.client.ClientModInitializer", "initClient");
    }

    private static void initKey(String key, String type, String method) throws ReflectiveOperationException {
        Class<?> c;
        try {
            c = Class.forName(type, false, Thread.currentThread().getContextClassLoader());
        } catch (ClassNotFoundException e) {
            return;
        }
        java.lang.reflect.Method m = c.getMethod(method);
        for (EntrypointContainer<?> e : FabricLoader.getInstance().getEntrypointContainers(key, c)) {
            Host.log("entrypoint " + key + ": " + e.getProvider().getMetadata().getId());
            m.invoke(e.getEntrypoint());
        }
    }

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
        // Mods of the Ornithe bundles read the block atlas while the resource manager starts; Rust owns the real one.
        R.set(mc, "field_147128_au", R.neu("net.minecraft.client.renderer.texture.TextureMap", "textures")); // textureMapBlocks
        // renderGlobal: its constructor makes GL display lists, which the host has none of; Cera registers the sky state a mixin adds to it.
        Object globe = R.alloc("net.minecraft.client.renderer.RenderGlobal");
        initMixinFields(globe);
        // startGame makes the OSL resource manager and resets it; the manager holds no pack until one is added, and the reload of the mods reads the font from it.
        // Only on Ornithe: Legacy Fabric has no OSL.
        try {
            Class.forName("net.ornithemc.osl.resource.loader.api.client.ClientResourceLoaderEvents", false, Thread.currentThread().getContextClassLoader());
            java.util.function.Consumer<Object> addVanilla = manager -> {
                Object vanilla = ((List<?>) R.get(mc, "field_110449_ao")).get(0);
                R.call(manager, "add", R.neu("net.ornithemc.osl.resource.loader.impl.adapter.WrappedResourcePack", vanilla));
            };
            R.call(R.get(R.cls("net.ornithemc.osl.resource.loader.api.client.ClientResourceLoaderEvents"), "INIT_RESOURCE_MANAGER"), "register", addVanilla);
        } catch (ClassNotFoundException e) {
            // no OSL
        }
        initMixinFields(mc); // Minecraft is allocated, not constructed: the modules that Cera adds to it
        R.set(mc, "field_71438_f", globe);
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

    /** What a constructor would do for the fields that mods add with Mixin ({@code modid$name}): a new object of the field's type, where it has a no-argument constructor. */
    private static void initMixinFields(Object o) {
        for (java.lang.reflect.Field f : o.getClass().getDeclaredFields()) {
            if (!f.getName().contains("$") || java.lang.reflect.Modifier.isStatic(f.getModifiers())) continue;
            try {
                f.setAccessible(true);
                if (f.get(o) == null) f.set(o, f.getType().getDeclaredConstructor().newInstance());
            } catch (ReflectiveOperationException | RuntimeException e) {
                Host.log("mixin field " + f.getName() + " stays null: " + e);
            }
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
