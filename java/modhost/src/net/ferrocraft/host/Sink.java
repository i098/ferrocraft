package net.ferrocraft.host;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Set;

/**
 * The draw commands of one frame. The patched Gui and FontRenderer methods and the GL11 matrix calls
 * land here; {@link #take(boolean)} returns them in the wire format that crates/modhost reads.
 *
 * <p>Wire: a frame is u8 mod screen open, u32 count, then per command a tag byte. 'R' rect: 4 f32 (x0 y0 x1 y1) i32 argb.
 * 'G' gradient: 4 f32, i32 top, i32 bottom. 'T' text: f32 x y, i32 argb, u8 shadow, f32 tx ty scale,
 * u32 byte length, UTF-8 bytes. 'D' define: u32 length and UTF-8 name, u32 length and bytes of a PNG or SVG file;
 * sent once per name. 'I' image: the name as in 'D', 4 f32, i32 argb, u8 recolor (an SVG takes the color of argb).
 * 'C' clip: u8 on, then 4 f32 when on; later commands are cut to it. Rect, gradient, image and clip coordinates
 * already include the GL matrix. 'H' HUD owner: i32 bit set of the vanilla HUD elements that Java owns this frame.
 */
public final class Sink {
    private static final int MODELVIEW = 5888;
    private static final int DEPTH = 32;
    private static final float[][] STACK = new float[DEPTH][3];
    private static int top;
    private static int mode = MODELVIEW;
    private static ByteArrayOutputStream bytes = new ByteArrayOutputStream(4096);
    private static DataOutputStream out = new DataOutputStream(bytes);
    private static int count;
    private static boolean clipped;
    private static final Set<String> DEFINED = new HashSet<>();

    private Sink() {
        throw new AssertionError();
    }

    /** Called by the patched GL11 shim. */
    public static synchronized void matrixMode(int m) {
        mode = m;
    }

    public static synchronized void push() {
        if (mode == MODELVIEW && top + 1 < DEPTH) {
            STACK[top + 1][0] = STACK[top][0];
            STACK[top + 1][1] = STACK[top][1];
            STACK[top + 1][2] = STACK[top][2];
            top++;
        }
    }

    public static synchronized void pop() {
        if (mode == MODELVIEW && top > 0) top--;
    }

    public static synchronized void identity() {
        if (mode == MODELVIEW) {
            STACK[top][0] = 1;
            STACK[top][1] = 0;
            STACK[top][2] = 0;
        }
    }

    /** Matrix entries: scale x, then translate x, translate y. Scale is uniform: GUI mods scale both axes alike. */
    public static synchronized void translate(float x, float y) {
        if (mode == MODELVIEW) {
            float[] m = STACK[top];
            m[1] += m[0] * x;
            m[2] += m[0] * y;
        }
    }

    public static synchronized void scale(float x) {
        if (mode == MODELVIEW) STACK[top][0] *= x;
    }

    public static synchronized void rect(int x0, int y0, int x1, int y1, int argb) {
        float[] m = STACK[top];
        try {
            out.writeByte('R');
            quad(m, x0, y0, x1, y1);
            out.writeInt(argb);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    public static synchronized void gradient(int x0, int y0, int x1, int y1, int top, int bottom) {
        float[] m = STACK[Sink.top];
        try {
            out.writeByte('G');
            quad(m, x0, y0, x1, y1);
            out.writeInt(top);
            out.writeInt(bottom);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    public static synchronized void text(String s, float x, float y, int argb, boolean shadow) {
        if (s == null || s.isEmpty()) return;
        float[] m = STACK[top];
        byte[] utf = s.getBytes(StandardCharsets.UTF_8);
        try {
            out.writeByte('T');
            out.writeFloat(x);
            out.writeFloat(y);
            // FontRenderer.renderString: an alpha of 0 means opaque.
            out.writeInt((argb & 0xFC000000) == 0 ? argb | 0xFF000000 : argb);
            out.writeByte(shadow ? 1 : 0);
            out.writeFloat(m[1]);
            out.writeFloat(m[2]);
            out.writeFloat(m[0]);
            out.writeInt(utf.length);
            out.write(utf);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /** The image {@code name} (defined with {@code data} the first time) in the box x0 y0 x1 y1. */
    public static synchronized void image(String name, byte[] data, float x0, float y0, float x1, float y1, int argb, boolean recolor) {
        float[] m = STACK[top];
        try {
            if (DEFINED.add(name)) {
                out.writeByte('D');
                string(name);
                out.writeInt(data.length);
                out.write(data);
                count++;
            }
            out.writeByte('I');
            string(name);
            quad(m, x0, y0, x1, y1);
            out.writeInt(argb);
            out.writeByte(recolor ? 1 : 0);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /** The box x0 y0 x1 y1 as the current matrix puts it on the screen. */
    public static synchronized float[] transform(float x0, float y0, float x1, float y1) {
        float[] m = STACK[top];
        return new float[] {m[0] * x0 + m[1], m[0] * y0 + m[2], m[0] * x1 + m[1], m[0] * y1 + m[2]};
    }

    /** Cuts what follows to {@code box} (x0 y0 x1 y1, already on the screen), or lifts the cut when null. */
    public static synchronized void clip(float[] box) {
        if (box == null && !clipped) return;
        try {
            out.writeByte('C');
            out.writeByte(box == null ? 0 : 1);
            if (box != null) for (float v : box) out.writeFloat(v);
            clipped = box != null;
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /** The vanilla HUD elements that Java owns this frame, as a bit set (see crates/modhost/src/hud.rs and {@link OverlayFilter}). */
    static synchronized void hud(int bits) {
        try {
            out.writeByte('H');
            out.writeInt(bits);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void string(String s) throws IOException {
        byte[] utf = s.getBytes(StandardCharsets.UTF_8);
        out.writeInt(utf.length);
        out.write(utf);
    }

    private static void quad(float[] m, float x0, float y0, float x1, float y1) throws IOException {
        out.writeFloat(m[0] * x0 + m[1]);
        out.writeFloat(m[0] * y0 + m[2]);
        out.writeFloat(m[0] * x1 + m[1]);
        out.writeFloat(m[0] * y1 + m[2]);
    }

    /** Returns the frame's commands, led by whether a mod screen is open, and resets the buffer and the matrix. */
    static synchronized byte[] take(boolean modScreen) {
        byte[] body = bytes.toByteArray();
        ByteArrayOutputStream framed = new ByteArrayOutputStream(body.length + 5);
        try {
            framed.write(modScreen ? 1 : 0);
            new DataOutputStream(framed).writeInt(count);
            framed.write(body);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        bytes = new ByteArrayOutputStream(4096);
        out = new DataOutputStream(bytes);
        count = 0;
        top = 0;
        mode = MODELVIEW;
        clipped = false;
        STACK[0][0] = 1;
        STACK[0][1] = 0;
        STACK[0][2] = 0;
        return framed.toByteArray();
    }

    static {
        STACK[0][0] = 1;
    }
}
