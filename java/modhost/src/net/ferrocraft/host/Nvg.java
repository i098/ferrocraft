package net.ferrocraft.host;

import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.function.LongConsumer;

/**
 * Stands in for OneConfig's NanoVG renderer (NanoVGHelperImpl). Its calls become Sink commands, so
 * HUD mods built on OneConfig draw through the same path as plain Gui and FontRenderer mods.
 * Coordinates are GUI pixels. Rounded corners draw square: the wire has no rounded rect yet.
 */
public final class Nvg {
    private static final int MODELVIEW = 5888;
    /** NanoVG font size that equals one vanilla glyph row. */
    private static final float VANILLA_SIZE = 9f;

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

    /** ScissorHelper.scissor: the wire has no clip rectangle yet, so nothing clips; the caller still gets its Scissor. */
    public static Object scissor(long vg, float x, float y, float w, float h) throws ReflectiveOperationException {
        Class<?> c = Class.forName("cc.polyfrost.oneconfig.renderer.scissor.Scissor", true, Nvg.class.getClassLoader());
        return c.getConstructor(float.class, float.class, float.class, float.class).newInstance(x, y, w, h);
    }
}
