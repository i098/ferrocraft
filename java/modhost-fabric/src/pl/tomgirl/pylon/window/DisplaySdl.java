package pl.tomgirl.pylon.window;

import org.lwjgl.opengl.DisplayMode;

/**
 * What OneConfig reads of Pylon's SDL window. The host owns the window and the input (Rust draws and reads them), so Pylon is on
 * the deny list and no SDL window exists: every setting is accepted and dropped, every size is 0, there is no handle.
 */
public final class DisplaySdl {
    private static final DisplaySdl INSTANCE = new DisplaySdl();

    private DisplaySdl() {}

    public static DisplaySdl instance() {
        return INSTANCE;
    }

    public long getHandle() {
        return 0L;
    }

    public int getWidth() {
        return 0;
    }

    public int getHeight() {
        return 0;
    }

    public int getWindowWidth() {
        return 0;
    }

    public int getWindowHeight() {
        return 0;
    }

    public boolean isExclusiveFullscreen() {
        return false;
    }

    public DisplayMode getFullscreenDisplayMode() {
        return null;
    }

    public void setExclusiveFullscreen(boolean on) {}

    public void setFullscreenDisplayMode(DisplayMode mode) {}

    public void setHighPixelDensity(boolean on) {}

    public void setTextInputActive(boolean on) {}

    public void setWindowHint(String name, String value) {}
}
