package net.ferrocraft.host;

import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.nio.IntBuffer;
import javax.imageio.ImageIO;

/**
 * Plays the GL calls of a mod that draws a {@code DynamicTexture} with {@code Gui.drawScaledCustomSizeModalRect} (as PolyCrosshair does)
 * into {@link GlState} and reads the frame {@link Sink} sends: the picture is defined once, named by its content, cut to the quad's
 * part, tinted and blended as GL would. No FML needed.
 */
public final class GlStateTest {
    private static final int RED = 0xFFFF0000;
    private static final int GREEN = 0xFF00FF00;
    private static final int BLUE = 0xFF0000FF;
    private static final int CLEAR = 0;

    private GlStateTest() {}

    public static void main(String[] args) throws Exception {
        int id = GlState.genTexture();
        GlState.bindTexture(3553, id);
        GlState.texImage(3553, 0, 6408, 4, 4, 0, 32993, 33639, null);
        // Rows 1 and 2 of the 4 x 4 texture; the rest stays clear.
        GlState.texSubImage(3553, 0, 0, 1, 4, 2, 32993, 33639, IntBuffer.wrap(new int[] {
            CLEAR, RED, GREEN, CLEAR,
            BLUE, CLEAR, CLEAR, RED}));
        GlState.color(1f, 0.5f, 0f, 1f);
        GlState.enable(3042);
        GlState.blendFunc(775, 769);

        // The 2 x 2 part at (1, 1), in a box of 8 x 8 at (10, 20).
        GlState.quad(10, 20, 1f, 1f, 2, 2, 8, 8, 4f, 4f);
        GlState.quad(30, 20, 1f, 1f, 2, 2, 8, 8, 4f, 4f);
        // The same part named by a texture the caller takes as twice as big: the same picture, not sent again.
        GlState.quad(50, 20, 2f, 2f, 4, 4, 8, 8, 8f, 8f);
        Frame f = Frame.read(Sink.take(false));
        check(f.names.size() == 1 && f.names.get(0).startsWith("tex:") && f.names.get(0).length() == 4 + 64, "one define named by a SHA-256: " + f.names);
        check(f.boxes.size() == 3 && f.boxes.get(0)[0] == 10 && f.boxes.get(0)[2] == 18 && f.boxes.get(1)[0] == 30 && f.boxes.get(2)[0] == 50, "three image commands in the boxes");
        check(f.tint == 0xFFFF8000 && f.recolor == 0 && f.blend == 1, "tint " + Integer.toHexString(f.tint) + ", no recolor, invert blend");
        int[] want = {RED, GREEN, CLEAR, CLEAR};
        // Part (1,1)-(3,3): row 1 is [RED, GREEN] and row 2 is [CLEAR, CLEAR].
        for (int i = 0; i < 4; i++) check(f.pixels[i] == want[i], "pixel " + i + " is " + Integer.toHexString(f.pixels[i]));

        // Another upload changes the picture and its name; blending off draws opaque.
        GlState.texSubImage(3553, 0, 1, 1, 1, 1, 32993, 33639, IntBuffer.wrap(new int[] {BLUE}));
        GlState.disable(3042);
        GlState.quad(10, 20, 1f, 1f, 2, 2, 8, 8, 4f, 4f);
        Frame g = Frame.read(Sink.take(false));
        check(g.names.size() == 1 && !g.names.get(0).equals(f.names.get(0)), "an upload gives the picture another name");
        check(g.blend == 0, "blend off is source alpha over");
        check(g.tint >>> 24 == 0xFF, "blend off draws opaque");
        check(g.pixels[0] >>> 24 == 0xFF && g.pixels[1] >>> 24 == 0xFF && g.pixels[2] >>> 24 == 0 && g.pixels[3] >>> 24 == 0, "blend off keeps opaque texels and alpha-tests transparent ones out");

        // A bound texture that holds no pixels draws nothing.
        GlState.bindTexture(3553, 99);
        GlState.quad(0, 0, 0f, 0f, 2, 2, 8, 8, 4f, 4f);
        check(Frame.read(Sink.take(false)).boxes.isEmpty(), "an unknown texture draws nothing");
        patchedClasses();
        System.out.println("GlStateTest ok");
    }

