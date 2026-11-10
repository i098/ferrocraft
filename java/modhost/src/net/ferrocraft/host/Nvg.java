package net.ferrocraft.host;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.LongConsumer;

/**
 * Stands in for OneConfig's NanoVG renderer (NanoVGHelperImpl). Its calls become Sink commands, so
 * HUD mods built on OneConfig draw through the same path as plain Gui and FontRenderer mods.
 * Coordinates are GUI pixels. Rounded corners draw square: the wire has no rounded rect yet. Images and SVG files are
 * read from the class path and sent whole once; Rust decodes them.
 */
public final class Nvg {
    private static final int MODELVIEW = 5888;
    /** NanoVG font size that equals one vanilla glyph row. */
    private static final float VANILLA_SIZE = 9f;
    /** Resource bytes by name; an empty array means the resource is missing. */
    private static final Map<String, byte[]> RESOURCES = new HashMap<>();
    /** The scissors in force, innermost last: the Scissor object and its box on the screen. */
    private static final List<Object[]> SCISSORS = new ArrayList<>();

    private Nvg() {
        throw new AssertionError();
    }

    /**
     * A proxy of the OneConfig renderer interface {@code iface} whose methods call the static method of this class
     * with the same name and argument count. Methods this class does not have do nothing and return zero. OneConfig's
     * own implementations run in an isolated LWJGL 3 class loader that cannot start here, so every accessor of
     * its LwjglManager returns one of these.
     */
    public static Object stand(String iface) throws ClassNotFoundException {
        ClassLoader cl = Nvg.class.getClassLoader();
        Class<?> c = Class.forName(iface, false, cl);
        return Proxy.newProxyInstance(cl, new Class<?>[] {c}, (proxy, m, args) -> {
            for (Method own : Nvg.class.getDeclaredMethods()) {
                if (own.getName().equals(m.getName()) && own.getParameterCount() == m.getParameterCount()) {
                    return own.invoke(null, args);
                }
            }
            Class<?> r = m.getReturnType();
            if (r == boolean.class) return false;
            if (r == float.class) return 0f;
            if (r == int.class) return 0;
            if (r == long.class) return 0L;
            if (r == double.class) return 0d;
            return null;
        });
    }

    /** NanoVGHelper.setupAndDraw(body): a frame in window pixels. */
    public static void setupAndDraw(Object body) {
        setupAndDraw(false, body);
    }

    /** A frame in scaled GUI units when {@code mc}, in window pixels otherwise; the Sink takes GUI units. */
    public static void setupAndDraw(boolean mc, Object body) {
        Sink.matrixMode(MODELVIEW);
        Sink.push();
        try {
            if (!mc) {
                Object size = R.neu("net.minecraft.client.gui.ScaledResolution", R.get(R.cls("net.minecraft.client.Minecraft"), "field_71432_P"));
                Sink.scale(1f / (Integer) R.call(size, "func_78325_e")); // ScaledResolution.getScaleFactor
            }
            ((LongConsumer) body).accept(1L);
        } finally {
            Sink.pop();
            SCISSORS.clear();
            Sink.clip(null);
        }
    }

    public static void setupAndDraw(int flags, boolean mc, Object body) {
        setupAndDraw(mc, body);
    }

    public static void drawRect(long vg, float x, float y, float w, float h, int argb) {
        Sink.matrixMode(MODELVIEW);
        Sink.rect(Math.round(x), Math.round(y), Math.round(x + w), Math.round(y + h), argb);
    }

    public static void drawRoundedRect(long vg, float x, float y, float w, float h, int argb, float radius) {
        drawRect(vg, x, y, w, h, argb);
    }

    /** The four radii are dropped as in drawRoundedRect. */
    public static void drawRoundedRectVaried(long vg, float x, float y, float w, float h, int argb, float tl, float tr, float br, float bl) {
        drawRect(vg, x, y, w, h, argb);
    }

    /** A line as the box of its end points grown by half the width: right for the horizontal and vertical lines OneConfig draws. */
    public static void drawLine(long vg, float x0, float y0, float x1, float y1, float width, int argb) {
        float half = width / 2f;
        drawRect(vg, Math.min(x0, x1) - half, Math.min(y0, y1) - half, Math.abs(x1 - x0) + width, Math.abs(y1 - y0) + width, argb);
    }

    public static void translate(long vg, float x, float y) {
        Sink.matrixMode(MODELVIEW);
        Sink.translate(x, y);
    }

    public static void scale(long vg, float sx, float sy) {
        Sink.matrixMode(MODELVIEW);
        Sink.scale(sx);
    }

    public static void resetTransform(long vg) {
    }

    public static void setAlpha(long vg, float alpha) {
    }

    public static boolean isDrawing() {
        return true;
    }

    /** NanoVG text is centred on y: drop by half a glyph row, scaled to the requested size. */
    public static void drawText(long vg, String s, float x, float y, int argb, float size, Object font) {
        float k = size / VANILLA_SIZE;
        Sink.matrixMode(MODELVIEW);
        Sink.push();
        Sink.translate(x, y - 4f * k);
        Sink.scale(k);
        Sink.text(s, 0, 0, argb, false);
        Sink.pop();
    }

