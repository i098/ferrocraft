package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Render probes. Rust draws the entities and the items, so the patches that mods make to their render methods never run
 * there. Once per frame Rust asks for the values those patches change, and this runs the real vanilla method on a
 * stand-in object and sends the result:
 *
 * <ul>
 *   <li>the hurt tint: {@code RendererLivingEntity.setBrightness} on a stand-in entity that is hit; the tint is what it
 *       leaves in the brightness buffer (vanilla: red at 0.3), as DamageTint changes it;
 *   <li>the item glint color: {@code RenderItem.renderEffect}, whose {@code renderModel} calls {@link BridgeTransformer}
 *       turns into {@link #glint}; GlintColorizer changes the color argument of those calls;
 *   <li>{@code RenderLivingEvent.Pre} and {@code Post} for each entity Rust draws; a cancelled Pre means the entity is not
 *       drawn. The events are posted only while a handler listens.
 * </ul>
 *
 * <p>Request, after the tag 'Q': u8 want (1 hurt tint, 2 glint, 4 render events), u8 hurt time, u32 entity count, then
 * per entity i32 id and three f64 position. Reply: u8 valid (1 hurt tint, 2 glint, 4 a handler listens), four f32 hurt
 * tint (red, green, blue, alpha), i32 glint ARGB, then one byte per entity: 1 when its Pre event was cancelled.
 *
 * <p>A group that throws is sent as not valid and logged once: Rust then keeps the vanilla value.
 */
public final class Probe {
    private static final int MAX_TARGETS = 1 << 12;
    private static final int WANT_HURT = 1;
    private static final int WANT_GLINT = 2;
    private static final int WANT_EVENTS = 4;
    private static final int MAX_HURT_TIME = 10;

    private static final Set<String> FAILED = ConcurrentHashMap.newKeySet();

    /** The colors that {@code renderModel} got while {@code renderEffect} ran; null when no probe runs. */
    private static List<Integer> glints;

    /** The stand-in entities that {@link Events} keeps, by id. */
    private final Map<Integer, Object> entities;
    private final Object forgeBus = R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS");
    private final Object renderer = R.alloc("net.minecraft.client.renderer.entity.RenderPig");
    private final Object renderItem = R.alloc("net.minecraft.client.renderer.entity.RenderItem");
    /** A blaze: its {@code getBrightness} needs no world. */
    private final Object hit = R.alloc("net.minecraft.entity.monster.EntityBlaze");

    @SuppressWarnings("unchecked")
    Probe(Object mc, Events events) {
        this.entities = (Map<Integer, Object>) R.get(events, "entities");
        R.set(renderer, "field_177095_g", ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder()).asFloatBuffer()); // brightnessBuffer
        R.set(renderer, "field_76990_c", R.get(mc, "field_175616_W")); // renderManager
        R.set(renderItem, "field_175057_n", R.get(mc, "field_71446_o")); // textureManager
    }

    /** Called by the patched {@code RenderItem.renderModel(IBakedModel, int)}: the color of one glint pass. */
    public static synchronized void glint(int color) {
        if (glints != null) glints.add(color);
    }

    private static synchronized void record(List<Integer> into) {
        glints = into;
    }

    void answer(DataInputStream in, DataOutputStream out) throws IOException {
        int want = in.readUnsignedByte();
        int hurtTime = in.readUnsignedByte();
        int n = in.readInt();
        if (n < 0 || n > MAX_TARGETS) throw new IOException(n + " render targets");
        int[] ids = new int[n];
        double[][] pos = new double[n][3];
        for (int i = 0; i < n; i++) {
            ids[i] = in.readInt();
            for (int k = 0; k < 3; k++) pos[i][k] = in.readDouble();
        }

        int valid = 0;
        float[] tint = new float[4];
        int glint = 0;
        byte[] cancelled = new byte[n];
        if ((want & WANT_HURT) != 0) {
            try {
                hurtTint(hurtTime, tint);
                valid |= 1;
            } catch (RuntimeException e) {
                failed("hurt tint", e);
            }
        }
        if ((want & WANT_GLINT) != 0) {
            try {
                glint = glintColor();
                valid |= 2;
            } catch (RuntimeException e) {
                failed("glint", e);
            }
        }
        if ((want & WANT_EVENTS) != 0) {
            try {
                if (listening()) valid |= 4;
                if ((valid & 4) != 0) renderEvents(ids, pos, cancelled);
            } catch (RuntimeException e) {
                failed("render events", e);
            }
        }

        out.writeByte(valid);
        for (float v : tint) out.writeFloat(v);
        out.writeInt(glint);
        out.write(cancelled);
        out.flush();
    }

    /** RendererLivingEntity.setBrightness for an entity that was hit: the color it puts in the brightness buffer. */
    private void hurtTint(int hurtTime, float[] tint) {
        R.set(hit, "field_70737_aN", Math.max(1, Math.min(hurtTime, MAX_HURT_TIME))); // hurtTime
        R.set(hit, "field_70738_aO", MAX_HURT_TIME); // maxHurtTime
        R.set(hit, "field_70725_aQ", 0); // deathTime
        if (!Boolean.TRUE.equals(R.call(renderer, "func_177092_a", hit, 0f, true))) throw new IllegalStateException("setBrightness set no tint");
        FloatBuffer b = (FloatBuffer) R.get(renderer, "field_177095_g");
        for (int i = 0; i < 4; i++) tint[i] = b.get(i);
    }

    /** RenderItem.renderEffect draws two passes; the first one's color stands for both. */
    private int glintColor() {
        List<Integer> colors = new ArrayList<>();
        record(colors);
        try {
            // Mods learn what is drawn from Forge's camera transform hook (GlintColorizer keys its color on it): the held item is the one that is asked.
            try {
                R.call(R.cls("net.minecraftforge.client.ForgeHooksClient"), "handleCameraTransforms", (Object) null, R.get(R.cls("net.minecraft.client.renderer.block.model.ItemCameraTransforms$TransformType"), "FIRST_PERSON"));
            } catch (RuntimeException ignored) {
                // the stand-in has no model: the hook of the mod ran before the vanilla body failed, or the mod does not use it
            }
            R.call(renderItem, "func_180451_a", (Object) null); // renderEffect(IBakedModel): renderModel is patched, so the model is not read
        } finally {
            record(null);
        }
        if (colors.isEmpty()) throw new IllegalStateException("renderEffect did not call renderModel");
        return colors.get(0);
    }

    private boolean listening() {
        int bus = (Integer) R.get(forgeBus, "busID");
        Object list = R.get(R.cls("net.minecraftforge.client.event.RenderLivingEvent$Pre"), "LISTENER_LIST");
        return ((Object[]) R.call(list, "getListeners", bus)).length > 0;
    }

    /** What RendererLivingEntity.doRender posts: Pre, then Post unless a handler cancelled Pre. */
    private void renderEvents(int[] ids, double[][] pos, byte[] cancelled) {
        Class<?> living = R.cls("net.minecraft.entity.EntityLivingBase");
        for (int i = 0; i < ids.length; i++) {
            Object entity = entities.get(ids[i]);
            if (!living.isInstance(entity)) continue;
            Object pre = R.neu("net.minecraftforge.client.event.RenderLivingEvent$Pre", entity, renderer, pos[i][0], pos[i][1], pos[i][2]);
            if (post(pre)) {
                cancelled[i] = 1;
                continue;
            }
            post(R.neu("net.minecraftforge.client.event.RenderLivingEvent$Post", entity, renderer, pos[i][0], pos[i][1], pos[i][2]));
        }
    }

    /** A failing handler is logged and counts as not cancelling. */
    private boolean post(Object event) {
        try {
            return Boolean.TRUE.equals(R.call(forgeBus, "post", event));
        } catch (RuntimeException e) {
            e.printStackTrace();
            return false;
        }
    }

    private static void failed(String group, RuntimeException e) {
        if (FAILED.add(group)) {
            Host.log("probe: " + group + " stays vanilla: " + e);
            e.printStackTrace();
        }
    }
}