    /**
     * The same calls through the real LWJGL {@code GL11} and a stand-in {@code Gui} after {@link BridgeTransformer} patched them:
     * the patched calls must reach {@link GlState} with their arguments in order.
     */
    private static void patchedClasses() throws Exception {
        Loader loader = new Loader();
        byte[] gl11;
        try (java.io.InputStream in = GlStateTest.class.getResourceAsStream("/org/lwjgl/opengl/GL11.class")) {
            java.io.ByteArrayOutputStream b = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            for (int n; (n = in.read(buf)) > 0; ) b.write(buf, 0, n);
            gl11 = b.toByteArray();
        }
        Class<?> gl = loader.define("org.lwjgl.opengl.GL11", new BridgeTransformer().transform("org.lwjgl.opengl.GL11", "org.lwjgl.opengl.GL11", gl11));
        String gui = "net.minecraft.client.gui.Gui";
        Class<?> g = loader.define(gui, new MinecraftBridgeTransformer().transform(gui, gui, stubGui()));
        int id = (Integer) gl.getMethod("glGenTextures").invoke(null);
        gl.getMethod("glBindTexture", int.class, int.class).invoke(null, 3553, id);
        gl.getMethod("glTexImage2D", int.class, int.class, int.class, int.class, int.class, int.class, int.class, int.class, IntBuffer.class)
                .invoke(null, 3553, 0, 6408, 2, 2, 0, 32993, 33639, IntBuffer.wrap(new int[] {RED, GREEN, BLUE, CLEAR}));
        gl.getMethod("glColor4f", float.class, float.class, float.class, float.class).invoke(null, 1f, 1f, 1f, 0.5f);
        gl.getMethod("glEnable", int.class).invoke(null, 3042);
        gl.getMethod("glBlendFunc", int.class, int.class).invoke(null, 770, 771);
        g.getMethod("func_152125_a", int.class, int.class, float.class, float.class, int.class, int.class, int.class, int.class, float.class, float.class)
                .invoke(null, 5, 6, 0f, 0f, 2, 2, 4, 4, 2f, 2f);
        g.getMethod("func_146110_a", int.class, int.class, float.class, float.class, int.class, int.class, float.class, float.class)
                .invoke(null, 1, 2, 0f, 0f, 2, 2, 2f, 2f);
        Frame f = Frame.read(Sink.take(false));
        check(f.names.size() == 1 && f.boxes.size() == 2, "one define and two images: " + f.names + " " + f.boxes.size());
        check(f.boxes.get(0)[0] == 5 && f.boxes.get(0)[1] == 6 && f.boxes.get(0)[2] == 9 && f.boxes.get(0)[3] == 10, "the scaled quad's box");
        check(f.boxes.get(1)[0] == 1 && f.boxes.get(1)[3] == 4, "the sized quad's box");
        check(f.tint == 0x80FFFFFF && f.blend == 0, "tint " + Integer.toHexString(f.tint));
        check(f.pixels[0] == RED && f.pixels[1] == GREEN && f.pixels[2] == BLUE && f.pixels[3] == CLEAR, "the uploaded 2 x 2 picture");
    }

    private static final class Loader extends ClassLoader {
        Class<?> define(String name, byte[] b) {
            return defineClass(name, b, 0, b.length);
        }
    }

    /** A {@code Gui} with the two textured quad methods and nothing in them. */
    private static byte[] stubGui() {
        org.objectweb.asm.ClassWriter cw = new org.objectweb.asm.ClassWriter(org.objectweb.asm.ClassWriter.COMPUTE_MAXS);
        cw.visit(org.objectweb.asm.Opcodes.V1_6, org.objectweb.asm.Opcodes.ACC_PUBLIC, "net/minecraft/client/gui/Gui", null, "java/lang/Object", null);
        stubMethod(cw, "func_152125_a", "(IIFFIIIIFF)V");
        stubMethod(cw, "func_146110_a", "(IIFFIIFF)V");
        return cw.toByteArray();
    }

    private static void stubMethod(org.objectweb.asm.ClassWriter cw, String name, String desc) {
        org.objectweb.asm.MethodVisitor v = cw.visitMethod(org.objectweb.asm.Opcodes.ACC_PUBLIC | org.objectweb.asm.Opcodes.ACC_STATIC, name, desc, null, null);
        v.visitCode();
        v.visitInsn(org.objectweb.asm.Opcodes.RETURN);
        v.visitMaxs(0, 0);
        v.visitEnd();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    /** The commands of one frame that this test cares about: define names, image boxes and the last image's tint, recolor and blend. */
    private static final class Frame {
        final java.util.List<String> names = new java.util.ArrayList<>();
        final java.util.List<float[]> boxes = new java.util.ArrayList<>();
        int tint;
        int recolor;
        int blend;
        int[] pixels;

        static Frame read(byte[] frame) throws IOException {
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(frame));
            Frame f = new Frame();
            in.readUnsignedByte();
            int n = in.readInt();
            for (int i = 0; i < n; i++) {
                int tag = in.readUnsignedByte();
                if (tag == 'D') {
                    f.names.add(string(in));
                    byte[] png = new byte[in.readInt()];
                    in.readFully(png);
                    BufferedImage img = ImageIO.read(new ByteArrayInputStream(png));
                    f.pixels = img.getRGB(0, 0, img.getWidth(), img.getHeight(), null, 0, img.getWidth());
                } else if (tag == 'I') {
                    string(in);
                    f.boxes.add(new float[] {in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat()});
                    f.tint = in.readInt();
                    f.recolor = in.readUnsignedByte();
                    f.blend = in.readUnsignedByte();
                } else {
                    throw new AssertionError("unexpected command " + (char) tag);
                }
            }
            return f;
        }

        private static String string(DataInputStream in) throws IOException {
            byte[] b = new byte[in.readInt()];
            in.readFully(b);
            return new String(b, "UTF-8");
        }
    }
}
