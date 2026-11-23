package net.ferrocraft.host;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.net.ServerSocket;
import java.net.Socket;

/**
 * VanillaHUD and PolyCrosshair put mixins on GuiIngame: the host must run 20 frames with them and still answer the last.
 */
public final class VanillaHudTest {
    private VanillaHudTest() {}

    public static void run() {
        int code = 1;
        try (ServerSocket server = new ServerSocket(0, 1, java.net.InetAddress.getLoopbackAddress())) {
            System.setProperty("ferro.bridge", "127.0.0.1:" + server.getLocalPort());
            Thread host = new Thread(() -> Host.run(new String[0]), "host");
            host.setDaemon(true);
            host.start();
            try (Socket s = server.accept()) {
                s.setSoTimeout(60_000);
                DataInputStream in = new DataInputStream(new BufferedInputStream(s.getInputStream()));
                DataOutputStream out = new DataOutputStream(new BufferedOutputStream(s.getOutputStream()));
                in.readFully(new byte[33]);
                in.readUTF(); // the brand
                HookIndex.skip(in);
                Object mc = R.get(R.cls("net.minecraft.client.Minecraft"), "field_71432_P");
                System.out.println("[vanilla-hud] fancy before: " + R.get(R.get(mc, "field_71474_y"), "field_74347_j"));
                R.set(R.get(mc, "field_71474_y"), "field_74347_j", true);
                for (int frame = 0; frame < 20; frame++) {
                    out.write(MirrorHudTest.mirror());
                    out.write(MirrorHudTest.frameRequest());
                    out.flush();
                    System.out.println("[vanilla-hud] frame " + frame + " texts " + MirrorHudTest.readTexts(in));
                }
            }
            System.out.println("VanillaHudTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }
}
