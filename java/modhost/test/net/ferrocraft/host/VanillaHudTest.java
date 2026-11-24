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
import java.nio.IntBuffer;
import java.util.HashMap;
import java.util.Map;
import javax.imageio.ImageIO;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

public final class VanillaHudTest {
    private static final int CROSSHAIR = 1 << 4;
    private static final int ARMOR = 1 << 5;
    private static final int DIAMOND_HELMET = 310;
    private static final int ARMOR_X = 32, ARMOR_Y = 40;
    private static final String OVERLAY_DONE = "VanillaHudTest: Post(ALL)";
    private static final String PARTIAL_DRAW = "VanillaHudTest: failed handler draw";

    private final Map<String, byte[]> textures = new HashMap<>();

    private volatile boolean fail;
    private volatile boolean failureInvoked;
    private int probeTexture;

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
            Class<?> armorClass = R.cls("org.polyfrost.vanillahud.hud.Armor");
            Object armor = R.get(armorClass, "hud");
            R.set(armor, "enabled", true);
            R.set(armor, "alignment", false);
            R.set(armor, "scale", 1f);
            R.set(armor.getClass(), "healthLink", false);
            R.set(armor.getClass(), "mountLink", false);
            R.call(R.get(armor, "position"), "setPosition", (float) ARMOR_X, (float) ARMOR_Y, 427f, 240f);
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
        out.write(MirrorHudTest.mirror(DIAMOND_HELMET));
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
        if ((frame.hud & ARMOR) == 0) throw new AssertionError("missing VanillaHUD armor ownership");
        checkArmor(frame, icons);
        if (fail && !hasRecoveryProbe(frame)) {
            throw new AssertionError("Minecraft's cached GL calls did not draw the recovery probe");
        }
        return frame.hud;
    }

    private void checkArmor(MirrorHudTest.Frame frame, BufferedImage icons) throws IOException {
        for (int slot = 0; slot < 10; slot++) {
            int x = ARMOR_X + slot * 8;
            int u = slot == 0 ? 34 : slot == 1 ? 25 : 16;
            if (!hasArmorSprite(frame, icons, x, u)) {
                throw new AssertionError("missing VanillaHUD armor sprite at " + x + ", " + ARMOR_Y);
            }
        }
    }

    private boolean hasArmorSprite(MirrorHudTest.Frame frame, BufferedImage icons, int x, int u) throws IOException {
        for (MirrorHudTest.Image image : frame.images) {
            if (image.x0 != x || image.y0 != ARMOR_Y || image.x1 != x + 9 || image.y1 != ARMOR_Y + 9
                    || image.argb != -1 || image.recolor || image.blend != 0 || image.clip != null) continue;
            if (sameArmorPixels(decodeTexture(image.name), icons, u)) return true;
        }
        return false;
    }

    private static boolean sameArmorPixels(BufferedImage actual, BufferedImage icons, int u) {
        if (actual == null || actual.getWidth() != 9 || actual.getHeight() != 9) return false;
        for (int dy = 0; dy < 9; dy++) {
            for (int dx = 0; dx < 9; dx++) {
                if (actual.getRGB(dx, dy) != icons.getRGB(u + dx, 9 + dy)) return false;
            }
        }
        return true;
    }

    private boolean hasRecoveryProbe(MirrorHudTest.Frame frame) throws IOException {
        int matches = 0;
        for (MirrorHudTest.Image image : frame.images) {
            if (image.x0 != 2 || image.y0 != 3 || image.x1 != 3 || image.y1 != 4
                    || image.argb != 0x804080bf || image.blend != 3 || image.recolor) continue;
            byte[] png = frame.definitions.get(image.name);
            if (png == null) continue;
            BufferedImage decoded = ImageIO.read(new ByteArrayInputStream(png));
            if (decoded != null && decoded.getWidth() == 1 && decoded.getHeight() == 1 && decoded.getRGB(0, 0) == -1) {
                matches++;
                if (frame.images.stream().filter(other -> other.name.equals(image.name)).count() != 1) return false;
            }
        }
        return matches == 1;
    }

    private boolean matchesCrosshair(MirrorHudTest.Image image, BufferedImage icons) throws IOException {
        if (image.x0 != 206.5f || image.y0 != 113 || image.x1 != 222.5f || image.y1 != 129
                || image.argb != -1 || image.recolor || image.blend != 1) return false;
        BufferedImage decoded = decodeTexture(image.name);
        return decoded != null && sameCrosshairPixels(image, decoded, icons);
    }

    private BufferedImage decodeTexture(String name) throws IOException {
        byte[] png = textures.get(name);
        return png == null ? null : ImageIO.read(new ByteArrayInputStream(png));
    }

    static boolean sameCrosshairPixels(MirrorHudTest.Image image, BufferedImage decoded, BufferedImage icons) {
        int size = icons.getWidth() / 16;
        if (decoded.getWidth() != size || decoded.getHeight() != size) return false;
        float dx = (image.x1 - image.x0) / size, dy = (image.y1 - image.y0) / size;
        boolean visible = false;
        for (int y = 0; y < size; y++) {
            for (int x = 0; x < size; x++) {
                int expected = crosshairPixel(icons.getRGB(x, y));
                int actual = decoded.getRGB(x, y);
                if (expected != (actual >>> 24 == 0 ? 0 : actual)) return false;
                visible |= expected != 0 && insideClip(image.clip, image.x0 + x * dx, image.y0 + y * dy, dx, dy);
            }
        }
        return visible;
    }

    private static boolean insideClip(float[] clip, float x, float y, float dx, float dy) {
        return clip == null || (Math.max(clip[0], x) < Math.min(clip[2], x + dx)
                && Math.max(clip[1], y) < Math.min(clip[3], y + dy));
    }

    private static int crosshairPixel(int pixel) {
        return pixel == 0xff000000 || pixel >>> 24 == 0 ? 0 : pixel;
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void pre(RenderGameOverlayEvent.Pre event) throws IOException {
        if (fail && event.type == ElementType.ALL) {
            failureInvoked = true;
            event.setCanceled(true);
            Sink.text(PARTIAL_DRAW, 0, 0, -1, false);
            Sink.push();
            Sink.translate(100, 100);
            probeTexture = GlState.genTexture();
            GlState.DrawState beforeUpload = GlState.saveDrawState();
            GlState.bindTexture(3553, probeTexture);
            GlState.texImage(3553, 0, 6408, 1, 1, 0, 32993, 33639, IntBuffer.wrap(new int[] {-1}));
            GlState.restoreDrawState(beforeUpload);
            selectProbeState();
            GlState.quadSized(2, 3, 0, 0, 1, 1, 1, 1);
            throw new IOException("VanillaHudTest: overlay handler failed after cancellation");
        }
    }

    @SubscribeEvent(priority = EventPriority.HIGH)
    public void drawAfterFailure(RenderGameOverlayEvent.Pre event) {
        if (!fail || event.type != ElementType.ALL) return;
        selectProbeState();
        GlState.quadSized(2, 3, 0, 0, 1, 1, 1, 1);
        Class<?> gl = R.cls("net.minecraft.client.renderer.GlStateManager");
        R.call(gl, "func_179131_c", 1f, 1f, 1f, 1f);
        R.call(gl, "func_179112_b", 770, 771);
        R.call(gl, "func_179144_i", 0);
    }

    private void selectProbeState() {
        Class<?> gl = R.cls("net.minecraft.client.renderer.GlStateManager");
        R.call(gl, "func_179144_i", probeTexture);
        R.call(gl, "func_179131_c", .25f, .5f, .75f, .5f);
        R.call(gl, "func_179147_l");
        R.call(gl, "func_179112_b", 768, 1);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void post(RenderGameOverlayEvent.Post event) {
        if (event.type == ElementType.ALL) Sink.text(OVERLAY_DONE, 0, 0, -1, false);
    }
}
