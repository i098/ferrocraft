package net.ferrocraft.host;

import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import javax.imageio.ImageIO;

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
    /** The wire bit of the crosshair (crates/modhost/src/hud.rs). */
    private static final int CROSSHAIR = 1 << 4;
    private static int hudBits;
    private static int textured;
    private static int crosshairCoverage;
    private static Mark crosshairListener;
    private static boolean crosshairDrawn;
    private static final int MODELVIEW = 5888;
    private static final int DEPTH = 32;
    private static final float[][] STACK = new float[DEPTH][3];
    private static int top;
    private static int mode = MODELVIEW;
    private static FrameBytes bytes = new FrameBytes();
    private static DataOutputStream out = new DataOutputStream(bytes);
    private static int count;
    private static float[] clip;
    private static final Set<String> DEFINED = new HashSet<>();
    private static final List<String> newDefinitions = new ArrayList<>();
    private static final Map<String, BufferedImage> crosshairImages = new HashMap<>();

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
        final int textures = textured;
        final int coverage = crosshairCoverage;
        final boolean crosshair = crosshairDrawn;
        final float[] clip = Sink.clip;
        int centerX, centerY;
        float pixel;
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
            if (crosshairListener != null && visible(argb >>> 24)) centeredBox(m, x0, y0, x1, y1, argb, argb);
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
            if (crosshairListener != null && visible(argb >>> 24)) centeredRound(m, x0, y0, x1, y1, radii);
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
            if (crosshairListener != null && visible(argb >>> 24)) centeredLine(m, x0, y0, x1, y1, width);
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
            if (crosshairListener != null) centeredBox(m, x0, y0, x1, y1, top, bottom);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void centeredBox(float[] m, float x0, float y0, float x1, float y1, int top, int bottom) {
        int ax = wireRound(m[0] * x0 + m[1]), ay = wireRound(m[0] * y0 + m[2]);
        int bx = wireRound(m[0] * x1 + m[1]), by = wireRound(m[0] * y1 + m[2]);
        if (ay == by || !visible(Math.max(top >>> 24, bottom >>> 24))) return;
        cover(Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by), (x, y) -> {
            float fraction = (y - ay) / (by - ay);
            return visible((top >>> 24) * (1 - fraction) + (bottom >>> 24) * fraction);
        });
    }

    private static int wireRound(float value) {
        return value < 0 ? -Math.round(-value) : Math.round(value);
    }

    private static void centeredRound(float[] m, float x0, float y0, float x1, float y1, float[] radii) {
        float left = m[0] * x0 + m[1], upper = m[0] * y0 + m[2];
        float right = m[0] * x1 + m[1], lower = m[0] * y1 + m[2];
        if (!(left < right && upper < lower)) return;
        float tl = Math.max(0, radii[0] * m[0]), tr = Math.max(0, radii[1] * m[0]);
        float br = Math.max(0, radii[2] * m[0]), bl = Math.max(0, radii[3] * m[0]);
        float fit = Math.min(1, Math.min((right - left) / Math.max(tl + tr, bl + br),
                (lower - upper) / Math.max(tl + bl, tr + br)));
        cover(left, upper, right, lower, (x, y) ->
                insideCorner(x - left, y - upper, tl * fit) && insideCorner(right - x, y - upper, tr * fit)
                && insideCorner(right - x, lower - y, br * fit) && insideCorner(x - left, lower - y, bl * fit));
    }

    private static boolean insideCorner(float x, float y, float radius) {
        if (x >= radius || y >= radius) return true;
        float dx = radius - x, dy = radius - y;
        return dx * dx + dy * dy < radius * radius;
    }

    private static void centeredLine(float[] m, float x0, float y0, float x1, float y1, float width) {
        float ax = m[0] * x0 + m[1], ay = m[0] * y0 + m[2];
        float dx = m[0] * x1 + m[1] - ax, dy = m[0] * y1 + m[2] - ay;
        float lengthSq = dx * dx + dy * dy;
        if (!(lengthSq > 0)) return;
        float half = Math.max(width * m[0], crosshairListener.pixel) * 0.5f;
        float extentX = half * Math.abs(dy) / (float) Math.sqrt(lengthSq);
        float extentY = half * Math.abs(dx) / (float) Math.sqrt(lengthSq);
        cover(Math.min(ax, ax + dx) - extentX, Math.min(ay, ay + dy) - extentY,
                Math.max(ax, ax + dx) + extentX, Math.max(ay, ay + dy) + extentY, (x, y) -> {
                    float along = (x - ax) * dx + (y - ay) * dy;
                    float across = (x - ax) * dy - (y - ay) * dx;
                    return along >= 0 && along <= lengthSq && across * across < half * half * lengthSq;
                });
    }

    private static boolean visible(float alpha) {
        return alpha / 255f > 0.1f;
    }

    private interface Coverage {
        boolean test(float x, float y);
    }

    private static void cover(float x0, float y0, float x1, float y1, Coverage visibleAt) {
        Mark listener = crosshairListener;
        float x = listener.centerX, y = listener.centerY, step = listener.pixel;
        x0 = Math.max(x0, clipStart(0));
        y0 = Math.max(y0, clipStart(1));
        x1 = Math.min(x1, clipEnd(0, 2 * x + 1));
        y1 = Math.min(y1, clipEnd(1, 2 * y + 1));
        if (!hasArea(x0, y0, x1, y1) || hasCenteredCoverage()) return;
        boolean spansX = inRange(x, x0, x1);
        boolean spansY = inRange(y, y0, y1);
        if (spansX && spansY && visibleAt.test(x, y)) {
            crosshairCoverage = 15;
            return;
        }
        if (spansY) coverAxis(1, x, y, x0, x1, step, true, visibleAt);
        if (spansX) coverAxis(4, y, x, y0, y1, step, false, visibleAt);
    }

    private static float clipStart(int axis) {
        return clip == null ? 0 : Math.max(0, clip[axis]);
    }

    private static float clipEnd(int axis, float limit) {
        return clip == null ? limit : Math.min(limit, clip[axis + 2]);
    }

    private static boolean inRange(float value, float lo, float hi) {
        return lo <= value && value < hi;
    }

    private static boolean hasArea(float x0, float y0, float x1, float y1) {
        return x0 < x1 && y0 < y1;
    }

    private static void coverAxis(int bit, float center, float fixed, float lo, float hi, float step,
            boolean horizontal, Coverage visibleAt) {
        coverSide(bit, center, fixed, lo, Math.min(center, hi), step, horizontal, visibleAt);
        coverSide(bit << 1, center, fixed, Math.max(center + step, lo), hi, step, horizontal, visibleAt);
    }

    private static void coverSide(int bit, float center, float fixed, float lo, float hi, float step,
            boolean horizontal, Coverage visibleAt) {
        if ((crosshairCoverage & bit) != 0) return;
        int end = (int) Math.ceil((hi - center) / step);
        for (int i = (int) Math.ceil((lo - center) / step); i < end; i++) {
            float p = center + i * step;
            if (horizontal ? visibleAt.test(p, fixed) : visibleAt.test(fixed, p)) {
                crosshairCoverage |= bit;
                return;
            }
        }
    }

    private static boolean hasCenteredCoverage() {
        return (crosshairCoverage & 3) == 3 || (crosshairCoverage & 12) == 12;
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

    static synchronized boolean needsCrosshairImage(String name) {
        return (OverlayHooks.active() || crosshairListener != null) && !crosshairImages.containsKey(name);
    }

    /** The image {@code name} (defined with {@code data} the first time) in the box x0 y0 x1 y1. */
    public static synchronized void image(String name, byte[] data, float x0, float y0, float x1, float y1, int argb, boolean recolor) {
        emit(name, data, x0, y0, x1, y1, argb, recolor, 0, false);
    }

    /** As above, with a blend code: 0 source alpha over, 1 inverts the destination, 2 darkens it, 3 adds (see {@link GlState}). */
    static synchronized void image(String name, byte[] data, float x0, float y0, float x1, float y1, int argb, boolean recolor, int blend) {
        emit(name, data, x0, y0, x1, y1, argb, recolor, blend, true);
    }

    private static void emit(String name, byte[] data, float x0, float y0, float x1, float y1, int argb,
            boolean recolor, int blend, boolean texturedQuad) {
        float[] m = STACK[top];
        x0 = m[0] * x0 + m[1];
        y0 = m[0] * y0 + m[2];
        x1 = m[0] * x1 + m[1];
        y1 = m[0] * y1 + m[2];
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
            out.writeFloat(x0);
            out.writeFloat(y0);
            out.writeFloat(x1);
            out.writeFloat(y1);
            out.writeInt(argb);
            out.writeByte(recolor ? 1 : 0);
            out.writeByte(blend);
            count++;
            if (visible(argb >>> 24) && (crosshairListener != null || texturedQuad && OverlayHooks.active())) {
                BufferedImage image = crosshairImage(name, data);
                if (image != null) imageCoverage(image, x0, y0, x1, y1, argb >>> 24, texturedQuad);
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static BufferedImage crosshairImage(String name, byte[] data) {
        if (data != null && !crosshairImages.containsKey(name)) {
            crosshairImages.put(name, decodePng(data));
        }
        return crosshairImages.get(name);
    }

    private static BufferedImage decodePng(byte[] data) {
        if (data.length < 8 || data[0] != (byte) 0x89 || data[1] != 'P' || data[2] != 'N' || data[3] != 'G') return null;
        try {
            return ImageIO.read(new ByteArrayInputStream(data));
        } catch (IOException badImage) {
            return null;
        }
    }

    private static void imageCoverage(BufferedImage image, float x0, float y0, float x1, float y1, int alpha,
            boolean texturedQuad) {
        float left = x0, upper = y0, right = x1, lower = y1;
        if (clip != null) {
            left = Math.max(left, clip[0]);
            upper = Math.max(upper, clip[1]);
            right = Math.min(right, clip[2]);
            lower = Math.min(lower, clip[3]);
        }
        if (!(left < right && upper < lower)) return;
        float sx = image.getWidth() / (x1 - x0), sy = image.getHeight() / (y1 - y0);
        if (crosshairListener != null) {
            cover(left, upper, right, lower,
                    (x, y) -> visible((image.getRGB(Math.min(image.getWidth() - 1, (int) ((x - x0) * sx)),
                            Math.min(image.getHeight() - 1, (int) ((y - y0) * sy))) >>> 24) * alpha / 255f));
        }
        if (!texturedQuad) return;
        if (visibleTexel(image, alpha, (int) ((left - x0) * sx), (int) ((upper - y0) * sy),
                (int) Math.ceil((right - x0) * sx), (int) Math.ceil((lower - y0) * sy))) textured++;
    }

    private static boolean visibleTexel(BufferedImage image, int alpha, int startX, int startY, int endX, int endY) {
        endX = Math.min(image.getWidth(), endX);
        endY = Math.min(image.getHeight(), endY);
        for (int y = startY; y < endY; y++) {
            for (int x = startX; x < endX; x++) {
                if (visible((image.getRGB(x, y) >>> 24) * alpha / 255f)) return true;
            }
        }
        return false;
    }

    /** The box x0 y0 x1 y1 as the current matrix puts it on the screen. */
    public static synchronized float[] transform(float x0, float y0, float x1, float y1) {
        float[] m = STACK[top];
        return new float[] {m[0] * x0 + m[1], m[0] * y0 + m[2], m[0] * x1 + m[1], m[0] * y1 + m[2]};
    }

    /** Cuts what follows to {@code box} (x0 y0 x1 y1, already on the screen), or lifts the cut when null. */
    public static synchronized void clip(float[] box) {
        if (box == null && clip == null) return;
        try {
            out.writeByte('C');
            out.writeByte(box == null ? 0 : 1);
            if (box != null) for (float v : box) out.writeFloat(v);
            clip = box;
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
        textured = mark.textures;
        crosshairCoverage = mark.coverage;
        crosshairDrawn = mark.crosshair;
        clip = mark.clip;
        Nvg.restoreScissors(mark.scissors);
        top = mark.matrixTop;
        mode = mark.matrixMode;
        for (int i = 0; i <= top; i++) System.arraycopy(mark.matrix, i * 3, STACK[i], 0, 3);
        while (newDefinitions.size() > mark.definitions) {
            String name = newDefinitions.remove(newDefinitions.size() - 1);
            DEFINED.remove(name);
            crosshairImages.remove(name);
        }
    }

    static synchronized void crosshairDrawn(int before) {
        crosshairDrawn |= textured > before;
    }

    static synchronized Mark trackCrosshair(Mark mark, int x, int y, int factor) {
        Mark previous = crosshairListener;
        mark.centerX = x;
        mark.centerY = y;
        mark.pixel = 1f / Math.max(1, factor);
        crosshairCoverage = 0;
        crosshairListener = mark;
        return previous;
    }

    static synchronized void stopTrackingCrosshair(Mark mark, Mark previous) {
        crosshairCoverage = mark.coverage;
        crosshairListener = previous;
    }

    static synchronized void centeredCrosshairDrawn() {
        crosshairDrawn |= hasCenteredCoverage();
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
        int owned = crosshairDrawn ? hudBits : hudBits & ~CROSSHAIR;
        if (owned != 0) writeHud(owned);
        hudBits = 0;
        crosshairDrawn = false;
        textured = 0;
        crosshairCoverage = 0;
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
        clip = null;
        STACK[0][0] = 1;
        STACK[0][1] = 0;
        STACK[0][2] = 0;
        return framed.toByteArray();
    }

    static {
        STACK[0][0] = 1;
    }
}
