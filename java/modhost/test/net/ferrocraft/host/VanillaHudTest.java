package net.ferrocraft.host;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.net.ServerSocket;
import java.net.Socket;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

public final class VanillaHudTest {
    private static final int CROSSHAIR = 1 << 4;
    private static final String OVERLAY_DONE = "VanillaHudTest: Post(ALL)";

    private volatile boolean fail;

    private VanillaHudTest() {}

    public static void run() {
        int code = 1;
        try (ServerSocket server = new ServerSocket(0, 1, java.net.InetAddress.getLoopbackAddress())) {
            System.setProperty("ferro.bridge", "127.0.0.1:" + server.getLocalPort());
            Thread host = new Thread(() -> Host.run(new String[0]), "host");
            host.setDaemon(true);
            host.start();
            Socket s = server.accept();
            s.setSoTimeout(60_000);
            DataInputStream in = new DataInputStream(new BufferedInputStream(s.getInputStream()));
            DataOutputStream out = new DataOutputStream(new BufferedOutputStream(s.getOutputStream()));
            if (in.readUnsignedByte() != 'H') throw new IOException("expected bridge hello");
            in.readFully(new byte[32]);
            in.readUTF();
            HookIndex.skip(in);
            VanillaHudTest probe = new VanillaHudTest();
            ForgeGlue.INSTANCE.register(probe);
            Object mc = R.get(R.cls("net.minecraft.client.Minecraft"), "field_71432_P");
            R.set(R.get(mc, "field_71474_y"), "field_74347_j", true);
            for (int frame = 0; frame < 20; frame++) checkFrame(in, out, false);
            probe.fail = true;
            checkFrame(in, out, true);
            probe.fail = false;
            checkFrame(in, out, false);
            System.out.println("VanillaHudTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    private static void checkFrame(DataInputStream in, DataOutputStream out, boolean failed) throws IOException {
        out.write(MirrorHudTest.mirror());
        out.write(MirrorHudTest.frameRequest());
        out.flush();
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(in);
        if (!frame.texts.contains(OVERLAY_DONE)) throw new AssertionError("overlay did not reach Post(ALL)");
        if (failed) {
            if (frame.hud != 0) throw new AssertionError("failed overlay asserted HUD ownership: " + frame.hud);
        } else if ((frame.hud & CROSSHAIR) == 0 || frame.images.stream().noneMatch(VanillaHudTest::centeredTexture)) {
            throw new AssertionError("missing PolyCrosshair image or ownership");
        }
    }

    private static boolean centeredTexture(MirrorHudTest.Image image) {
        return image.name.startsWith("tex:") && image.x0 <= 214 && image.x1 >= 212
                && image.y0 <= 121 && image.y1 >= 119
                && image.x1 > image.x0 && image.x1 - image.x0 < 64
                && image.y1 > image.y0 && image.y1 - image.y0 < 64;
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void pre(RenderGameOverlayEvent.Pre event) {
        if (fail && event.type == ElementType.ALL) {
            event.setCanceled(true);
            throw new IllegalStateException("VanillaHudTest: overlay handler failed after cancellation");
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void post(RenderGameOverlayEvent.Post event) {
        if (event.type == ElementType.ALL) Sink.text(OVERLAY_DONE, 0, 0, -1, false);
    }
}
