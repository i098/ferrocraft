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

    /** Reads one request, asks the getters and writes the reply. */
    void answer(DataInputStream in, DataOutputStream out) throws IOException {
        float partial = in.readFloat();
        float yaw = in.readFloat();
        float pitch = in.readFloat();
        float lookYaw = in.readFloat();
        float lookPitch = in.readFloat();
        int perspective = in.readUnsignedByte();
        long time = in.readLong();
        float rain = in.readFloat();
        float thunder = in.readFloat();
        float fov = in.readFloat();
        float handPrev = in.readFloat();
        float hand = in.readFloat();
        int nightVision = in.readInt();

        int valid = 0;
        long timeOut = 0;
        float[] weather = new float[2];
        float[] camera = new float[3];
        float fovOut = 0;
        float nvOut = 0;
        float[] look = new float[2];

        // The look first: the camera and the angles below start from where it left the player.
        R.set(entity, "field_70177_z", yaw);
        R.set(entity, "field_70125_A", pitch);
        R.set(entity, "field_70126_B", yaw);
        R.set(entity, "field_70127_C", pitch);
        try {
            R.call(entity, "func_70082_c", (float) (lookYaw / MOUSE_SCALE), (float) (-lookPitch / MOUSE_SCALE)); // setAngles
            float newYaw = (Float) R.get(entity, "field_70177_z");
            float newPitch = (Float) R.get(entity, "field_70125_A");
            look[0] = newYaw - yaw;
            look[1] = newPitch - pitch;
            yaw = newYaw;
            pitch = newPitch;
            valid |= 32;
        } catch (RuntimeException e) {
            failed("look", e);
        }
        R.set(entity, "field_70177_z", yaw);
        R.set(entity, "field_70125_A", pitch);
        R.set(entity, "field_70126_B", yaw);
        R.set(entity, "field_70127_C", pitch);

        try {
            Object info = R.get(world, "field_72986_A");
            R.set(info, "field_76094_f", time); // worldTime
            R.set(info, "field_82575_g", time); // totalTime
            timeOut = (Long) R.call(world, "func_72820_D"); // World.getWorldTime
            valid |= 1;
        } catch (RuntimeException e) {
            failed("time", e);
        }

        try {
            for (String f : new String[] {"field_73004_o", "field_73017_q"}) R.set(world, f, rain); // prev and now rainingStrength
            for (String f : new String[] {"field_73018_p", "field_73016_r"}) R.set(world, f, thunder); // prev and now thunderingStrength
            weather[0] = (Float) R.call(world, "func_72867_j", partial); // World.getRainStrength
            weather[1] = (Float) R.call(world, "func_72819_i", partial); // World.getThunderStrength
            valid |= 2;
        } catch (RuntimeException e) {
            failed("weather", e);
        }

        double posY = (Double) R.get(entity, "field_70163_u");
        double prevY = (Double) R.get(entity, "field_70167_r");
        R.set(entity, "field_70163_u", VOID_Y);
        R.set(entity, "field_70167_r", VOID_Y);
        try {
            R.set(settings, "field_74320_O", perspective); // thirdPersonView
            if (orient(partial, perspective, camera)) valid |= 4;
        } catch (RuntimeException e) {
            failed("camera", e);
        }
        try {
            R.set(settings, "field_74334_X", fov); // fovSetting
            R.set(renderer, "field_78506_S", handPrev); // fovModifierHandPrev
            R.set(renderer, "field_78507_R", hand); // fovModifierHand
            fovOut = (Float) R.call(renderer, "func_78481_a", partial, true); // EntityRenderer.getFOVModifier
            valid |= 8;
        } catch (RuntimeException e) {
            failed("fov", e);
        }
        R.set(entity, "field_70163_u", posY);
        R.set(entity, "field_70167_r", prevY);

        try {
            nvOut = nightVision(partial, nightVision);
            valid |= 16;
        } catch (RuntimeException e) {
            // Without the effect vanilla throws here and no mod answers; that is the vanilla value, not a failure.
            if (nightVision >= 0) failed("night vision", e);
        }

        out.writeByte(valid);
        out.writeLong(timeOut);
        for (float v : new float[] {weather[0], weather[1], camera[0], camera[1], camera[2], fovOut, nvOut, look[0], look[1]}) out.writeFloat(v);
        out.flush();
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
            if (c[0] == 'R' && c[2] == 1 && c[3] == 0 && c[4] == 0) pitch = c[1];
            if (c[0] == 'R' && c[2] == 0 && c[3] == 1 && c[4] == 0) yaw = c[1] - 180f;
            if (c[0] == 'T' && c[1] == 0 && c[2] == 0 && c[3] < 0 && distance == null) distance = -c[3];
        }
        if (yaw == null || pitch == null) return false;
        camera[0] = yaw;
        camera[1] = pitch;
        camera[2] = perspective == 0 || distance == null ? 0 : distance;
        return true;
    }

    /** EntityRenderer.getNightVisionBrightness on the stand-in player, which holds the effect only for the call. */
    private float nightVision(float partial, int ticks) {
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
