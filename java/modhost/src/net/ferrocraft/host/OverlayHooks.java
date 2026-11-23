package net.ferrocraft.host;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
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
    private static final String EXCEPTION = "java/lang/Exception";
    private static final String GUI = "net.minecraft.client.gui.GuiIngame";
    private static final String FORGE_GUI = "net.minecraftforge.client.GuiIngameForge";
    private static final String RENDERER = "net.minecraft.client.renderer.EntityRenderer";
    private static final String MARK = "net/ferrocraft/host/Sink$Mark";
    private static Thread overlayThread;
    private static GlCache glCache;

    private OverlayHooks() {}

    static void begin(GlCache cache, int centerX, int centerY) {
        Sink.center(centerX, centerY);
        glCache = cache;
        overlayThread = Thread.currentThread();
    }

    static void end() {
        overlayThread = null;
        glCache = null;
    }

    static boolean active() {
        return overlayThread == Thread.currentThread();
    }

    static void failed(Exception failure, Sink.Mark mark) {
        Sink.rollback(mark);
        GlState.restoreDrawState(mark.gl);
        if (glCache != null) glCache.restore(mark.cache);
        failure.printStackTrace();
    }

    public static Sink.Mark checkpoint() {
        return active() ? Sink.mark(GlState.saveDrawState(), glCache == null ? null : glCache.save()) : null;
    }

    public static void crosshairCompleted(Sink.Mark mark) {
        if (mark != null) Sink.crosshairDrawn(mark.textures);
    }

    public static void mixinFailed(Exception failure, Object callback, Sink.Mark mark) throws Exception {
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
        if (!name.equals(GUI) && !name.equals(RENDERER) && !name.equals(FORGE_GUI)) return bytes;
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, 0);
        Map<String, MethodNode> methods = new HashMap<>();
        for (MethodNode method : (List<MethodNode>) cls.methods) methods.put(method.name + method.desc, method);
        Map<MethodNode, MethodNode> wrappers = new HashMap<>();
        for (MethodNode method : hudMethods(name, cls.name, methods)) {
            wrapCalls(cls.name, method, methods, wrappers);
        }
        if (wrappers.isEmpty()) return bytes;
        cls.methods.addAll(wrappers.values());
        return write(cls);
    }

    private static List<MethodNode> hudMethods(String name, String owner, Map<String, MethodNode> methods) {
        Set<String> roots = hudRoots(name);
        ArrayDeque<MethodNode> pending = new ArrayDeque<>();
        Set<MethodNode> seen = new HashSet<>();
        List<MethodNode> hud = new ArrayList<>();
        for (MethodNode method : methods.values()) {
            if (roots.contains(method.name) || roots.contains(method.name + method.desc)) pending.add(method);
        }
        while (!pending.isEmpty()) {
            MethodNode method = pending.removeFirst();
            if (!seen.add(method) || method.name.startsWith("handler$")) continue;
            hud.add(method);
            for (MethodInsnNode call : localCalls(owner, method)) {
                MethodNode target = methods.get(call.name + call.desc);
                if (target != null) pending.add(target);
            }
        }
        return hud;
    }

    private static Set<String> hudRoots(String name) {
        if (name.equals(RENDERER)) return new HashSet<>(Arrays.asList("func_181560_a(FJ)V", "func_78478_c()V"));
        if (name.equals(FORGE_GUI)) return java.util.Collections.singleton("func_175180_a(F)V");
        return new HashSet<>(Arrays.asList("func_175183_b()Z", "func_180476_e",
                "func_180480_a", "func_180479_a"));
    }

    private static List<MethodInsnNode> localCalls(String owner, MethodNode method) {
        List<MethodInsnNode> calls = new ArrayList<>();
        for (AbstractInsnNode insn : method.instructions.toArray()) {
            if (insn instanceof MethodInsnNode && ((MethodInsnNode) insn).owner.equals(owner)) {
                calls.add((MethodInsnNode) insn);
            }
        }
        return calls;
    }

    private static void wrapCalls(String owner, MethodNode method, Map<String, MethodNode> methods,
            Map<MethodNode, MethodNode> wrappers) {
        for (MethodInsnNode call : localCalls(owner, method)) {
            if (!call.name.startsWith("handler$") || !call.desc.endsWith(")V")) continue;
            MethodNode handler = methods.get(call.name + call.desc);
            if (handler == null || !ShellTransformer.isMerged(handler)) continue;
            MethodNode wrapper = wrappers.computeIfAbsent(handler, h -> wrap(owner, h));
            if (wrapper != null) call.name = wrapper.name;
        }
    }

    private static boolean drawsPolyCrosshair(MethodNode handler) {
        for (AbstractInsnNode insn : handler.instructions.toArray()) {
            if (!(insn instanceof MethodInsnNode)) continue;
            MethodInsnNode call = (MethodInsnNode) insn;
            if (call.owner.equals("org/polyfrost/crosshair/render/CrosshairRenderer") && call.name.equals("drawCrosshair")
                    && call.desc.equals("(Lnet/minecraft/client/renderer/EntityRenderer;)V")) return true;
        }
        return false;
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

        MethodNode wrapper = new MethodNode(handler.access, handler.name + "$ferroOverlay", handler.desc, null, null);
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
        wrapper.visitTryCatchBlock(start, end, caught, EXCEPTION);
        wrapper.visitLabel(start);
        wrapper.visitMethodInsn(isStatic ? Opcodes.INVOKESTATIC : Opcodes.INVOKESPECIAL,
                owner, handler.name, handler.desc, false);
        wrapper.visitLabel(end);
        if (drawsPolyCrosshair(handler)) {
            wrapper.visitVarInsn(Opcodes.ALOAD, mark);
            wrapper.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/OverlayHooks", "crosshairCompleted",
                    "(L" + MARK + ";)V", false);
        }
        wrapper.visitInsn(Opcodes.RETURN);
        wrapper.visitLabel(caught);
        Object[] locals = handlerLocals(owner, isStatic, args);
        wrapper.visitFrame(Opcodes.F_FULL, locals.length, locals, 1, new Object[] {EXCEPTION});
        wrapper.visitVarInsn(Opcodes.ALOAD, callback);
        wrapper.visitVarInsn(Opcodes.ALOAD, mark);
        wrapper.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/OverlayHooks", "mixinFailed",
                "(Ljava/lang/Exception;Ljava/lang/Object;L" + MARK + ";)V", false);
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
