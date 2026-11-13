package net.ferrocraft.host;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import net.minecraft.launchwrapper.Launch;

/**
 * Boots Forge's FML around Minecraft state that Rust owns, then answers one frame message at a time.
 *
 * <p>Hello (Java to Rust): 'H', the token, then the brand of ClientBrandRetriever.getClientModName() as writeUTF.
 * Frame in (Rust to Java): 'F', i32 width, i32 height, i32 guiScale, u8 forceUnicode, u8 screenOpen,
 * i32 mouseX, i32 mouseY, u16 event count, then the key and mouse events of wire.rs. 'P' is a
 * player tick request, see {@link Player}. 'C' is a chat line, see {@link Chat}. 'M' is the HUD state mirror, see {@link Mirror}, which comes before a frame. 'O' asks for the world values mods change, see {@link Overrides}. Any other first byte ends the host.
 * Frame out: the bytes of {@link Sink#take(boolean)}: whether a screen a mod opened is current, then the draw commands.
 */
public final class Host {
    private static final long TICK_NANOS = 50_000_000L;
    private static final int MAX_TICKS = 10;

    private final Object mc;
    private final Object forgeBus;
    private final Object fml;
    private long lastTick = System.nanoTime();
    private final Player player;
    private final Overrides overrides;
    private final Chat chat = new Chat();
    private final Mirror mirror;
    private boolean screenOpen;

    private Host(Object mc) {
        this.mc = mc;
        this.forgeBus = R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS");
        this.fml = R.call(R.cls("net.minecraftforge.fml.common.FMLCommonHandler"), "instance");
        this.player = new Player(mc, R.get(mc, "field_71439_g"), R.get(mc, "field_71474_y"));
        this.overrides = new Overrides(mc);
        this.mirror = new Mirror(mc, R.get(mc, "field_71439_g"));
    }

    /** Entry point; the JVM never exits on its own because mods start non-daemon threads. */
    public static void run(String[] args) {
        try {
            start();
        } catch (Throwable t) {
            t.printStackTrace();
            halt(1);
        }
        halt(0);
    }

