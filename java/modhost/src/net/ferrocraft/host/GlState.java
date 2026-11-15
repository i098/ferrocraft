package net.ferrocraft.host;

import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HashMap;
import java.util.Map;
import javax.imageio.ImageIO;

/**
 * The GL state that a textured GUI quad reads, for the stand-in that has no GL context: texture ids and the pixels
 * uploaded to them, the bound texture, the current color and the blend function. {@link GlPatch} routes the LWJGL
 * calls here. {@link #quad} is {@code Gui.drawScaledCustomSizeModalRect}: it cuts the quad's part out of the bound
 * texture and sends it to {@link Sink} as an image named by the SHA-256 of its pixels, so a {@code DynamicTexture} that
 * a mod draws its HUD with (PolyCrosshair) reaches Rust. Only level 0 of GL_TEXTURE_2D is kept, in the layout vanilla
 * uploads (BGRA, unsigned int 8-8-8-8 reversed: an ARGB int per pixel).
 */
public final class GlState {
    private static final int TEXTURE_2D = 3553;
    private static final int BLEND = 3042;
    private static final int BGRA = 32993;
    private static final int ARGB_INT = 33639;
    /** Bigger than 4096 x 4096 is no mod HUD texture; the texture is not kept. */
    private static final int MAX_PIXELS = 1 << 24;
    /** The most pixels of one quad's picture (a quad that tiles a small texture many times). */
    private static final int MAX_QUAD_PIXELS = 1 << 22;

    private static final class Tex {
        final int w;
        final int h;
        final int[] px;
        /** Region key to image name; cleared by every upload. */
        final Map<String, String> names = new HashMap<>();

        Tex(int w, int h) {
            this.w = w;
            this.h = h;
            px = new int[w * h];
        }
    }

    private static final Map<Integer, Tex> TEXTURES = new HashMap<>();
    private static int next = 1;
    private static int bound;
    private static float red = 1, green = 1, blue = 1, alpha = 1;
    private static boolean blend;
    private static int src = 770, dst = 771;

    private GlState() {
        throw new AssertionError();
    }

    public static synchronized int genTexture() {
        return next++;
    }

    public static synchronized void bindTexture(int target, int id) {
        if (target == TEXTURE_2D) bound = id;
    }

    public static synchronized void deleteTexture(int id) {
        TEXTURES.remove(id);
        if (bound == id) bound = 0;
    }

    /** {@code glTexImage2D}: a new level 0 for the bound texture, with {@code pixels} when given. */
    public static synchronized void texImage(int target, int level, int internal, int w, int h, int border, int format, int type, IntBuffer pixels) {
        if (target != TEXTURE_2D || level != 0 || bound == 0) return;
        if (w <= 0 || h <= 0 || (long) w * h > MAX_PIXELS) {
            TEXTURES.remove(bound);
            return;
        }
        Tex t = new Tex(w, h);
        TEXTURES.put(bound, t);
        texSubImage(target, level, 0, 0, w, h, format, type, pixels);
    }

    /** {@code glTexSubImage2D}: the rows of {@code pixels} into the bound texture. */
    public static synchronized void texSubImage(int target, int level, int x, int y, int w, int h, int format, int type, IntBuffer pixels) {
        Tex t = TEXTURES.get(bound);
        if (target != TEXTURE_2D || level != 0 || t == null || pixels == null || format != BGRA || type != ARGB_INT) return;
        if (x < 0 || y < 0 || w < 0 || h < 0 || x + w > t.w || y + h > t.h || pixels.remaining() < (long) w * h) return;
        IntBuffer from = pixels.duplicate();
        for (int row = 0; row < h; row++) from.get(t.px, (y + row) * t.w + x, w);
        t.names.clear();
    }

    public static synchronized void color(float r, float g, float b, float a) {
        red = r;
        green = g;
        blue = b;
        alpha = a;
    }

    public static synchronized void color3(float r, float g, float b) {
        color(r, g, b, 1);
    }

    public static synchronized void enable(int cap) {
        if (cap == BLEND) blend = true;
    }

    public static synchronized void disable(int cap) {
        if (cap == BLEND) blend = false;
    }

