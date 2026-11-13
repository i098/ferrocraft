package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * World values that mods change with Mixin and that Rust draws with: the world time, the rain and thunder
 * strength, the camera angles and distance, the FOV and the night vision brightness, plus what a look does to the
 * player's yaw and pitch. Rust sends what it knows once per frame; this puts it on the stand-in world, player and
 * renderer, calls the real vanilla getters, where the mods' hooks run, and sends back what they return.
 *
 * <p>A group that throws (a stand-in object lacks something the getter reads) is sent as not valid and logged once:
 * Rust then keeps the vanilla value. The camera runs the real {@code EntityRenderer.orientCamera}, whose GL calls
 * {@link #rotate} and {@link #translate} record; the camera sits far below the world so the ray trace of the third
 * person view and the block at the eyes read no chunk.
 *
 * <p>Request, after the tag 'O': f32 partial, f32 yaw, f32 pitch, f32 look yaw, f32 look pitch, u8 perspective,
 * i64 world time, f32 rain, f32 thunder, f32 FOV, f32 hand modifier previous, f32 hand modifier, i32 night vision
 * ticks (-1 none). Reply: u8 valid bits (1 time, 2 weather, 4 camera, 8 FOV, 16 night vision, 32 look), i64 time,
 * f32 rain, f32 thunder, f32 camera yaw, f32 camera pitch, f32 camera distance, f32 FOV, f32 night vision, f32
 * yaw delta, f32 pitch delta.
 */
public final class Overrides {
    /** Entity.setAngles scales a mouse delta by this. */
    private static final double MOUSE_SCALE = 0.15;
    /** Far under the world: no block lies at the eyes or on the ray. */
    private static final double VOID_Y = -1000;
    private static final int NIGHT_VISION_ID = 16;

    private static final Set<String> FAILED = ConcurrentHashMap.newKeySet();

    /** The GL rotate and translate calls of the camera run; null when none is recording. */
    private static List<float[]> recorded;

    private final Object world;
    private final Object renderer;
    private final Object settings;
    private final Object entity;

    Overrides(Object mc) {
        this.world = R.get(mc, "field_71441_e");
        this.renderer = R.get(mc, "field_71460_t");
        this.settings = R.get(mc, "field_71474_y");
        this.entity = R.get(mc, "field_175622_Z");
        R.set(world, "field_72986_A", R.alloc("net.minecraft.world.storage.WorldInfo")); // worldInfo
        // Forge's World.getWorldTime asks the provider, which reads the world info back from its world
        Object provider = R.alloc("net.minecraft.world.WorldProviderSurface");
        R.set(provider, "field_76579_a", world); // worldObj
        R.set(world, "field_73011_w", provider); // provider
        // orientCamera asks the render global for cloud fog
        Object globe = R.alloc("net.minecraft.client.renderer.RenderGlobal");
        R.set(globe, "field_72777_q", mc); // mc
        R.set(mc, "field_71438_f", globe); // renderGlobal
        R.set(renderer, "field_78490_B", 4.0f); // thirdPersonDistance
        R.set(renderer, "field_78491_C", 4.0f); // thirdPersonDistanceTemp
    }

    /** Called by the patched GL11 shim: glRotatef. */
    public static synchronized void rotate(float angle, float x, float y, float z) {
        if (recorded != null) recorded.add(new float[] {'R', angle, x, y, z});
    }

    /** Called by the patched GL11 shim: glTranslatef. */
    public static synchronized void translate(float x, float y, float z) {
        if (recorded != null) recorded.add(new float[] {'T', x, y, z, 0});
    }

    /** The values a request got, and which groups are valid. */
    private static final class Reply {
        int valid;
        long time;
        final float[] weather = new float[2];
        final float[] camera = new float[3];
        float fov;
        float nightVision;
        final float[] look = new float[2];
    }

    /** Reads one request, asks the getters and writes the reply. */
    void answer(DataInputStream in, DataOutputStream out) throws IOException {
        float partial = in.readFloat();
        float[] angles = {in.readFloat(), in.readFloat()};
        float[] mouse = {in.readFloat(), in.readFloat()};
        int perspective = in.readUnsignedByte();
        long time = in.readLong();
        float rain = in.readFloat();
        float thunder = in.readFloat();
        float fov = in.readFloat();
        float handPrev = in.readFloat();
        float hand = in.readFloat();
        int nightVision = in.readInt();

        Reply r = new Reply();
        // The look first: the camera and the angles below start from where it left the player.
        placeEntity(angles);
        look(angles, mouse, r);
        placeEntity(angles);
        time(time, r);
        weather(rain, thunder, partial, r);
        double posY = (Double) R.get(entity, "field_70163_u");
        double prevY = (Double) R.get(entity, "field_70167_r");
        R.set(entity, "field_70163_u", VOID_Y);
        R.set(entity, "field_70167_r", VOID_Y);
        camera(partial, perspective, r);
        fov(partial, fov, handPrev, hand, r);
        R.set(entity, "field_70163_u", posY);
        R.set(entity, "field_70167_r", prevY);
        nightVision(partial, nightVision, r);

        out.writeByte(r.valid);
        out.writeLong(r.time);
        for (float v : new float[] {r.weather[0], r.weather[1], r.camera[0], r.camera[1], r.camera[2], r.fov, r.nightVision, r.look[0], r.look[1]}) out.writeFloat(v);
        out.flush();
    }

    /** Puts the stand-in view entity at the angles, with no turn between the previous and the current. */
    private void placeEntity(float[] angles) {
        R.set(entity, "field_70177_z", angles[0]);
        R.set(entity, "field_70125_A", angles[1]);
        R.set(entity, "field_70126_B", angles[0]);
        R.set(entity, "field_70127_C", angles[1]);
    }

    /** Entity.setAngles; angles holds the result afterwards and the reply what the look changed. */
    private void look(float[] angles, float[] mouse, Reply r) {
        try {
            R.call(entity, "func_70082_c", (float) (mouse[0] / MOUSE_SCALE), (float) (-mouse[1] / MOUSE_SCALE));
            float newYaw = (Float) R.get(entity, "field_70177_z");
            float newPitch = (Float) R.get(entity, "field_70125_A");
            r.look[0] = newYaw - angles[0];
            r.look[1] = newPitch - angles[1];
            angles[0] = newYaw;
            angles[1] = newPitch;
            r.valid |= 32;
        } catch (RuntimeException e) {
            failed("look", e);
        }
    }

    private void time(long time, Reply r) {
        try {
            Object info = R.get(world, "field_72986_A");
            R.set(info, "field_76094_f", time); // worldTime
            R.set(info, "field_82575_g", time); // totalTime
            r.time = (Long) R.call(world, "func_72820_D"); // World.getWorldTime
            r.valid |= 1;
        } catch (RuntimeException e) {
            failed("time", e);
        }
    }

    private void weather(float rain, float thunder, float partial, Reply r) {
        try {
            for (String f : new String[] {"field_73003_n", "field_73004_o"}) R.set(world, f, rain); // prev and now rainingStrength
            for (String f : new String[] {"field_73018_p", "field_73017_q"}) R.set(world, f, thunder); // prev and now thunderingStrength
            r.weather[0] = (Float) R.call(world, "func_72867_j", partial); // World.getRainStrength
            r.weather[1] = (Float) R.call(world, "func_72819_i", partial); // World.getThunderStrength
            r.valid |= 2;
        } catch (RuntimeException e) {
            failed("weather", e);
        }
    }

    private void camera(float partial, int perspective, Reply r) {
        try {
            R.set(settings, "field_74320_O", perspective); // thirdPersonView
            if (orient(partial, perspective, r.camera)) r.valid |= 4;
        } catch (RuntimeException e) {
            failed("camera", e);
        }
    }

    private void fov(float partial, float fov, float handPrev, float hand, Reply r) {
        try {
            R.set(settings, "field_74334_X", fov); // fovSetting
            R.set(renderer, "field_78506_S", handPrev); // fovModifierHandPrev
            R.set(renderer, "field_78507_R", hand); // fovModifierHand
            r.fov = (Float) R.call(renderer, "func_78481_a", partial, true); // EntityRenderer.getFOVModifier
            r.valid |= 8;
        } catch (RuntimeException e) {
            failed("fov", e);
        }
    }

    private void nightVision(float partial, int ticks, Reply r) {
        try {
            r.nightVision = nightVisionBrightness(partial, ticks);
            r.valid |= 16;
        } catch (RuntimeException e) {
            // Without the effect vanilla throws here and no mod answers; that is the vanilla value, not a failure.
            if (ticks >= 0) failed("night vision", e);
        }
    }

    /**
     * Runs EntityRenderer.orientCamera and reads the angles off the last two rotations, which are the pitch and the
     * yaw plus 180 degrees, and the distance off the translation along z of the third person views.
     */
    private boolean orient(float partial, int perspective, float[] camera) {
        List<float[]> calls = new ArrayList<>();
        synchronized (Overrides.class) {
            recorded = calls;
        }
        Sink.push();
        try {
            R.call(renderer, "func_78467_g", partial); // orientCamera
        } finally {
            Sink.pop();
            synchronized (Overrides.class) {
                recorded = null;
            }
        }
        Float yaw = null;
        Float pitch = null;
        Float distance = null;
        for (float[] c : calls) {
            if (isRotation(c, 1, 0)) pitch = c[1];
            if (isRotation(c, 0, 1)) yaw = c[1] - 180f;
            if (distance == null && c[0] == 'T' && c[3] < 0 && c[1] == 0 && c[2] == 0) distance = -c[3];
        }
        if (yaw == null || pitch == null) return false;
        camera[0] = yaw;
        camera[1] = pitch;
        camera[2] = perspective == 0 || distance == null ? 0 : distance;
        return true;
    }

    /** A recorded rotation about the x axis (1, 0) or the y axis (0, 1). */
    private static boolean isRotation(float[] c, float x, float y) {
        return c[0] == 'R' && c[2] == x && c[3] == y && c[4] == 0;
    }

    /** EntityRenderer.getNightVisionBrightness on the stand-in player, which holds the effect only for the call. */
    private float nightVisionBrightness(float partial, int ticks) {
        @SuppressWarnings("unchecked")
        HashMap<Object, Object> effects = (HashMap<Object, Object>) R.get(entity, "field_70713_bf"); // activePotionsMap
        Object had = effects.get(NIGHT_VISION_ID);
        if (ticks >= 0) effects.put(NIGHT_VISION_ID, R.neu("net.minecraft.potion.PotionEffect", NIGHT_VISION_ID, ticks));
        try {
            return (Float) R.call(renderer, "func_180438_a", entity, partial);
        } finally {
            if (had != null) effects.put(NIGHT_VISION_ID, had);
            else effects.remove(NIGHT_VISION_ID);
        }
    }

    private static void failed(String group, RuntimeException e) {
        if (FAILED.add(group)) {
            Host.log("overrides: " + group + " stays vanilla: " + e);
            e.printStackTrace();
        }
    }
}
