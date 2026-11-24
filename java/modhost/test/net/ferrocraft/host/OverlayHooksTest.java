package net.ferrocraft.host;

import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.IntBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import javax.imageio.ImageIO;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.client.event.RenderGameOverlayEvent.ElementType;
import net.minecraftforge.fml.common.eventhandler.IEventListener;
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

        OverlayHooks.begin(null);
        try {
            Method render = cls.getMethod("func_181560_a", float.class, long.class);
            render.invoke(null, 0f, 0L);
            assertRecoveredDraw(MirrorHudTest.readFrame(new DataInputStream(new ByteArrayInputStream(Sink.take(false)))));
            assertCrosshairCancellation();
            assertTessellatorRollback();
            assertScissorRollback();
            assertEventPayloadRollback();
            assertPropagates(cls.getMethod("unrelated", double.class), IOException.class, 0d);
            assertPropagates(render, AssertionError.class, 1f, 0L);
        } finally {
            OverlayHooks.end();
        }
        assertPropagates(cls.getMethod("func_181560_a", float.class, long.class), IOException.class, 0f, 0L);
        System.out.println("OverlayHooksTest passed");
    }

    private static void assertCrosshairCancellation() throws IOException {
        assertCrosshairFrame(event -> drawImage(), 0, 1);
        assertCrosshairFrame(event -> event.setCanceled(true), 1 << 4, 0);
        assertCrosshairFrame(event -> {
            event.setCanceled(true);
            drawImage();
            throw new IllegalStateException("test crosshair listener failed");
        }, 0, 0);
    }

    private static void assertCrosshairFrame(IEventListener listener, int hud, int images) throws IOException {
        RenderGameOverlayEvent.Pre crosshair = crosshairEvent();
        OverlayFilter.invoke(listener, crosshair);
        new OverlayFilter().pre(crosshair);
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(new DataInputStream(new ByteArrayInputStream(Sink.take(false))));
        if (frame.hud != hud || frame.images.size() != images) {
            throw new AssertionError("only a successful mod cancellation must suppress the Rust crosshair");
        }
    }

    private static RenderGameOverlayEvent.Pre crosshairEvent() {
        return new RenderGameOverlayEvent.Pre(new RenderGameOverlayEvent(0, null), ElementType.CROSSHAIRS) {
            @Override
            public boolean isCancelable() {
                return true;
            }
        };
    }

    private static void assertTessellatorRollback() {
        Object builder = R.call(R.call(R.cls("bfx"), "a"), "c");
        Object format = R.get(R.cls("bms"), "e");
        RenderGameOverlayEvent.Pre all = new RenderGameOverlayEvent.Pre(new RenderGameOverlayEvent(0, null), ElementType.ALL);
        OverlayFilter.invoke(event -> {
            R.call(builder, "a", 7, format);
            throw new IllegalStateException("listener left a quad open");
        }, all);
        R.call(builder, "a", 7, format);
        OverlayFilter.invoke(event -> {
            throw new IllegalStateException("listener failed inside the caller's quad");
        }, all);
        R.call(builder, "e");
        R.call(builder, "b");
    }

    @SuppressWarnings("unchecked")
    private static void assertScissorRollback() throws IOException {
        List<Object[]> scissors = (List<Object[]>) R.get(Nvg.class, "SCISSORS");
        Object outer = new Object(), inner = new Object(), failed = new Object();
        float[] box = {0, 0, 10, 10};
        scissors.add(new Object[] {outer, box});
        scissors.add(new Object[] {inner, new float[] {1, 1, 5, 5}});
        Nvg.resetScissor(1L, new Object());
        RenderGameOverlayEvent.Post post = new RenderGameOverlayEvent.Post(new RenderGameOverlayEvent(0, null), ElementType.ALL);
        OverlayFilter.invoke(event -> {
            Nvg.resetScissor(1L, outer);
            Nvg.clearScissors(1L);
            scissors.add(new Object[] {failed, new float[] {20, 20, 30, 30}});
            Nvg.resetScissor(1L, inner);
            throw new IllegalStateException("test scissor listener failed");
        }, post);
        drawImage();
        Nvg.resetScissor(1L, inner);
        drawImage();
        Nvg.clearScissors(1L);
        drawImage();
        MirrorHudTest.Frame frame = MirrorHudTest.readFrame(new DataInputStream(new ByteArrayInputStream(Sink.take(false))));
        if (!Arrays.equals(frame.images.get(0).clip, new float[] {1, 1, 5, 5})
                || !Arrays.equals(frame.images.get(1).clip, box) || frame.images.get(2).clip != null) {
            throw new AssertionError("failed handler changed the next draw's scissor stack");
        }
    }

    private static void assertEventPayloadRollback() {
        RenderGameOverlayEvent parent = new RenderGameOverlayEvent(0, null);
        RenderGameOverlayEvent.Chat chat = new RenderGameOverlayEvent.Chat(parent, 2, 3);
        OverlayFilter.invoke(event -> chat.posY = 4, chat);
        OverlayFilter.invoke(event -> {
            chat.posX = -100;
            chat.posY = -200;
            throw new IllegalStateException("test chat listener failed");
        }, chat);
        if (chat.posX != 2 || chat.posY != 4) throw new AssertionError("failed listener moved chat");

        List<String> expectedLeft = Arrays.asList("first", "second", "second");
        List<String> expectedRight = Collections.singletonList("kept");
        RenderGameOverlayEvent.Text text = new RenderGameOverlayEvent.Text(
                parent, new ArrayList<>(expectedLeft), new ArrayList<>());
        OverlayFilter.invoke(event -> text.right.add("kept"), text);
        OverlayFilter.invoke(event -> {
            text.left.clear();
            text.right.set(0, "failed");
            throw new IllegalStateException("test text listener failed");
        }, text);
        if (!text.left.equals(expectedLeft) || !text.right.equals(expectedRight)) {
            throw new AssertionError("failed listener changed overlay text");
        }
    }

    private static void assertRecoveredDraw(MirrorHudTest.Frame frame) throws IOException {
        if (frame.texts.contains(FAILED) || !frame.texts.contains(LATER)) {
            throw new AssertionError("failed HUD draw survived or the later HUD draw was lost");
        }
        if (frame.images.size() != 1 || frame.definitions.size() != 1 || frame.hud != 0) {
            throw new AssertionError("the failed draw left an image, a definition, or a HUD claim");
        }
        MirrorHudTest.Image image = frame.images.get(0);
        if (image.argb != -1 || image.blend != 1 || image.x0 != 2 || image.y0 != 3) {
            throw new AssertionError("the failed callback changed the later image");
        }
        byte[] png = frame.definitions.get(image.name);
        if (png == null) throw new AssertionError("the recovered image has no texture definition");
        BufferedImage pixels = ImageIO.read(new ByteArrayInputStream(png));
        if (pixels == null || pixels.getWidth() != 1 || pixels.getHeight() != 1 || pixels.getRGB(0, 0) != -1) {
            throw new AssertionError("the recovered image does not contain the uploaded white pixel");
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
        drawImage();
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

    private static void drawImage() {
        GlState.quadSized(2, 3, 0, 0, 1, 1, 1, 1);
    }

    private static byte[] rendererBytes() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(V1_8, ACC_PUBLIC, RENDERER, null, "java/lang/Object", null);
        MethodVisitor hud = cw.visitMethod(ACC_PUBLIC | ACC_STATIC, "func_181560_a", "(FJ)V", null, null);
        hud.visitVarInsn(FLOAD, 0);
        hud.visitInsn(F2D);
        hud.visitMethodInsn(INVOKESTATIC, RENDERER, "hudHelper", "(D)V", false);
        finish(hud);
        entry(cw, "hudHelper");
        entry(cw, "unrelated");
        MethodVisitor handler = cw.visitMethod(ACC_PRIVATE | ACC_STATIC, "handler$test", DESC, null, null);
        handler.visitAnnotation("Lorg/spongepowered/asm/mixin/transformer/meta/MixinMerged;", true).visitEnd();
        handler.visitVarInsn(ALOAD, 3);
        handler.visitVarInsn(DLOAD, 4);
        handler.visitInsn(D2I);
        handler.visitMethodInsn(INVOKESTATIC, TEST, "fail", "(Ljava/lang/Object;I)V", false);
        finish(handler);
        return cw.toByteArray();
    }

    private static void entry(ClassWriter cw, String name) {
        MethodVisitor mv = cw.visitMethod(ACC_PUBLIC | ACC_STATIC, name, "(D)V", null, null);
        mv.visitTypeInsn(NEW, CALLBACK);
        mv.visitInsn(DUP);
        mv.visitMethodInsn(INVOKESPECIAL, CALLBACK, "<init>", "()V", false);
        mv.visitVarInsn(ASTORE, 2);
        mv.visitInsn(FCONST_0);
        mv.visitInsn(LCONST_0);
        mv.visitVarInsn(ALOAD, 2);
        mv.visitVarInsn(DLOAD, 0);
        mv.visitInsn(ACONST_NULL);
        mv.visitMethodInsn(INVOKESTATIC, RENDERER, "handler$test", DESC, false);
        mv.visitVarInsn(ALOAD, 2);
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
