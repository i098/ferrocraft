package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.IntBuffer;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

public final class OverlayHooksTest implements Opcodes {
    private static final String RENDERER = "net/minecraft/client/renderer/EntityRenderer";
    private static final String CALLBACK = "org/spongepowered/asm/mixin/injection/callback/CallbackInfo";
    private static final String TEST = "net/ferrocraft/host/OverlayHooksTest";
    private static final String DESC = "(FJL" + CALLBACK + ";D[I)V";
    private static final String FAILED = "failed HUD draw";
    private static final String LATER = "later HUD draw";

    public static void main(String[] args) throws Exception {
        Loader loader = new Loader();
        loader.define(CALLBACK, callbackBytes());
        byte[] renderer = new OverlayHooks.HudTransformer().transform(RENDERER, RENDERER.replace('/', '.'), rendererBytes());
        Class<?> cls = loader.define(RENDERER, renderer);
        GlState.bindTexture(3553, GlState.genTexture());
        GlState.texImage(3553, 0, 6408, 1, 1, 0, 32993, 33639, IntBuffer.wrap(new int[] {-1}));
        GlState.color(1, 1, 1, 1);
        GlState.enable(3042);
        GlState.blendFunc(775, 769);

        OverlayHooks.begin();
        try {
            cls.getMethod("func_181560_a", float.class, long.class).invoke(null, 0f, 0L);
            assertFrame();
            assertLateCrosshair();
            assertPropagates(cls.getMethod("unrelated"), IOException.class);
            assertPropagates(cls.getMethod("func_78478_c"), AssertionError.class);
        } finally {
            OverlayHooks.end();
        }
        assertPropagates(cls.getMethod("func_181560_a", float.class, long.class), IOException.class, 0f, 0L);
        System.out.println("OverlayHooksTest passed");
    }

    private static void assertLateCrosshair() throws IOException {
        RenderGameOverlayEvent.Post post = new RenderGameOverlayEvent.Post(new RenderGameOverlayEvent(0, null), ElementType.ALL);
        Sink.crosshairCenter(213, 120);
        assertLateImage(post, 2, 119, 0);
        assertLateImage(post, 212, 3, 0);
        assertLateImage(post, 212, 119, 1 << 4);
    }

    private static void assertLateImage(RenderGameOverlayEvent.Post post, int x, int y, int hud) throws IOException {
        Sink.hud(1 << 4);
        OverlayFilter.invoke(event -> GlState.quadSized(x, y, 0, 0, 3, 3, 3, 3), post);
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(new DataInputStream(new ByteArrayInputStream(Sink.take(false))));
        if (frame.hud != hud || frame.images.size() != 1) {
            throw new AssertionError("only a Post(ALL) image covering the crosshair center can retain ownership");
        }
    }

    private static void assertFrame() throws IOException {
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(new DataInputStream(new ByteArrayInputStream(Sink.take(false))));
        if (frame.texts.contains(FAILED) || !frame.texts.contains(LATER) || frame.hud != 0 || frame.images.size() != 1) {
            throw new AssertionError("failed HUD draw survived or the later HUD draw was lost");
        }
        MirrorHudTest.Image image = frame.images.get(0);
        if (image.argb != -1 || image.blend != 1 || image.x0 != 2 || image.y0 != 3) {
            throw new AssertionError("the failed callback changed the later image");
        }
    }

    private static void assertPropagates(Method method, Class<? extends Throwable> type, Object... args) throws Exception {
        try {
            method.invoke(null, args);
            throw new AssertionError("expected " + type.getSimpleName() + " from " + method.getName());
        } catch (InvocationTargetException failure) {
            if (!type.isInstance(failure.getCause())) throw failure;
        }
    }

    public static void fail(Object callback, int error) throws IOException {
        if (error != 0) throw new AssertionError("fatal HUD error");
        R.set(callback, "cancelled", true);
        Sink.hud(2);
        Sink.text(FAILED, 0, 0, -1, false);
        Sink.push();
        Sink.translate(50, 50);
        GlState.bindTexture(3553, 0);
        GlState.color(0.1f, 0.2f, 0.3f, 0);
        GlState.disable(3042);
        GlState.blendFunc(0, 769);
        throw new IOException("test HUD callback failed");
    }

    public static void later(Object callback) {
        if ((Boolean) R.get(callback, "cancelled")) return;
        Sink.text(LATER, 0, 0, -1, false);
        GlState.quadSized(2, 3, 0, 0, 1, 1, 1, 1);
    }

    private static byte[] rendererBytes() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(V1_8, ACC_PUBLIC, RENDERER, null, "java/lang/Object", null);
        MethodVisitor hud = cw.visitMethod(ACC_PUBLIC | ACC_STATIC, "func_181560_a", "(FJ)V", null, null);
        hud.visitMethodInsn(INVOKESTATIC, RENDERER, "hudHelper", "()V", false);
        finish(hud);
        entry(cw, "hudHelper", 0);
        entry(cw, "unrelated", 0);
        entry(cw, "func_78478_c", 1);
        MethodVisitor handler = cw.visitMethod(ACC_PRIVATE | ACC_STATIC, "handler$test", DESC, null, null);
        handler.visitAnnotation("Lorg/spongepowered/asm/mixin/transformer/meta/MixinMerged;", true).visitEnd();
        handler.visitVarInsn(ALOAD, 3);
        handler.visitVarInsn(DLOAD, 4);
        handler.visitInsn(D2I);
        handler.visitMethodInsn(INVOKESTATIC, TEST, "fail", "(Ljava/lang/Object;I)V", false);
        finish(handler);
        return cw.toByteArray();
    }

    private static void entry(ClassWriter cw, String name, int error) {
        MethodVisitor mv = cw.visitMethod(ACC_PUBLIC | ACC_STATIC, name, "()V", null, null);
        mv.visitTypeInsn(NEW, CALLBACK);
        mv.visitInsn(DUP);
        mv.visitMethodInsn(INVOKESPECIAL, CALLBACK, "<init>", "()V", false);
        mv.visitVarInsn(ASTORE, 0);
        mv.visitInsn(FCONST_0);
        mv.visitInsn(LCONST_0);
        mv.visitVarInsn(ALOAD, 0);
        mv.visitInsn(DCONST_0 + error);
        mv.visitInsn(ACONST_NULL);
        mv.visitMethodInsn(INVOKESTATIC, RENDERER, "handler$test", DESC, false);
        mv.visitVarInsn(ALOAD, 0);
        mv.visitMethodInsn(INVOKESTATIC, TEST, "later", "(Ljava/lang/Object;)V", false);
        finish(mv);
    }

    private static byte[] callbackBytes() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(V1_8, ACC_PUBLIC, CALLBACK, null, "java/lang/Object", null);
        cw.visitField(ACC_PRIVATE, "cancelled", "Z", null, null).visitEnd();
        MethodVisitor init = cw.visitMethod(ACC_PUBLIC, "<init>", "()V", null, null);
        init.visitVarInsn(ALOAD, 0);
        init.visitMethodInsn(INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false);
        finish(init);
        return cw.toByteArray();
    }

    private static void finish(MethodVisitor mv) {
        mv.visitInsn(RETURN);
        mv.visitMaxs(0, 0);
        mv.visitEnd();
    }

    private static final class Loader extends ClassLoader {
        Class<?> define(String name, byte[] bytes) {
            return defineClass(name.replace('/', '.'), bytes, 0, bytes.length);
        }
    }
}
