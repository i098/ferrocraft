package org.lwjgl.opengl;

/**
 * LWJGL 2's {@code Display}, for the mods that read the window. The host owns the window (Rust draws and reads it), and the
 * real class needs a native library and a display at class load: every size is 0, every setting is accepted and dropped.
 */
public final class Display {
    private Display() {}

    public static boolean isCreated() {
        return true;
    }

    public static boolean isActive() {
        return true;
    }

    public static boolean isVisible() {
        return true;
    }

    public static boolean isCloseRequested() {
        return false;
    }

    public static int getWidth() {
        return 0;
    }

    public static int getHeight() {
        return 0;
    }

    public static DisplayMode[] getAvailableDisplayModes() {
        return new DisplayMode[0];
    }

    public static void setFullscreen(boolean fullscreen) {}

    public static void setVSyncEnabled(boolean sync) {}

    public static void sync(int fps) {}
}
