package net.ferrocraft.host;

import java.awt.image.BufferedImage;
import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.HashMap;
import java.util.Map;
import javax.imageio.ImageIO;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

public final class VanillaHudTest {
    private static final int CROSSHAIR = 1 << 4;
    private static final String OVERLAY_DONE = "VanillaHudTest: Post(ALL)";
    private static final String PARTIAL_DRAW = "VanillaHudTest: failed handler draw";

    private final Map<String, byte[]> textures = new HashMap<>();

    private volatile boolean fail;
    private volatile boolean failureInvoked;

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
            Class<?> configClass = R.cls("org.polyfrost.crosshair.config.ModConfig");
            Object config = R.get(configClass, "INSTANCE");
            R.call(config, "setMode", false);
            Object position = R.call(config, "getNewCurrentCrosshair");
            R.call(position, "setScale", 100);
            R.call(position, "setOffsetX", 0);
            R.call(position, "setOffsetY", 0);
            R.call(R.call(config, "getRenderConfig"), "setInvertColor", true);
            BufferedImage icons;
            try (InputStream asset = configClass.getResourceAsStream("/assets/minecraft/textures/gui/icons.png")) {
                if (asset == null) throw new IOException("missing Minecraft HUD texture");
                icons = ImageIO.read(asset);
            }
            if (icons == null) throw new IOException("invalid Minecraft HUD texture");
            int owned = 0;
            for (int frame = 0; frame < 20; frame++) owned = probe.checkFrame(in, out, icons);
            probe.fail = true;
            if (probe.checkFrame(in, out, icons) != owned) throw new AssertionError("failure changed completed HUD claims");
            if (!probe.failureInvoked) throw new AssertionError("the failing handler did not run");
            probe.fail = false;
            probe.checkFrame(in, out, icons);
            System.out.println("VanillaHudTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    private int checkFrame(DataInputStream in, DataOutputStream out, BufferedImage icons) throws IOException {
        out.write(MirrorHudTest.mirror());
        out.write(MirrorHudTest.frameRequest());
        out.flush();
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(in);
        textures.putAll(frame.definitions);
        if (!frame.texts.contains(OVERLAY_DONE)) throw new AssertionError("overlay did not reach Post(ALL)");
        if (frame.texts.contains(PARTIAL_DRAW)) throw new AssertionError("failed HUD draw reached the reply");
        if ((frame.hud & CROSSHAIR) == 0) throw new AssertionError("missing PolyCrosshair ownership");
        int matches = 0;
        for (MirrorHudTest.Image image : frame.images) {
            if (matchesCrosshair(image, icons)) matches++;
        }
        if (matches != 1) throw new AssertionError("expected one visible PolyCrosshair image, got " + matches);
        return frame.hud;
    }

    private boolean matchesCrosshair(MirrorHudTest.Image image, BufferedImage icons) throws IOException {
        if (image.x0 != 206 || image.y0 != 113 || image.x1 != 222 || image.y1 != 129
                || image.argb != -1 || image.recolor || image.blend != 1) return false;
        byte[] png = textures.get(image.name);
        if (png == null) return false;
        BufferedImage decoded = ImageIO.read(new ByteArrayInputStream(png));
        return decoded != null && sameCrosshairPixels(decoded, icons);
    }

    private static boolean sameCrosshairPixels(BufferedImage decoded, BufferedImage icons) {
        int size = icons.getWidth() / 16;
        if (decoded.getWidth() != size || decoded.getHeight() != size) return false;
        boolean visible = false;
        for (int y = 0; y < size; y++) {
            for (int x = 0; x < size; x++) {
                int expected = crosshairPixel(icons.getRGB(x, y));
                int actual = decoded.getRGB(x, y);
                if (expected != (actual >>> 24 == 0 ? 0 : actual)) return false;
                visible |= expected != 0;
            }
        }
        return visible;
    }

    private static int crosshairPixel(int pixel) {
        return pixel == 0xff000000 || pixel >>> 24 == 0 ? 0 : pixel;
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void pre(RenderGameOverlayEvent.Pre event) {
        if (fail && event.type == ElementType.ALL) {
            failureInvoked = true;
            event.setCanceled(true);
            Sink.text(PARTIAL_DRAW, 0, 0, -1, false);
            Sink.push();
            Sink.translate(100, 100);
            throw new IllegalStateException("VanillaHudTest: overlay handler failed after cancellation");
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void post(RenderGameOverlayEvent.Post event) {
        if (event.type == ElementType.ALL) Sink.text(OVERLAY_DONE, 0, 0, -1, false);
    }
}
