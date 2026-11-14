package net.ferrocraft.host;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.VarInsnNode;

/**
 * Draws in the world: name tags, the TNT countdown and the lines of hitboxes. Rust sends the entities in view; this
 * runs the real vanilla methods that mods changed with Mixin ({@code Render.renderLivingLabel},
 * {@code RenderTNTPrimed.doRender}, {@code RenderManager.renderDebugBoundingBox}) on stand-in entities, with the
 * entity at the origin, and records what they draw: the text of {@link Sink#text} and the vertices of
 * {@code WorldRenderer} (patched by {@link #patch}). Rust projects and draws the result.
 *
 * <p>A label hangs billboard-like: its text and plates are recorded as an offset to the right and up, in blocks,
 * from the entity's feet. A hitbox is recorded as lines, as offsets from the feet in the world axes.
 *
 * <p>Request, after the tag 'A': see crates/modhost/src/anchor.rs. Reply: u32 item count, then 'L' labels and 'W' lines.
 * Subject drawing that throws (a stand-in lacks something the code reads) is logged once per role and dropped.
 */
public final class Anchor {
    private static final int LABEL = 1;
    private static final int COUNTDOWN = 2;
    private static final int HITBOX = 4;
    private static final String RENDER_TNT = "net.minecraft.client.renderer.entity.RenderTNTPrimed";
    /** The stand-in vanilla class of each wire class (player, other, primed TNT). */
    private static final String[] ENTITY = {"net.minecraft.client.entity.EntityOtherPlayerMP", "net.minecraft.entity.passive.EntityPig", "net.minecraft.entity.item.EntityTNTPrimed"};

    private static final Set<String> FAILED = new HashSet<>();
    /** The role being drawn; 0 when no subject is. */
    private static int role;
    private static List<Object[]> ops;
    private static List<float[]> lines;
    private static final List<float[]> VERTS = new ArrayList<>();
    private static int mode;
    private static float[] color = {1, 1, 1, 1};

    private final Object mc;
    private final Object manager;
    private final Map<Integer, Object> entities = new HashMap<>();
    private Object render;
    /** Stands for the player that looks: the label distance and the hitbox visibility read its position. */
    private final Object viewer;

    Anchor(Object mc) {
        this.mc = mc;
        this.manager = R.get(mc, "field_175616_W");
        this.viewer = R.alloc("net.minecraft.client.entity.EntityOtherPlayerMP");
        R.set(manager, "field_78734_h", viewer); // livingPlayer
        // Render.bindTexture asks the texture manager, which loads files; every texture is the same stand-in here.
        Object textures = R.alloc("net.minecraft.client.renderer.texture.TextureManager");
        final Object stub = R.alloc("net.minecraft.client.renderer.texture.SimpleTexture");
        R.set(stub, "field_110553_a", 0); // glTextureId
        R.set(textures, "field_110585_a", new HashMap<Object, Object>() { // mapTextureObjects
            @Override
            public Object get(Object key) {
                return stub;
            }
        });
        R.set(manager, "field_78724_e", textures); // renderEngine
        R.set(mc, "field_175618_aM", R.alloc("net.minecraft.client.renderer.BlockRendererDispatcher")); // blockRenderDispatcher
        // The host has no font texture, so the glyph widths read 0; the plates need the vanilla widths of ascii.png.
        int[] widths = (int[]) R.get(R.get(mc, "field_71466_p"), "field_78286_d"); // charWidth
        for (int c = 32; c < 127; c++) widths[c] = 6;
        for (char c : "i!.,:;'|".toCharArray()) widths[c] = 2;
        for (char c : "l`".toCharArray()) widths[c] = 3;
        for (char c : "It[]\"".toCharArray()) widths[c] = 4;
        for (char c : "fk<>(){}*".toCharArray()) widths[c] = 5;
        widths[' '] = 4;
        widths['@'] = widths['~'] = 7;
        R.set(manager, "field_78736_p", R.get(mc, "field_71466_p")); // textRenderer = fontRendererObj
    }

