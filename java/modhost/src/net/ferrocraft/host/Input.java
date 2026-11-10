package net.ferrocraft.host;

/** Key, mouse and window state that Rust sends each frame. The patched LWJGL 2 classes read it. */
public final class Input {
    private static final boolean[] KEYS = new boolean[256];
    private static final boolean[] BUTTONS = new boolean[8];
    private static volatile int mouseX;
    private static volatile int mouseY;
    private static volatile int width = 854;
    private static volatile int height = 480;

    private Input() {}

    static void set(int w, int h, int mx, int my) {
        width = w;
        height = h;
        mouseX = mx;
        mouseY = my;
    }

    static void key(int code, boolean down) {
        if (code >= 0 && code < KEYS.length) KEYS[code] = down;
    }

    static void button(int b, boolean down) {
        if (b >= 0 && b < BUTTONS.length) BUTTONS[b] = down;
    }

    static boolean keyState(int code) {
        return code >= 0 && code < KEYS.length && KEYS[code];
    }

    public static boolean isKeyDown(int code) {
        return keyState(code);
    }

    public static boolean isButtonDown(int b) {
        return b >= 0 && b < BUTTONS.length && BUTTONS[b];
    }

    public static int mouseX() {
        return mouseX;
    }

    /** LWJGL puts the mouse origin at the bottom left. */
    public static int mouseY() {
        return height - 1 - mouseY;
    }

    public static int width() {
        return width;
    }

    public static int height() {
        return height;
    }
}
