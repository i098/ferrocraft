package net.ferrocraft.host;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * Plays the engine's side of the bridge: starts the host, sends the mirror and frames as Rust does, and reads the text
 * that EvergreenHUD draws in the reply. The texts must hold the values of the mirror.
 */
public final class MirrorHudTest {
    private MirrorHudTest() {}

    public static void run() {
        int code = 1;
        try (ServerSocket server = new ServerSocket(0, 1, java.net.InetAddress.getLoopbackAddress())) {
            System.setProperty("ferro.bridge", "127.0.0.1:" + server.getLocalPort());
            Thread host = new Thread(() -> Host.run(new String[0]), "host");
            host.setDaemon(true);
            host.start();
            try (Socket s = server.accept()) {
                DataInputStream in = new DataInputStream(new BufferedInputStream(s.getInputStream()));
                DataOutputStream out = new DataOutputStream(new BufferedOutputStream(s.getOutputStream()));
                byte[] hello = new byte[33];
                in.readFully(hello);
                check(hello[0] == 'H', "hello");
                in.readUTF(); // the brand
                HookIndex.skip(in);
                enable("getFps", "getFps");
                enable("getBiome", "getHud");
                enable("getServerIP", "getHud");
                R.call(R.cls("cc.polyfrost.oneconfig.internal.hud.HudCore"), "reInitHuds");
                List<String> texts = new ArrayList<>();
                for (int frame = 0; frame < 30 && !done(texts); frame++) {
                    out.write(mirror());
                    out.write(frameRequest());
                    out.flush();
                    texts = readTexts(in);
                    Thread.sleep(60);
                }
                System.out.println("[mirror-hud] texts: " + texts);
                String all = String.join("\n", texts);
                check(texts.contains("FPS: 144"), "FPS text: " + texts);
                check(texts.contains("Biome: Taiga"), "Biome text: " + texts);
                check(texts.stream().anyMatch(t -> t.contains("mc.example:25565")), "Server text: " + texts);
                check(texts.containsAll(java.util.Arrays.asList("X: 12", "Y: 64", "Z: -7")), "Coordinates text: " + texts);
            }
            System.out.println("MirrorHudTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    /** Turns on an EvergreenHUD element: ModConfig.INSTANCE.group().hud().enabled. */
    private static void enable(String group, String hud) {
        Object config = R.get(R.cls("org.polyfrost.evergreenhud.config.ModConfig"), "INSTANCE");
        Object g = R.call(config, group);
        R.set(g, "enabled", true);
        Object h = R.call(g, hud);
        R.set(h, "enabled", true);
        System.out.println("[mirror-hud] " + h.getClass().getName() + " enabled=" + R.get(h, "enabled"));
    }

    private static boolean done(List<String> texts) {
        String all = String.join("\n", texts);
        return all.contains("FPS: 144") && all.contains("Taiga") && all.contains("mc.example:25565") && all.contains("Z: -7");
    }

    private static byte[] mirror() throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeByte('M');
        w.writeInt(144);
        w.writeInt(37);
        w.writeDouble(12.0);
        w.writeDouble(64.0);
        w.writeDouble(-7.0);
        w.writeFloat(90f);
        w.writeFloat(-10f);
        w.writeByte(5);
        w.writeFloat(18.5f);
        w.writeByte(0);
        w.writeShort(0);
        for (int i = 0; i < 40; i++) {
            w.writeShort(-1);
            w.writeByte(0);
            w.writeShort(0);
        }
        byte[] name = "mc.example:25565".getBytes(StandardCharsets.UTF_8);
        w.writeShort(name.length);
        w.write(name);
        return b.toByteArray();
    }

    private static byte[] frameRequest() throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeByte('F');
        w.writeInt(854);
        w.writeInt(480);
        w.writeInt(2);
        w.writeByte(0);
        w.writeByte(0);
        w.writeByte(0);
        w.writeInt(0);
        w.writeInt(0);
        w.writeShort(0);
        return b.toByteArray();
    }

    /** The texts of one reply frame; the layout is Sink.take. */
    private static List<String> readTexts(DataInputStream in) throws IOException {
        in.readUnsignedByte();
        long n = in.readInt() & 0xffffffffL;
        List<String> texts = new ArrayList<>();
        for (long i = 0; i < n; i++) {
            int tag = in.readUnsignedByte();
            switch (tag) {
                case 'R': in.skipBytes(16 + 4); break;
                case 'G': in.skipBytes(16 + 8); break;
                case 'T':
                    in.skipBytes(8 + 4 + 1 + 12);
                    texts.add(string(in));
                    break;
                case 'D':
                    string(in);
                    in.skipBytes(in.readInt());
                    break;
                case 'I':
                    string(in);
                    in.skipBytes(16 + 4 + 1);
                    break;
                case 'C':
                    if (in.readUnsignedByte() != 0) in.skipBytes(16);
                    break;
                default: throw new IOException("unknown command " + tag);
            }
        }
        return texts;
    }

    private static String string(DataInputStream in) throws IOException {
        byte[] b = new byte[in.readInt()];
        in.readFully(b);
        return new String(b, StandardCharsets.UTF_8);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