    /** Called by the patched Sink.text: whether a subject is being drawn and took the text. */
    static synchronized boolean text(String s, float x, float y, int argb, boolean shadow, float[] m) {
        if ((role & (LABEL | COUNTDOWN)) == 0) return false;
        // Sink matrix: m0 scale (negative: vanilla scales labels by -s, -s, s), m1 and m2 the offset in billboard axes.
        ops.add(new Object[] {'T', -(m[0] * x + m[1]), m[0] * y + m[2], Math.abs(m[0]), (argb & 0xFC000000) == 0 ? argb | 0xFF000000 : argb, shadow, s});
        return true;
    }

    /** Called by the patched Sink.rect: a label plate drawn with Gui.drawRect. */
    static synchronized boolean rect(float x0, float y0, float x1, float y1, int argb) {
        if ((role & (LABEL | COUNTDOWN)) == 0) return false;
        float[] t = Sink.transform(x0, y0, x1, y1);
        ops.add(new Object[] {'Q', new float[] {-t[0], t[1], -t[2], t[1], -t[2], t[3], -t[0], t[3], Float.intBitsToFloat(argb)}});
        return true;
    }

    public static synchronized void begin(int glMode) {
        mode = glMode;
        VERTS.clear();
    }

    public static synchronized void color(float r, float g, float b, float a) {
        color = new float[] {r, g, b, a};
    }

    public static synchronized void colorInt(int r, int g, int b, int a) {
        color(r / 255f, g / 255f, b / 255f, a / 255f);
    }

    public static synchronized void pos(double x, double y, double z) {
        if (role == 0) return;
        VERTS.add(new float[] {(float) x, (float) y, (float) z, 0, 0, 0, 0});
    }

    /** One vertex done: lines in the world axes for a hitbox, plates in the billboard axes for a label. */
    public static synchronized void endVertex() {
        if (role == 0 || VERTS.isEmpty()) return;
        float[] v = VERTS.get(VERTS.size() - 1);
        v[3] = argb(color);
        int n = VERTS.size();
        if (role == HITBOX) {
            if (mode == 1 && n % 2 == 0) lines.add(segment(VERTS.get(n - 2), v));
            if ((mode == 3 || mode == 2) && n > 1) lines.add(segment(VERTS.get(n - 2), v));
        } else if (mode == 7 && n % 4 == 0) {
            float[] f = new float[9];
            for (int i = 0; i < 4; i++) {
                float[] p = VERTS.get(n - 4 + i);
                float[] t = Sink.transform(p[0], p[1], p[0], p[1]);
                f[i * 2] = -t[0];
                f[i * 2 + 1] = t[1];
            }
            f[8] = v[3];
            ops.add(new Object[] {'Q', f});
        }
    }

    private static float[] segment(float[] a, float[] b) {
        return new float[] {a[0], a[1], a[2], b[0], b[1], b[2], b[3]};
    }

    private static float argb(float[] c) {
        int v = (Math.round(c[3] * 255) << 24) | (Math.round(c[0] * 255) << 16) | (Math.round(c[1] * 255) << 8) | Math.round(c[2] * 255);
        return Float.intBitsToFloat(v);
    }

    /** BlockRendererDispatcher.renderBlockBrightness draws a block model from files the host does not have: it draws nothing. */
    static byte[] patchDispatcher(byte[] bytes) {
        ClassNode cn = new ClassNode();
        new ClassReader(bytes).accept(cn, 0);
        for (Object o : cn.methods) {
            MethodNode m = (MethodNode) o;
            if (!(m.name + m.desc).equals("func_175016_a(Lnet/minecraft/block/state/IBlockState;F)V")) continue;
            m.instructions = new InsnList();
            m.instructions.add(new InsnNode(Opcodes.RETURN));
            m.tryCatchBlocks.clear();
            m.localVariables = null;
        }
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cn.accept(cw);
        return cw.toByteArray();
    }