    /** Registers a Forge event handler; without an active mod container Forge logs an error and uses the Minecraft one. */
    private static void registerAs(Object handler) {
        R.call(R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS"), "register", handler);
    }

    /**
     * OneConfig starts from mixins in Minecraft.startGame, which the host does not run. This does the same:
     * the start event, then OneConfig.init and the initialization event that registers its HUD.
     */
    private static void startOneConfig() {
        Object events = R.get(R.cls("cc.polyfrost.oneconfig.events.EventManager"), "INSTANCE");
        R.call(events, "post", R.neu("cc.polyfrost.oneconfig.events.event.StartEvent"));
        R.call(R.get(R.cls("cc.polyfrost.oneconfig.internal.OneConfig"), "INSTANCE"), "init");
        R.call(events, "post", R.neu("cc.polyfrost.oneconfig.events.event.InitializationEvent"));
    }

    /** FML's security manager traps every exit except the one from its own handler. */
    static void halt(int code) {
        R.call(R.call(R.cls("net.minecraftforge.fml.common.FMLCommonHandler"), "instance"), "exitJava", code, true);
    }

    private static void start() throws Exception {

        Object mc = Minecraft.create(Launch.minecraftHome);
        log("minecraft state ready");
        Minecraft.loadMods(mc);
        log("mods loaded");
        registerAs(new OverlayFilter());
        startOneConfig();

        Host host = new Host(mc);
        String bridge = System.getProperty("ferro.bridge");
        int colon = bridge.lastIndexOf(':');
        try (Socket s = new Socket(bridge.substring(0, colon), Integer.parseInt(bridge.substring(colon + 1)))) {
            s.setTcpNoDelay(true);
            host.serve(new DataInputStream(new BufferedInputStream(s.getInputStream())), new DataOutputStream(new BufferedOutputStream(s.getOutputStream())));
        }
    }

    static void log(String s) {
        System.out.println("[ferro-host] " + s);
    }

    private void serve(DataInputStream in, DataOutputStream out) throws IOException {
        out.writeByte('H');
        out.write(System.getenv("FERROCRAFT_BRIDGE_TOKEN").getBytes(StandardCharsets.US_ASCII));
        out.writeUTF((String) R.call(R.cls("net.minecraft.client.ClientBrandRetriever"), "getClientModName"));
        out.flush();
        while (true) {
            int tag = in.read();
            if (tag == 'P') {
                player.tick(in, out);
                continue;
            }
            if (tag == 'O') {
                overrides.answer(in, out);
                continue;
            }
            if (tag == 'C') {
                chat.line(in, out);
                continue;
            }
            if (tag == 'M') {
                mirror.apply(in);
                continue;
            }
            if (tag != 'F') return;
            int w = in.readInt();
            int h = in.readInt();
            int scale = in.readInt();
            boolean unicode = in.readUnsignedByte() != 0;
            boolean screen = in.readUnsignedByte() != 0;
            Input.set(w, h, in.readInt(), in.readInt());
            int n = in.readUnsignedShort();
            for (int i = 0; i < n; i++) {
                int kind = in.readUnsignedByte();
                int arg = in.readInt();
                Input.event(kind, arg, in.readUnsignedByte(), in.readUnsignedShort());
            }
            frame(w, h, scale, unicode, screen);
            out.write(Sink.take(Minecraft.modScreen(mc) != null));
            out.flush();
        }
    }

    private void frame(int w, int h, int scale, boolean unicode, boolean screen) {
        Minecraft.resize(mc, w, h, scale, unicode);
        if (screen != screenOpen) {
            screenOpen = screen;
            Minecraft.setScreen(mc, screen);
        }
        long t = System.nanoTime();
        int ticks = (int) Math.min(MAX_TICKS, (t - lastTick) / TICK_NANOS);
        lastTick += ticks * TICK_NANOS;
        if (t - lastTick > MAX_TICKS * TICK_NANOS) lastTick = t;
        for (int i = 0; i < ticks; i++) {
            postTick("START");
            R.call(fml, "onPreClientTick");
            Object modScreen = Minecraft.modScreen(mc);
            if (modScreen != null) {
                R.call(modScreen, "func_146269_k"); // GuiScreen.handleInput
                R.call(modScreen, "func_73876_c"); // GuiScreen.updateScreen
            } else {
                pollMouse();
                pollKeyboard();
            }
            R.call(fml, "onPostClientTick");
            postTick("END");
        }
        float partial = Math.min(1f, (t - lastTick) / (float) TICK_NANOS);
        renderOverlay(partial);
    }

    /**
     * The mouse loop of Minecraft.runTick: Forge's MouseEvent, key binds, then the input events. Events
     * that arrived with a screen open belong to the screen, so the game only sees their releases.
     */
    private void pollMouse() {
        while (Input.nextMouse()) {
            boolean down = Input.eventButtonDown();
            int button = Input.eventButton() - 100;
            if (Input.mouseInScreen()) {
                if (!down) keyBind(button, false);
                continue;
            }
            if (Boolean.TRUE.equals(R.call(R.cls("net.minecraftforge.client.ForgeHooksClient"), "postMouseEvent"))) continue;
            keyBind(button, down);
            R.call(fml, "fireMouseInput");
            postOneConfig("MouseInputEvent", Input.eventButton(), down);
        }
    }

    /** The keyboard loop of Minecraft.runTick; like the mouse loop, it hides events of an open screen. */
    private void pollKeyboard() {
        while (Input.nextKey()) {
            int key = Input.eventKey();
            boolean down = Input.eventKeyDown();
            if (Input.keyInScreen()) {
                if (!down) keyBind(key, false);
                continue;
            }
            keyBind(key, down);
            R.call(fml, "fireKeyInput");
            postOneConfig("KeyInputEvent", key, down);
        }
    }

    /** OneConfig's TickEvent, which its mixin posts at the head and the return of runTick; its TickDelay, and so GuiUtils.displayScreen, waits for it. */
    private static void postTick(String stage) {
        Object start = R.get(R.cls("cc.polyfrost.oneconfig.events.event.Stage"), stage);
        R.call(R.get(R.cls("cc.polyfrost.oneconfig.events.EventManager"), "INSTANCE"), "post", R.neu("cc.polyfrost.oneconfig.events.event.TickEvent", start));
    }

    /** KeyBinding.setKeyBindState, and KeyBinding.onTick on a press; mouse buttons are code - 100. */
    private static void keyBind(int code, boolean down) {
        Class<?> keyBinding = R.cls("net.minecraft.client.settings.KeyBinding");
        R.call(keyBinding, "func_74510_a", code, down);
        if (down) R.call(keyBinding, "func_74507_a", code);
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

    /** OneConfig's RenderEvent, which its mixin posts around each frame; GuiUtils reads the mouse button and the frame time from it. */
    private static void postRender(String stage, float partial) {
        Object s = R.get(R.cls("cc.polyfrost.oneconfig.events.event.Stage"), stage);
        R.call(R.get(R.cls("cc.polyfrost.oneconfig.events.EventManager"), "INSTANCE"), "post", R.neu("cc.polyfrost.oneconfig.events.event.RenderEvent", s, partial));
    }

    /** GuiIngameForge.renderGameOverlay, which posts the overlay events and runs mixins hooked into it. */
    private void renderOverlay(float partial) {
        postRender("START", partial);
        R.call(fml, "onRenderTickStart", partial);
        Minecraft.renderOverlay(mc, partial);
        Object modScreen = Minecraft.modScreen(mc);
        if (modScreen != null) Minecraft.drawScreen(mc, modScreen, partial);
        R.call(fml, "onRenderTickEnd", partial);
        postRender("END", partial);
    }

    /** Called in place of Minecraft.displayGuiScreen, see {@link BridgeTransformer}. */
    public static void displayScreen(Object screen) {
        Minecraft.displayScreen(screen);
    }
}
