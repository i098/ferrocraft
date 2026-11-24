package net.ferrocraft.host;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The draw commands of one frame. The patched Gui and FontRenderer methods and the GL11 matrix calls
 * land here; {@link #take(boolean)} returns them in the wire format that crates/modhost reads.
 *
 * <p>Wire: a frame is u8 mod screen open, u32 count, then per command a tag byte. 'R' rect: 4 f32 (x0 y0 x1 y1) i32 argb.
 * 'G' gradient: 4 f32, i32 top, i32 bottom. 'T' text: f32 x y, i32 argb, u8 shadow, f32 tx ty scale,
 * u32 byte length, UTF-8 bytes. 'D' define: u32 length and UTF-8 name, u32 length and bytes of a PNG or SVG file;
 * sent once per name. 'I' image: the name as in 'D', 4 f32, i32 argb, u8 recolor (an SVG takes the color of argb), u8 blend (0 source alpha over, 1 inverts the destination, 2 darkens it, 3 adds).
 * 'C' clip: u8 on, then 4 f32 when on; later commands are cut to it. 'S' shape: u8 kind; kind 0 a rounded rect: 4 f32
 * box, 4 f32 radii (top left, top right, bottom right, bottom left), i32 argb; kind 1 a line: 4 f32 (x0 y0 x1 y1), f32 width,
 * i32 argb. 'H' HUD owner: i32 bit set of the vanilla HUD elements that Java owns this frame. Rect, gradient, image, shape and clip coordinates already include the GL matrix.
 */
public final class Sink {
    private static int hudBits;
    private static final int MODELVIEW = 5888;
    private static final int DEPTH = 32;
    private static final float[][] STACK = new float[DEPTH][3];
    private static int top;
    private static int mode = MODELVIEW;
    private static FrameBytes bytes = new FrameBytes();
    private static DataOutputStream out = new DataOutputStream(bytes);
    private static int count;
    private static boolean clip;
    private static final Set<String> DEFINED = new HashSet<>();
    private static final List<String> newDefinitions = new ArrayList<>();

    private static final class FrameBytes extends ByteArrayOutputStream {
        FrameBytes() {
            super(4096);
        }

        void truncate(int size) {
            count = size;
        }
    }

    public static final class Mark {
        final int size = bytes.size();
        final int commands = count;
        final int owned = hudBits;
        final boolean clip = Sink.clip;
        final int definitions = newDefinitions.size();
        final int matrixTop = top;
        final int matrixMode = mode;
        final float[] matrix = new float[(matrixTop + 1) * 3];
        final GlState.DrawState gl;
        final int[] cache;
        final boolean tessellating;
        final Object[][] scissors = Nvg.saveScissors();

        Mark(GlState.DrawState gl, int[] cache, boolean tessellating) {
            this.gl = gl;
            this.cache = cache;
            this.tessellating = tessellating;
            for (int i = 0; i <= matrixTop; i++) System.arraycopy(STACK[i], 0, matrix, i * 3, 3);
        }
    }

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
        if (Anchor.rect(x0, y0, x1, y1, argb)) return;
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

    /** A rectangle with the corner radii tl, tr, br, bl; the radii are scaled by the matrix like the box. */
    public static synchronized void roundRect(float x0, float y0, float x1, float y1, float[] radii, int argb) {
        float[] m = STACK[top];
        try {
            out.writeByte('S');
            out.writeByte(0);
            quad(m, x0, y0, x1, y1);
            for (float r : radii) out.writeFloat(r * m[0]);
            out.writeInt(argb);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /** A line of {@code width} from one point to the other, at any angle. */
    public static synchronized void line(float x0, float y0, float x1, float y1, float width, int argb) {
        float[] m = STACK[top];
        try {
            out.writeByte('S');
            out.writeByte(1);
            out.writeFloat(m[0] * x0 + m[1]);
            out.writeFloat(m[0] * y0 + m[2]);
            out.writeFloat(m[0] * x1 + m[1]);
            out.writeFloat(m[0] * y1 + m[2]);
            out.writeFloat(width * m[0]);
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
        if (Anchor.text(s, x, y, argb, shadow, m)) return;
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

    /** Whether {@link #image} sent {@code name} as a define command already, so its {@code data} may be null. */
    static synchronized boolean defined(String name) {
        return DEFINED.contains(name);
    }

    /** The image {@code name} (defined with {@code data} the first time) in the box x0 y0 x1 y1. */
    public static synchronized void image(String name, byte[] data, float x0, float y0, float x1, float y1, int argb, boolean recolor) {
        emit(name, data, x0, y0, x1, y1, argb, recolor, 0);
    }

    /** As above, with a blend code: 0 source alpha over, 1 inverts the destination, 2 darkens it, 3 adds (see {@link GlState}). */
    static synchronized void image(String name, byte[] data, float x0, float y0, float x1, float y1, int argb, boolean recolor, int blend) {
        emit(name, data, x0, y0, x1, y1, argb, recolor, blend);
    }

    private static void emit(String name, byte[] data, float x0, float y0, float x1, float y1, int argb,
            boolean recolor, int blend) {
        try {
            if (DEFINED.add(name)) {
                newDefinitions.add(name);
                out.writeByte('D');
                string(name);
                out.writeInt(data.length);
                out.write(data);
                count++;
            }
            out.writeByte('I');
            string(name);
            quad(STACK[top], x0, y0, x1, y1);
            out.writeInt(argb);
            out.writeByte(recolor ? 1 : 0);
            out.writeByte(blend);
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
        if (box == null && !clip) return;
        try {
            out.writeByte('C');
            out.writeByte(box == null ? 0 : 1);
            if (box != null) for (float v : box) out.writeFloat(v);
            clip = box != null;
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /** The vanilla HUD elements that Java owns this frame, as a bit set (see crates/modhost/src/hud.rs and {@link OverlayFilter}). */
    static synchronized void hud(int bits) {
        hudBits |= bits;
    }

    static synchronized Mark mark(GlState.DrawState gl, int[] cache, boolean tessellating) {
        return new Mark(gl, cache, tessellating);
    }

    static synchronized void rollback(Mark mark) {
        bytes.truncate(mark.size);
        count = mark.commands;
        hudBits = mark.owned;
        clip = mark.clip;
        Nvg.restoreScissors(mark.scissors);
        top = mark.matrixTop;
        mode = mark.matrixMode;
        for (int i = 0; i <= top; i++) System.arraycopy(mark.matrix, i * 3, STACK[i], 0, 3);
        while (newDefinitions.size() > mark.definitions) {
            String name = newDefinitions.remove(newDefinitions.size() - 1);
            DEFINED.remove(name);
        }
    }

    private static void writeHud(int bits) {
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
        if (hudBits != 0) writeHud(hudBits);
        hudBits = 0;
        byte[] body = bytes.toByteArray();
        ByteArrayOutputStream framed = new ByteArrayOutputStream(body.length + 5);
        try {
            framed.write(modScreen ? 1 : 0);
            new DataOutputStream(framed).writeInt(count);
            framed.write(body);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        bytes = new FrameBytes();
        out = new DataOutputStream(bytes);
        newDefinitions.clear();
        count = 0;
        top = 0;
        mode = MODELVIEW;
        clip = false;
        STACK[0][0] = 1;
        STACK[0][1] = 0;
        STACK[0][2] = 0;
        return framed.toByteArray();
    }

    static {
        STACK[0][0] = 1;
    }
}
