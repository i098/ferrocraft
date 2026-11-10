package net.ferrocraft.host;

import java.util.ArrayDeque;

/**
 * Key, mouse and window state that Rust sends each frame. The patched LWJGL 2 classes read it: the
 * polled state directly, and the event queues behind {@code Keyboard.next} and {@code Mouse.next}.
 */
public final class Input {
    private static final boolean[] KEYS = new boolean[256];
    private static final boolean[] BUTTONS = new boolean[8];
    /** Key events: key, down, repeat. */
    private static final ArrayDeque<int[]> KEY_EVENTS = new ArrayDeque<>();
    /** Mouse events: button (-1 for the wheel), down, wheel, x, y, dx, dy. */
    private static final ArrayDeque<int[]> MOUSE_EVENTS = new ArrayDeque<>();
    private static int[] keyEvent = new int[3];
    private static int[] mouseEvent = new int[7];
    private static boolean repeatEvents;
    private static int lastEventX;
    private static int lastEventY;
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

    /** One transition from Rust: kind 'K', 'B' or 'W', see wire.rs. */
    static synchronized void event(int kind, int arg, int state) {
        boolean down = (state & 1) != 0;
        switch (kind) {
            case 'K':
                if (arg >= 0 && arg < KEYS.length) KEYS[arg] = down;
                KEY_EVENTS.add(new int[] {arg, down ? 1 : 0, (state & 2) != 0 ? 1 : 0});
                break;
            case 'B':
                if (arg >= 0 && arg < BUTTONS.length) BUTTONS[arg] = down;
                mouse(arg, down ? 1 : 0, 0);
                break;
            case 'W':
                mouse(-1, 0, arg);
                break;
            default:
                throw new IllegalStateException("unknown event kind " + kind);
        }
    }

    private static void mouse(int button, int down, int wheel) {
        int x = mouseX;
        int y = mouseY();
        MOUSE_EVENTS.add(new int[] {button, down, wheel, x, y, x - lastEventX, y - lastEventY});
        lastEventX = x;
        lastEventY = y;
    }

    public static synchronized boolean nextKey() {
        int[] e;
        do {
            e = KEY_EVENTS.poll();
            if (e == null) return false;
        } while (e[2] != 0 && !repeatEvents);
        keyEvent = e;
        return true;
    }

    public static synchronized boolean nextMouse() {
        int[] e = MOUSE_EVENTS.poll();
        if (e == null) return false;
        mouseEvent = e;
        return true;
    }

    public static synchronized void repeatEvents(boolean enable) {
        repeatEvents = enable;
    }

    public static synchronized boolean repeatEventsEnabled() {
        return repeatEvents;
    }

    public static synchronized int eventKey() {
        return keyEvent[0];
    }

    public static synchronized boolean eventKeyDown() {
        return keyEvent[1] != 0;
    }

    public static synchronized boolean eventRepeat() {
        return keyEvent[2] != 0;
    }

    public static synchronized int eventButton() {
        return mouseEvent[0];
    }

    public static synchronized boolean eventButtonDown() {
        return mouseEvent[1] != 0;
    }

    public static synchronized int eventDWheel() {
        return mouseEvent[2];
    }

    public static synchronized int eventX() {
        return mouseEvent[3];
    }

    public static synchronized int eventY() {
        return mouseEvent[4];
    }

    public static synchronized int eventDX() {
        return mouseEvent[5];
    }

    public static synchronized int eventDY() {
        return mouseEvent[6];
    }

    public static boolean isKeyDown(int code) {
        return code >= 0 && code < KEYS.length && KEYS[code];
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
