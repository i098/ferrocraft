package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.launchwrapper.IClassTransformer;
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
    private static final String MARK = "net/ferrocraft/host/Sink$Mark";
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

    static void failed(RuntimeException failure, Sink.Mark mark) {
        Sink.rollback(mark);
        failure.printStackTrace();
    }

    public static Sink.Mark checkpoint() {
        return active() ? Sink.mark() : null;
    }

    public static void mixinFailed(RuntimeException failure, Object callback, Sink.Mark mark) {
        if (mark == null) throw failure;
        if (callback != null) R.set(callback, "cancelled", false);
        failed(failure, mark);
    }

    public static final class EventBusTransformer implements IClassTransformer {
        @Override
        public byte[] transform(String name, String transformedName, byte[] bytes) {
            return bytes != null && transformedName.equals("net.minecraftforge.fml.common.eventhandler.EventBus")
                    ? patchEventBus(bytes) : bytes;
        }
    }

    public static final class HudTransformer implements IClassTransformer {
        @Override
        public byte[] transform(String name, String transformedName, byte[] bytes) {
            return bytes == null ? null : patchHud(transformedName, bytes);
        }
    }

    private static byte[] patchEventBus(byte[] bytes) {
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
    private static byte[] patchHud(String name, byte[] bytes) {
        if (!name.equals("net.minecraft.client.gui.GuiIngame")
                && !name.equals("net.minecraft.client.renderer.EntityRenderer")
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
            }
            slot += arg.getSize();
        }
        if (callback == -1) return null;

        MethodNode wrapper = new MethodNode(handler.access, handler.name, handler.desc, null, null);
        handler.name += "$ferroOverlay";
        int mark = slot;
        wrapper.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/OverlayHooks", "checkpoint", "()L" + MARK + ";", false);
        wrapper.visitVarInsn(Opcodes.ASTORE, mark);
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
        Object[] locals = handlerLocals(owner, isStatic, args);
        wrapper.visitFrame(Opcodes.F_FULL, locals.length, locals, 1, new Object[] {RUNTIME_EXCEPTION});
        wrapper.visitVarInsn(Opcodes.ALOAD, callback);
        wrapper.visitVarInsn(Opcodes.ALOAD, mark);
        wrapper.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/OverlayHooks", "mixinFailed",
                "(Ljava/lang/RuntimeException;Ljava/lang/Object;L" + MARK + ";)V", false);
        wrapper.visitInsn(Opcodes.RETURN);
        return wrapper;
    }

    private static Object[] handlerLocals(String owner, boolean isStatic, Type[] args) {
        Object[] locals = new Object[args.length + (isStatic ? 1 : 2)];
        int index = 0;
        if (!isStatic) locals[index++] = owner;
        for (Type arg : args) {
            switch (arg.getSort()) {
                case Type.ARRAY:
                case Type.OBJECT: locals[index++] = arg.getInternalName(); break;
                case Type.LONG: locals[index++] = Opcodes.LONG; break;
                case Type.DOUBLE: locals[index++] = Opcodes.DOUBLE; break;
                case Type.FLOAT: locals[index++] = Opcodes.FLOAT; break;
                default: locals[index++] = Opcodes.INTEGER;
            }
        }
        locals[index] = MARK;
        return locals;
    }

    private static byte[] write(ClassNode cls) {
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cls.accept(writer);
        return writer.toByteArray();
    }
}
