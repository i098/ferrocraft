package org.lwjgl.opengl;

/** LWJGL 3's {@code GL}, for the mods that ask what the context supports; the host has no context. */
public final class GL {
    private static final GLCapabilities CAPABILITIES = new GLCapabilities();

    private GL() {}

    public static GLCapabilities getCapabilities() {
        return CAPABILITIES;
    }
}
