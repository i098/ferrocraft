package net.ferrocraft.host;

import java.lang.reflect.Method;
import java.util.List;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Label;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

/**
 * Runs {@link ShellTransformer} on a class that has the shape Mixin leaves in Minecraft, and runs the result. No FML needed.
 * The class has five handlers: {@code head} (may cancel), {@code body} (a block in the middle of the method), {@code nul}
 * (called with a null CallbackInfo), {@code loop} (inside a loop) and {@code tail} (cannot cancel). The shell must call all
 * but {@code loop} in order, run no vanilla instruction, report {@code loop} as lost, and verify.
 */
public final class ShellTest implements Opcodes {
    private static final String CI = "org/spongepowered/asm/mixin/injection/callback/CallbackInfo";
    private static final String FAKE = "Fake";
    private static final String MERGED = "Lorg/spongepowered/asm/mixin/transformer/meta/MixinMerged;";

    private ShellTest() {}

    public static void main(String[] args) throws Exception {
        byte[] shelled = ShellTransformer.shell(fake());
        Loader loader = new Loader();
        loader.define(CI, callbackInfo());
        Class<?> cls = loader.define(FAKE, shelled);
        Object o = cls.newInstance();
        cls.getMethod("func_71407_l").invoke(o);
        cls.getMethod("func_71411_J").invoke(o);
        check(log(cls).toString().equals("[head, body, nul, tail, head]"), "the shell runs the unconditional handlers in order, one handler at head and return twice, and no vanilla code: " + log(cls));
        // Host order: head part, then the body (here a marker), then the tail part.
        Loader second = new Loader();
        second.define(CI, callbackInfo());
        Class<?> parts = second.define(FAKE, shelled);
        Object p = parts.newInstance();
        parts.getMethod("func_71407_l$head").invoke(p);
        log(parts).add("MARK");
        parts.getMethod("func_71407_l$tail").invoke(p);
        check(log(parts).toString().equals("[head, body, nul, MARK, tail, head]"), "head handlers run before the body, return handlers after: " + log(parts));
        check(ShellTransformer.kept().size() == 5, "five handler call sites kept: " + ShellTransformer.kept());
        String lost = ShellTransformer.lost().toString();
        check(ShellTransformer.lost().size() == 3 && lost.contains("handler$cond") && lost.contains("handler$sw") && lost.contains("handler$loop") && lost.contains("Fake.func_71407_l"), "the handlers in the if, the switch case and the loop are lost: " + lost);
        System.out.println("ShellTest passed");
    }