    public static synchronized void blendFunc(int s, int d) {
        src = s;
        dst = d;
    }

    public static synchronized void blendFuncSeparate(int s, int d, int sa, int da) {
        blendFunc(s, d);
    }

    /** {@code Gui.drawModalRectWithCustomSizedTexture}: the region is as big as the box. */
    public static void quadSized(int x, int y, float u, float v, int w, int h, float tw, float th) {
        quad(x, y, u, v, w, h, w, h, tw, th);
    }

    /**
     * {@code Gui.drawScaledCustomSizeModalRect}: the part {@code uw} x {@code vh} at {@code u},{@code v} of a texture
     * that the caller takes as {@code tw} x {@code th}, drawn in the box {@code w} x {@code h} at {@code x},{@code y}. The part is
     * sampled with nearest filtering and wraps at the texture edge, as the GL default of a dynamic texture does.
     * A bound texture that holds no pixels draws nothing.
     */
    public static synchronized void quad(int x, int y, float u, float v, int uw, int vh, int w, int h, float tw, float th) {
        Tex t = TEXTURES.get(bound);
        if (t == null || !drawable(uw, vh, w, h, tw, th)) return;
        float sx = u * t.w / tw, sy = v * t.h / th, sw = uw * t.w / tw, sh = vh * t.h / th;
        int ow = Math.max(1, Math.round(sw)), oh = Math.max(1, Math.round(sh));
        if ((long) ow * oh > MAX_QUAD_PIXELS) return;
        String key = sx + "," + sy + "," + sw + "," + sh;
        String name = t.names.get(key);
        int[] part = null;
        if (name == null) {
            part = region(t, sx, sy, sw, sh, ow, oh);
            name = "tex:" + sha256(ow, oh, part);
            t.names.put(key, name);
        }
        byte[] png = null;
        if (!Sink.defined(name)) {
            if (part == null) part = region(t, sx, sy, sw, sh, ow, oh);
            png = png(ow, oh, part);
        }
        Sink.image(name, png, x, y, x + w, y + h, tint(), false, blendMode());
    }

    private static boolean drawable(int uw, int vh, int w, int h, float tw, float th) {
        return tw > 0 && th > 0 && uw > 0 && vh > 0 && w != 0 && h != 0;
    }

    private static int[] region(Tex t, float sx, float sy, float sw, float sh, int ow, int oh) {
        int[] out = new int[ow * oh];
        for (int j = 0; j < oh; j++) {
            int ty = Math.floorMod((int) Math.floor(sy + (j + 0.5f) * sh / oh), t.h);
            for (int i = 0; i < ow; i++) {
                int tx = Math.floorMod((int) Math.floor(sx + (i + 0.5f) * sw / ow), t.w);
                out[j * ow + i] = t.px[ty * t.w + tx];
            }
        }
        return out;
    }

    private static int tint() {
        return channel(alpha) << 24 | channel(red) << 16 | channel(green) << 8 | channel(blue);
    }

    private static int channel(float c) {
        return Math.round(Math.max(0, Math.min(1, c)) * 255);
    }

    /** The blend code of the image command: 0 source alpha over, 1 inverts the destination, 2 darkens, 3 adds. */
    private static int blendMode() {
        if (!blend) return 0;
        if (src == 775 && dst == 769) return 1;
        if (src == 0 && dst == 769) return 2;
        if (src == 768 && dst == 1) return 3;
        return 0;
    }

    static String sha256(int w, int h, int[] px) {
        try {
            ByteBuffer b = ByteBuffer.allocate(8 + px.length * 4);
            b.putInt(w).putInt(h);
            b.asIntBuffer().put(px);
            byte[] d = MessageDigest.getInstance("SHA-256").digest(b.array());
            StringBuilder s = new StringBuilder(64);
            for (byte x : d) s.append(String.format("%02x", x));
            return s.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static byte[] png(int w, int h, int[] px) {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        img.setRGB(0, 0, w, h, px, 0, w);
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        try {
            ImageIO.write(img, "png", out);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        return out.toByteArray();
    }
}