    /** WorldRenderer: every call also lands here; the original body still runs. */
    static byte[] patch(byte[] bytes) {
        ClassNode cn = new ClassNode();
        new ClassReader(bytes).accept(cn, 0);
        for (Object o : cn.methods) {
            MethodNode m = (MethodNode) o;
            InsnList head = new InsnList();
            switch (m.name + m.desc) {
                case "func_181668_a(ILnet/minecraft/client/renderer/vertex/VertexFormat;)V":
                    head.add(new VarInsnNode(Opcodes.ILOAD, 1));
                    head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Anchor", "begin", "(I)V", false));
                    break;
                case "func_181662_b(DDD)Lnet/minecraft/client/renderer/WorldRenderer;":
                    head.add(new VarInsnNode(Opcodes.DLOAD, 1));
                    head.add(new VarInsnNode(Opcodes.DLOAD, 3));
                    head.add(new VarInsnNode(Opcodes.DLOAD, 5));
                    head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Anchor", "pos", "(DDD)V", false));
                    break;
                case "func_181666_a(FFFF)Lnet/minecraft/client/renderer/WorldRenderer;":
                    for (int i = 1; i <= 4; i++) head.add(new VarInsnNode(Opcodes.FLOAD, i));
                    head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Anchor", "color", "(FFFF)V", false));
                    break;
                case "func_181669_b(IIII)Lnet/minecraft/client/renderer/WorldRenderer;":
                    for (int i = 1; i <= 4; i++) head.add(new VarInsnNode(Opcodes.ILOAD, i));
                    head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Anchor", "colorInt", "(IIII)V", false));
                    break;
                case "func_181675_d()V":
                    head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Anchor", "endVertex", "()V", false));
                    break;
                default:
                    continue;
            }
            m.instructions.insert(head);
        }
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cn.accept(cw);
        return cw.toByteArray();
    }

    /** Reads one request, draws what the mods changed and writes the reply. */
    void answer(DataInputStream in, DataOutputStream out) throws IOException {
        float partial = in.readFloat();
        float viewYaw = in.readFloat();
        float viewPitch = in.readFloat();
        double[] camera = {in.readDouble(), in.readDouble(), in.readDouble()};
        int n = in.readUnsignedShort();
        ByteArrayOutputStream items = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(items);
        int count = 0;
        Set<Integer> seen = new HashSet<>();
        try {
            R.set(viewer, "field_70165_t", camera[0]); // posX
            R.set(viewer, "field_70163_u", camera[1]);
            R.set(viewer, "field_70161_v", camera[2]);
            R.set(manager, "field_78735_i", viewYaw); // playerViewY
            R.set(manager, "field_78732_j", viewPitch); // playerViewX
            R.set(manager, "field_78730_l", camera[0]); // viewerPosX
            R.set(manager, "field_78731_m", camera[1]);
            R.set(manager, "field_78728_n", camera[2]);
        } catch (RuntimeException e) {
            failed("view", e);
        }
        for (int i = 0; i < n; i++) {
            int id = in.readInt();
            int cls = in.readUnsignedByte();
            int roles = in.readUnsignedByte();
            double[] pos = {in.readDouble(), in.readDouble(), in.readDouble()};
            float width = in.readFloat();
            float height = in.readFloat();
            float yaw = in.readFloat();
            float dy = in.readFloat();
            int fuse = in.readInt();
            String label = null;
            if ((roles & LABEL) != 0) {
                byte[] b = new byte[in.readInt()];
                in.readFully(b);
                label = new String(b, StandardCharsets.UTF_8);
            }
            seen.add(id);
            Object e;
            try {
                e = entity(id, cls, pos, width, height, yaw, fuse);
            } catch (RuntimeException x) {
                failed("entity", x);
                continue;
            }
            if ((roles & LABEL) != 0) count += draw(w, id, LABEL, e, label, partial, yaw);
            if ((roles & COUNTDOWN) != 0) count += draw(w, id, COUNTDOWN, e, null, partial, yaw);
            if ((roles & HITBOX) != 0) count += draw(w, id, HITBOX, e, null, partial, yaw);
        }
        entities.keySet().retainAll(seen);
        out.writeInt(count);
        w.flush();
        items.writeTo(out);
        out.flush();
    }