    @SuppressWarnings("unchecked")
    private static List<String> log(Class<?> cls) throws Exception {
        return (List<String>) cls.getField("log").get(null);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static final class Loader extends ClassLoader {
        Class<?> define(String name, byte[] b) {
            return defineClass(name.replace('/', '.'), b, 0, b.length);
        }
    }

    /** A CallbackInfo that is never cancelled. */
    private static byte[] callbackInfo() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(V1_6, ACC_PUBLIC, CI, null, "java/lang/Object", null);
        MethodVisitor m = cw.visitMethod(ACC_PUBLIC, "<init>", "(Ljava/lang/String;Z)V", null, null);
        m.visitCode();
        m.visitVarInsn(ALOAD, 0);
        m.visitMethodInsn(INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false);
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        m = cw.visitMethod(ACC_PUBLIC, "isCancelled", "()Z", null, null);
        m.visitCode();
        m.visitInsn(ICONST_0);
        m.visitInsn(IRETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        cw.visitEnd();
        return cw.toByteArray();
    }

    private static byte[] fake() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(V1_8, ACC_PUBLIC, FAKE, null, "java/lang/Object", null);
        cw.visitField(ACC_PUBLIC | ACC_STATIC, "log", "Ljava/util/List;", null, null).visitEnd();
        MethodVisitor m = cw.visitMethod(ACC_STATIC, "<clinit>", "()V", null, null);
        m.visitCode();
        m.visitTypeInsn(NEW, "java/util/ArrayList");
        m.visitInsn(DUP);
        m.visitMethodInsn(INVOKESPECIAL, "java/util/ArrayList", "<init>", "()V", false);
        m.visitFieldInsn(PUTSTATIC, FAKE, "log", "Ljava/util/List;");
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        m = cw.visitMethod(ACC_PUBLIC, "<init>", "()V", null, null);
        m.visitCode();
        m.visitVarInsn(ALOAD, 0);
        m.visitMethodInsn(INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false);
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        for (String h : new String[] {"head", "body", "nul", "cond", "sw", "loop", "tail"}) {
            m = cw.visitMethod(ACC_PRIVATE, "handler$" + h, "(L" + CI + ";)V", null, null);
            m.visitAnnotation(MERGED, false).visitEnd();
            record(m, h);
        }
        m = cw.visitMethod(ACC_PRIVATE, "vanilla", "()V", null, null);
        record(m, "vanilla");
        // runTick: head block with the cancel check, vanilla code, a block in the body, vanilla code, return block, return.
        m = cw.visitMethod(ACC_PUBLIC, "func_71407_l", "()V", null, null);
        m.visitCode();
        block(m, "head", true);
        vanilla(m);
        block(m, "body", false);
        vanilla(m);
        m.visitVarInsn(ALOAD, 0);
        m.visitInsn(ACONST_NULL);
        m.visitMethodInsn(INVOKESPECIAL, FAKE, "handler$nul", "(L" + CI + ";)V", false);
        Label skip = new Label();
        m.visitInsn(ICONST_1);
        m.visitJumpInsn(IFEQ, skip);
        block(m, "cond", false);
        m.visitLabel(skip);
        m.visitFrame(F_NEW, 2, new Object[] {FAKE, CI}, 0, new Object[0]);
        Label inCase = new Label();
        Label endCase = new Label();
        m.visitInsn(ICONST_0);
        m.visitTableSwitchInsn(0, 0, endCase, inCase);
        m.visitLabel(inCase);
        m.visitFrame(F_NEW, 2, new Object[] {FAKE, CI}, 0, new Object[0]);
        block(m, "sw", false);
        m.visitLabel(endCase);
        m.visitFrame(F_NEW, 2, new Object[] {FAKE, CI}, 0, new Object[0]);
        Label again = new Label();
        m.visitLabel(again);
        m.visitFrame(F_NEW, 2, new Object[] {FAKE, CI}, 0, new Object[0]);
        vanilla(m);
        block(m, "loop", false);
        m.visitInsn(ICONST_0);
        m.visitJumpInsn(IFNE, again);
        block(m, "tail", false);
        block(m, "head", false);
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        // runGameLoop: vanilla code only.
        m = cw.visitMethod(ACC_PUBLIC, "func_71411_J", "()V", null, null);
        m.visitCode();
        vanilla(m);
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
        cw.visitEnd();
        return cw.toByteArray();
    }

    /** A handler that adds its name to the log. */
    private static void record(MethodVisitor m, String name) {
        m.visitCode();
        m.visitFieldInsn(GETSTATIC, FAKE, "log", "Ljava/util/List;");
        m.visitLdcInsn(name);
        m.visitMethodInsn(INVOKEINTERFACE, "java/util/List", "add", "(Ljava/lang/Object;)Z", true);
        m.visitInsn(POP);
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
    }

    private static void vanilla(MethodVisitor m) {
        m.visitVarInsn(ALOAD, 0);
        m.visitMethodInsn(INVOKESPECIAL, FAKE, "vanilla", "()V", false);
    }

    /** What Mixin emits for an injector: a CallbackInfo in local 1, the handler call and, when it may cancel, the check. */
    private static void block(MethodVisitor m, String handler, boolean cancellable) {
        m.visitTypeInsn(NEW, CI);
        m.visitInsn(DUP);
        m.visitLdcInsn("func_71407_l");
        m.visitInsn(cancellable ? ICONST_1 : ICONST_0);
        m.visitMethodInsn(INVOKESPECIAL, CI, "<init>", "(Ljava/lang/String;Z)V", false);
        m.visitVarInsn(ASTORE, 1);
        m.visitVarInsn(ALOAD, 0);
        m.visitVarInsn(ALOAD, 1);
        m.visitMethodInsn(INVOKESPECIAL, FAKE, "handler$" + handler, "(L" + CI + ";)V", false);
        if (cancellable) {
            Label after = new Label();
            m.visitVarInsn(ALOAD, 1);
            m.visitMethodInsn(INVOKEVIRTUAL, CI, "isCancelled", "()Z", false);
            m.visitJumpInsn(IFEQ, after);
            m.visitInsn(RETURN);
            m.visitLabel(after);
            m.visitFrame(F_NEW, 2, new Object[] {FAKE, CI}, 0, new Object[0]);
        }
    }
}
