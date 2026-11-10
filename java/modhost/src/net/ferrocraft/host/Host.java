package net.ferrocraft.host;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.net.Socket;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import net.minecraft.launchwrapper.Launch;

/**
 * Boots Forge's FML around Minecraft state that Rust owns, then answers one frame message at a time.
 *
 * <p>Frame in (Rust to Java): 'F', i32 width, i32 height, i32 guiScale, u8 forceUnicode, i32 mouseX,
 * i32 mouseY, u8 mouse button mask, u16 key count, i32 LWJGL key codes. 'Q' ends the host.
 * Frame out: the bytes of {@link Sink#take()}.
 */
public final class Host {
    private static final long TICK_NANOS = 50_000_000L;
    private static final int MAX_TICKS = 10;

    private final Object mc;
    private final Object forgeBus;
    private final Object fml;
    private final boolean[] held = new boolean[256];
    private long lastTick = System.nanoTime();

    private Host(Object mc) {
        this.mc = mc;
        this.forgeBus = R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS");
        this.fml = R.call(R.cls("net.minecraftforge.fml.common.FMLCommonHandler"), "instance");
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
    private static void halt(int code) {
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
        if (bridge == null) {
            host.selfTest();
            return;
        }
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
        out.flush();
        while (true) {
            int tag = in.read();
            if (tag != 'F') return;
            int w = in.readInt();
            int h = in.readInt();
            int scale = in.readInt();
            boolean unicode = in.readUnsignedByte() != 0;
            int mx = in.readInt();
            int my = in.readInt();
            int buttons = in.readUnsignedByte();
            int n = in.readUnsignedShort();
            int[] keys = new int[n];
            for (int i = 0; i < n; i++) keys[i] = in.readInt();
            frame(w, h, scale, unicode, mx, my, buttons, keys);
            out.write(Sink.take());
            out.flush();
        }
    }

    private void frame(int w, int h, int scale, boolean unicode, int mx, int my, int buttons, int[] keys) {
        Input.set(w, h, mx, my);
        for (int b = 0; b < 3; b++) Input.button(b, (buttons & (1 << b)) != 0);
        boolean[] now = new boolean[256];
        for (int k : keys) if (k >= 0 && k < 256) now[k] = true;
        Class<?> keyBinding = R.cls("net.minecraft.client.settings.KeyBinding");
        for (int k = 0; k < 256; k++) {
            if (now[k] != held[k]) {
                Input.key(k, now[k]);
                R.call(keyBinding, "func_74510_a", k, now[k]);
                if (now[k]) R.call(keyBinding, "func_74507_a", k);
                held[k] = now[k];
            }
        }
        Minecraft.resize(mc, w, h, scale, unicode);
        long t = System.nanoTime();
        int ticks = (int) Math.min(MAX_TICKS, (t - lastTick) / TICK_NANOS);
        lastTick += ticks * TICK_NANOS;
        if (t - lastTick > MAX_TICKS * TICK_NANOS) lastTick = t;
        for (int i = 0; i < ticks; i++) {
            R.call(fml, "onPreClientTick");
            R.call(fml, "onPostClientTick");
        }
        float partial = Math.min(1f, (t - lastTick) / (float) TICK_NANOS);
        renderOverlay(partial);
    }

    /** GuiIngameForge.renderGameOverlay, which posts the overlay events and runs mixins hooked into it. */
    private void renderOverlay(float partial) {
        R.call(fml, "onRenderTickStart", partial);
        Minecraft.renderOverlay(mc, partial);
        R.call(fml, "onRenderTickEnd", partial);
    }

    /** Without a Rust peer: hold W for a while and print the commands, to prove the draw path. */
    private void selfTest() {
        for (int i = 0; i < 40; i++) {
            int[] keys = i >= 20 ? new int[] {17} : new int[0];
            frame(854, 480, 0, false, 10, 10, 0, keys);
            byte[] b = Sink.take();
            if (i == 30) { StringBuilder sb = new StringBuilder(); for (byte x : b) sb.append(String.format("%02x", x)); log("HEX " + sb); }
            if (i == 5 || i == 30) log("frame " + i + ": " + java.util.Arrays.toString(Collections.singletonList(b.length).toArray()) + " bytes, commands=" + ((b[0] << 24) | (b[1] << 16) | (b[2] << 8) | b[3]));
            try {
                Thread.sleep(50);
            } catch (InterruptedException e) {
                return;
            }
        }
        List<String> done = new ArrayList<>();
        done.add("done");
        log(done.get(0));
    }
}
