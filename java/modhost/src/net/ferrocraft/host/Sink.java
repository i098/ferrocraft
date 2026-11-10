package net.ferrocraft.host;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/**
 * The draw commands of one frame. The patched Gui and FontRenderer methods and the GL11 matrix calls
 * land here; {@link #take()} returns them in the wire format that crates/modhost reads.
 *
 * <p>Wire: a frame is u32 count, then per command a tag byte. 'R' rect: 4 f32 (x0 y0 x1 y1) i32 argb.
 * 'G' gradient: 4 f32, i32 top, i32 bottom. 'T' text: f32 x y, i32 argb, u8 shadow, f32 tx ty scale,
 * u32 byte length, UTF-8 bytes. Rect and gradient coordinates already include the GL matrix.
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

    private static void quad(float[] m, int x0, int y0, int x1, int y1) throws IOException {
        out.writeFloat(m[0] * x0 + m[1]);
        out.writeFloat(m[0] * y0 + m[2]);
        out.writeFloat(m[0] * x1 + m[1]);
        out.writeFloat(m[0] * y1 + m[2]);
    }

    /** Returns the frame's commands and resets the buffer and the matrix. */
    static synchronized byte[] take() {
        byte[] body = bytes.toByteArray();
        ByteArrayOutputStream framed = new ByteArrayOutputStream(body.length + 4);
        try {
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
        STACK[0][0] = 1;
        STACK[0][1] = 0;
        STACK[0][2] = 0;
        return framed.toByteArray();
    }

    static {
        STACK[0][0] = 1;
    }
}