    public static void drawCenteredText(long vg, String s, float x, float y, int argb, float size, Object font) {
        drawText(vg, s, x - getTextWidth(vg, s, size, font) / 2f, y, argb, size, font);
    }

    public static float getTextWidth(long vg, String s, float size, Object font) {
        Object mc = R.call(R.cls("net.minecraft.client.Minecraft"), "func_71410_x");
        int w = (Integer) R.call(R.get(mc, "field_71466_p"), "func_78256_a", s);
        return w * size / VANILLA_SIZE;
    }

    /** GLPlatformImpl.drawRect: the corners x0 y0 x1 y1, as Gui.drawRect. */
    public static void glRect(float x0, float y0, float x1, float y1, int argb) {
        Sink.matrixMode(MODELVIEW);
        Sink.rect(Math.round(x0), Math.round(y0), Math.round(x1), Math.round(y1), argb);
    }

    public static void drawImage(long vg, Object file, float x, float y, float w, float h) {
        image(file, x, y, w, h, false, null, null);
    }

    /** The last argument is the ARGB tint or the Class whose loader finds the file. */
    public static void drawImage(long vg, Object file, float x, float y, float w, float h, Object a) {
        image(file, x, y, w, h, false, a, null);
    }

    public static void drawImage(long vg, Object file, float x, float y, float w, float h, Object a, Object b) {
        image(file, x, y, w, h, false, a, b);
    }

    /** The radius is dropped as in drawRoundedRect. */
    public static void drawRoundImage(long vg, Object file, float x, float y, float w, float h, float radius, Object a) {
        image(file, x, y, w, h, false, a, null);
    }

    public static void drawSvg(long vg, Object file, float x, float y, float w, float h) {
        image(file, x, y, w, h, true, null, null);
    }

    public static void drawSvg(long vg, Object file, float x, float y, float w, float h, Object a) {
        image(file, x, y, w, h, true, a, null);
    }

    public static void drawSvg(long vg, Object file, float x, float y, float w, float h, Object a, Object b) {
        image(file, x, y, w, h, true, a, b);
    }

    /** A file name, then a tint (an Integer) or a Class to load from, in either order. An SVG with a tint is painted in it. */
    private static void image(Object file, float x, float y, float w, float h, boolean svg, Object a, Object b) {
        if (!(file instanceof String)) return;
        Integer tint = null;
        Class<?> from = null;
        for (Object o : new Object[] {a, b}) {
            if (o instanceof Integer) tint = (Integer) o;
            else if (o instanceof Class) from = (Class<?>) o;
        }
        byte[] data = resource((String) file, from);
        if (data.length == 0) return;
        Sink.matrixMode(MODELVIEW);
        Sink.image((String) file, data, x, y, x + w, y + h, tint == null ? -1 : tint, svg && tint != null);
    }

    private static synchronized byte[] resource(String name, Class<?> from) {
        byte[] known = RESOURCES.get(name);
        if (known != null) return known;
        byte[] data = new byte[0];
        InputStream in = from == null ? null : from.getResourceAsStream(name);
        if (in == null) in = Nvg.class.getClassLoader().getResourceAsStream(name.startsWith("/") ? name.substring(1) : name);
        if (in == null) {
            Host.log("no resource " + name);
        } else {
            try (InputStream s = in) {
                ByteArrayOutputStream all = new ByteArrayOutputStream();
                byte[] chunk = new byte[8192];
                for (int n; (n = s.read(chunk)) > 0; ) all.write(chunk, 0, n);
                data = all.toByteArray();
            } catch (IOException e) {
                Host.log("cannot read " + name + ": " + e);
            }
        }
        RESOURCES.put(name, data);
        return data;
    }

    /** ScissorHelper.scissor: later drawing is cut to the box, inside the scissors already in force. */
    public static Object scissor(long vg, float x, float y, float w, float h) throws ReflectiveOperationException {
        Class<?> c = Class.forName("cc.polyfrost.oneconfig.renderer.scissor.Scissor", true, Nvg.class.getClassLoader());
        Object s = c.getConstructor(float.class, float.class, float.class, float.class).newInstance(x, y, w, h);
        Sink.matrixMode(MODELVIEW);
        SCISSORS.add(new Object[] {s, Sink.transform(x, y, x + w, y + h)});
        applyScissors();
        return s;
    }

    /** ScissorHelper.resetScissor: lifts one scissor; the others stay. */
    public static void resetScissor(long vg, Object scissor) {
        SCISSORS.removeIf(e -> e[0] == scissor);
        applyScissors();
    }

    public static void clearScissors(long vg) {
        SCISSORS.clear();
        applyScissors();
    }

    private static void applyScissors() {
        if (SCISSORS.isEmpty()) {
            Sink.clip(null);
            return;
        }
        float[] box = {-Float.MAX_VALUE, -Float.MAX_VALUE, Float.MAX_VALUE, Float.MAX_VALUE};
        for (Object[] e : SCISSORS) {
            float[] b = (float[]) e[1];
            box[0] = Math.max(box[0], Math.min(b[0], b[2]));
            box[1] = Math.max(box[1], Math.min(b[1], b[3]));
            box[2] = Math.min(box[2], Math.max(b[0], b[2]));
            box[3] = Math.min(box[3], Math.max(b[1], b[3]));
        }
        Sink.clip(box);
    }
}