    private int draw(DataOutputStream w, int id, int what, Object e, String label, float partial, float yaw) throws IOException {
        synchronized (Anchor.class) {
            role = what;
            ops = new ArrayList<>();
            lines = new ArrayList<>();
            mode = 0;
            VERTS.clear();
            color = new float[] {1, 1, 1, 1};
        }
        Sink.push();
        Sink.identity();
        try {
            if (render == null) render = R.neu(RENDER_TNT, manager);
            switch (what) {
                case LABEL:
                    R.call(render, "func_147906_a", e, label, 0.0, 0.0, 0.0, 64); // Render.renderLivingLabel
                    break;
                case COUNTDOWN:
                    R.call(render, "func_76986_a", e, 0.0, 0.0, 0.0, yaw, partial); // RenderTNTPrimed.doRender
                    break;
                default:
                    R.call(manager, "func_85094_b", e, 0.0, 0.0, 0.0, yaw, partial); // RenderManager.renderDebugBoundingBox
                    break;
            }
        } catch (RuntimeException x) {
            failed("role " + what, x);
        } finally {
            Sink.pop();
        }
        List<Object[]> o;
        List<float[]> l;
        synchronized (Anchor.class) {
            role = 0;
            o = ops;
            l = lines;
        }
        int count = 0;
        if (!o.isEmpty()) {
            w.writeByte('L');
            w.writeInt(id);
            w.writeShort(Math.min(o.size(), 256));
            for (int i = 0; i < Math.min(o.size(), 256); i++) {
                Object[] op = o.get(i);
                if (((Character) op[0]).charValue() == 'Q') {
                    float[] f = (float[]) op[1];
                    w.writeByte('Q');
                    for (int k = 0; k < 8; k++) w.writeFloat(f[k]);
                    w.writeInt(Float.floatToRawIntBits(f[8]));
                } else {
                    byte[] utf = ((String) op[6]).getBytes(StandardCharsets.UTF_8);
                    w.writeByte('T');
                    w.writeFloat((Float) op[1]);
                    w.writeFloat((Float) op[2]);
                    w.writeFloat((Float) op[3]);
                    w.writeInt((Integer) op[4]);
                    w.writeByte((Boolean) op[5] ? 1 : 0);
                    w.writeInt(utf.length);
                    w.write(utf);
                }
            }
            count++;
        }
        for (float[] s : l) {
            w.writeByte('W');
            w.writeInt(id);
            for (int k = 0; k < 6; k++) w.writeFloat(s[k]);
            w.writeInt(Float.floatToRawIntBits(s[6]));
            count++;
        }
        return count;
    }

    private Object entity(int id, int cls, double[] pos, float width, float height, float yaw, int fuse) {
        Object e = entities.get(id);
        if (e == null) {
            e = R.alloc(ENTITY[cls]);
            R.set(e, "field_145783_c", id); // entityId
            R.set(e, "field_70170_p", R.get(mc, "field_71441_e")); // worldObj
            Object watcher = R.neu("net.minecraft.entity.DataWatcher", e);
            R.call(watcher, "func_75682_a", 0, (byte) 0); // addObject
            R.call(watcher, "func_75682_a", 1, (short) 300);
            R.call(watcher, "func_75682_a", 3, (byte) 0);
            R.call(watcher, "func_75682_a", 2, "");
            R.call(watcher, "func_75682_a", 4, (byte) 0);
            R.set(e, "field_70180_af", watcher); // dataWatcher
            entities.put(id, e);
        }
        R.set(e, "field_70165_t", pos[0]); // posX
        R.set(e, "field_70163_u", pos[1]);
        R.set(e, "field_70161_v", pos[2]);
        R.set(e, "field_70130_N", width); // width
        R.set(e, "field_70131_O", height); // height
        R.set(e, "field_70177_z", yaw); // rotationYaw
        if (cls == 2) R.set(e, "field_70516_a", fuse); // fuse
        R.call(e, "func_174826_a", R.neu("net.minecraft.util.AxisAlignedBB", pos[0] - width / 2, pos[1], pos[2] - width / 2, pos[0] + width / 2, pos[1] + height, pos[2] + width / 2)); // setEntityBoundingBox
        return e;
    }

    private static void failed(String what, Throwable t) {
        if (FAILED.add(what)) {
            Throwable c = t.getCause() != null ? t.getCause() : t;
            StringBuilder at = new StringBuilder();
            for (int i = 0; i < Math.min(6, c.getStackTrace().length); i++) at.append("\n    at ").append(c.getStackTrace()[i]);
            Host.log("anchor " + what + " failed: " + t + " <- " + c + at);
        }
    }
}
