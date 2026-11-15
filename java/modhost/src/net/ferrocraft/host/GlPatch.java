package net.ferrocraft.host;

import java.util.HashMap;
import java.util.Map;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.MethodNode;

/**
 * The bodies {@link BridgeTransformer} gives to the LWJGL calls and the {@code Gui} quad methods that {@link GlState}
 * answers: texture upload, bind, color, blend, and the three textured quads. Keys are name plus descriptor; LWJGL names
 * are plain, {@code Gui} names are the SRG ones.
 */
final class GlPatch {
    private static final String STATE = "net/ferrocraft/host/GlState";
    private static final Map<String, String> GL = new HashMap<>();
    private static final Map<String, String> GUI = new HashMap<>();

    static {
        GL.put("glGenTextures()I", "genTexture");
        GL.put("glBindTexture(II)V", "bindTexture");
        GL.put("glDeleteTextures(I)V", "deleteTexture");
        GL.put("glTexImage2D(IIIIIIIILjava/nio/IntBuffer;)V", "texImage");
        GL.put("glTexSubImage2D(IIIIIIIILjava/nio/IntBuffer;)V", "texSubImage");
        GL.put("glColor4f(FFFF)V", "color");
        GL.put("glColor3f(FFF)V", "color3");
        GL.put("glEnable(I)V", "enable");
        GL.put("glDisable(I)V", "disable");
        GL.put("glBlendFunc(II)V", "blendFunc");
        GL.put("glBlendFuncSeparate(IIII)V", "blendFuncSeparate");
        GUI.put("func_152125_a(IIFFIIIIFF)V", "quad"); // Gui.drawScaledCustomSizeModalRect
        GUI.put("func_73729_b(IIIIII)V", "quad256"); // Gui.drawTexturedModalRect
        GUI.put("func_146110_a(IIFFIIFF)V", "quadSized"); // Gui.drawModalRectWithCustomSizedTexture
    }

    private GlPatch() {
        throw new AssertionError();
    }

    /** The body for an LWJGL call, or null when {@link GlState} does not answer it. */
    static InsnList gl(MethodNode m) {
        return forward(GL, m);
    }

    /** The body for a {@code Gui} method, or null when {@link GlState} does not answer it. */
    static InsnList gui(MethodNode m) {
        return forward(GUI, m);
    }

    private static InsnList forward(Map<String, String> table, MethodNode m) {
        String target = table.get(m.name + m.desc);
        return target == null ? null : BridgeTransformer.forward(m, STATE, target, m.desc);
    }
}
