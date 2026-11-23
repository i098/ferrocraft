package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.List;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Label;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;

public final class OverlayHooks {
    private static final String EVENT_HANDLER = "net/minecraftforge/fml/common/eventhandler/";
    private static final String CALLBACK = "Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo";
    private static final String RUNTIME_EXCEPTION = "java/lang/RuntimeException";
    private static Thread overlayThread;

    private OverlayHooks() {}

    static void begin() {
        overlayThread = Thread.currentThread();
    }

    static void end() {
        overlayThread = null;
    }

    static boolean active() {
        return overlayThread == Thread.currentThread();
    }

    static void failed(RuntimeException failure) {
        Sink.overlayFailed();
        failure.printStackTrace();
    }

    public static void mixinFailed(RuntimeException failure, Object callback) {
        if (!active()) throw failure;
        if (callback != null) R.set(callback, "cancelled", false);
        failed(failure);
    }

    static byte[] patchEventBus(byte[] bytes) {
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, 0);
        for (Object item : cls.methods) {
            MethodNode method = (MethodNode) item;
            if (!method.name.equals("post") || !method.desc.equals("(L" + EVENT_HANDLER + "Event;)Z")) continue;
            for (AbstractInsnNode insn : method.instructions.toArray()) {
                if (!(insn instanceof MethodInsnNode)) continue;
                MethodInsnNode call = (MethodInsnNode) insn;
                if (call.owner.equals(EVENT_HANDLER + "IEventListener") && call.name.equals("invoke")) {
                    method.instructions.set(call, new MethodInsnNode(Opcodes.INVOKESTATIC,
                            "net/ferrocraft/host/OverlayFilter", "invoke",
                            "(L" + EVENT_HANDLER + "IEventListener;L" + EVENT_HANDLER + "Event;)V", false));
                    return write(cls);
                }
            }
        }
        return bytes;
    }

    @SuppressWarnings("unchecked")
    static byte[] patchHud(String name, byte[] bytes) {
        if (!name.equals(Names.of("net.minecraft.client.gui.GuiIngame"))
                && !name.equals(Names.of("net.minecraft.client.renderer.EntityRenderer"))
                && !name.equals("net.minecraftforge.client.GuiIngameForge")) return bytes;
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, 0);
        List<MethodNode> wrappers = new ArrayList<>();
        for (Object item : cls.methods) {
            MethodNode method = (MethodNode) item;
            if (!method.name.startsWith("handler$") || !method.desc.endsWith(")V") || !ShellTransformer.isMerged(method)) continue;
            MethodNode wrapper = wrap(cls.name, method);
            if (wrapper != null) wrappers.add(wrapper);
        }
        if (wrappers.isEmpty()) return bytes;
        cls.methods.addAll(wrappers);
        return write(cls);
    }

    private static MethodNode wrap(String owner, MethodNode handler) {
        Type[] args = Type.getArgumentTypes(handler.desc);
        boolean isStatic = (handler.access & Opcodes.ACC_STATIC) != 0;
        int callback = -1;
        int slot = isStatic ? 0 : 1;
        for (Type arg : args) {
            if (arg.getDescriptor().equals(CALLBACK + ";") || arg.getDescriptor().equals(CALLBACK + "Returnable;")) {
                callback = slot;
                break;
            }
            slot += arg.getSize();
        }
        if (callback == -1) return null;

        MethodNode wrapper = new MethodNode(handler.access, handler.name, handler.desc, null, null);
        handler.name += "$ferroOverlay";
        if (!isStatic) wrapper.visitVarInsn(Opcodes.ALOAD, 0);
        slot = isStatic ? 0 : 1;
        for (Type arg : args) {
            wrapper.visitVarInsn(arg.getOpcode(Opcodes.ILOAD), slot);
            slot += arg.getSize();
        }
        Label start = new Label();
        Label end = new Label();
        Label caught = new Label();
        wrapper.visitTryCatchBlock(start, end, caught, RUNTIME_EXCEPTION);
        wrapper.visitLabel(start);
        wrapper.visitMethodInsn(isStatic ? Opcodes.INVOKESTATIC : Opcodes.INVOKESPECIAL,
                owner, handler.name, handler.desc, false);
        wrapper.visitLabel(end);
        wrapper.visitInsn(Opcodes.RETURN);
        wrapper.visitLabel(caught);
        wrapper.visitFrame(Opcodes.F_SAME1, 0, null, 1, new Object[] {RUNTIME_EXCEPTION});
        wrapper.visitVarInsn(Opcodes.ALOAD, callback);
        wrapper.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/OverlayHooks", "mixinFailed",
                "(Ljava/lang/RuntimeException;Ljava/lang/Object;)V", false);
        wrapper.visitInsn(Opcodes.RETURN);
        return wrapper;
    }

    private static byte[] write(ClassNode cls) {
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cls.accept(writer);
        return writer.toByteArray();
    }
}
