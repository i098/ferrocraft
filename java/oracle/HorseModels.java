import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import net.minecraft.flat.Bootstrap;
import net.minecraft.flat.HorseBaseEntity;
import net.minecraft.flat.HorseModel;
import net.minecraft.flat.ModelPart;
import net.minecraft.flat.SyncedData;

/**
 * Prints the part tree of the real 1.8.9 HorseModel (texture offset, box corners, grow) and, for six poses of a
 * horse, the pivot and rotation every part has after prepare (setLivingAnimations). The horse constructor needs a
 * world, so entities are allocated without it and given the synced data a spawned horse registers; the animation
 * progress floats are set by reflection, last = current, so the partial tick is moot.
 * Java 8 source level.
 *
 * Run (after `cargo test -p oracle` built target/oracle/mapped.jar):
 *   javac --release 8 -cp target/oracle/mapped.jar -d /tmp/horse java/oracle/HorseModels.java
 *   java -cp /tmp/horse:target/oracle/mapped.jar:$(printf %s: target/oracle/libs/*.jar) HorseModels
 */
public class HorseModels {
    private static Field find(Class<?> c, String name) throws Exception {
        for (; c != null; c = c.getSuperclass()) {
            try {
                Field f = c.getDeclaredField(name);
                f.setAccessible(true);
                return f;
            } catch (NoSuchFieldException e) {
                // keep looking in the superclass
            }
        }
        throw new NoSuchFieldException(name);
    }

    private static void set(Object o, String name, Object v) throws Exception {
        find(o.getClass(), name).set(o, v);
    }

    private static void call(Object o, String name, Class<?> type, Object v) throws Exception {
        Method m = o.getClass().getMethod(name, type);
        m.invoke(o, v);
    }

    private static void dump(HorseModel m, String label) throws Exception {
        System.out.println("== " + label);
        for (Field f : HorseModel.class.getDeclaredFields()) {
            if (f.getType() != ModelPart.class) {
                continue;
            }
            f.setAccessible(true);
            ModelPart p = (ModelPart) f.get(m);
            StringBuilder sb = new StringBuilder();
            sb.append(f.getName()).append(" pos=(").append(p.x).append(',').append(p.y).append(',').append(p.z)
                .append(") rot=(").append(p.rotationX).append(',').append(p.rotationY).append(',')
                .append(p.rotationZ).append(')');
            System.out.println(sb);
        }
    }

    private static void layout(HorseModel m) throws Exception {
        System.out.println("== HorseModel texture " + m.textureWidth + "x" + m.textureHeight);
        for (Field f : HorseModel.class.getDeclaredFields()) {
            if (f.getType() != ModelPart.class) {
                continue;
            }
            f.setAccessible(true);
            ModelPart p = (ModelPart) f.get(m);
            Field u = ModelPart.class.getDeclaredField("u");
            Field v = ModelPart.class.getDeclaredField("v");
            u.setAccessible(true);
            v.setAccessible(true);
            StringBuilder sb = new StringBuilder();
            sb.append(f.getName()).append(" uv=(").append(u.getInt(p)).append(',').append(v.getInt(p)).append(')');
            for (Object b : p.boxes) {
                Class<?> c = b.getClass();
                sb.append(" box[");
                for (String n : new String[] {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"}) {
                    sb.append(c.getField(n).getFloat(b)).append(' ');
                }
                sb.append(']');
            }
            System.out.println(sb);
        }
    }

    private static void progress(HorseBaseEntity h, float grass, float rear, float mouth) throws Exception {
        set(h, "eatingGrassAnimationProgress", grass);
        set(h, "lastEatingGrassAnimationProgress", grass);
        set(h, "angryAnimationProgress", rear);
        set(h, "lastAngryAnimationProgress", rear);
        set(h, "eatingAnimationProgress", mouth);
        set(h, "lastEatingAnimationProgress", mouth);
    }

    private static HorseBaseEntity horse(int type) throws Exception {
        Class<?> unsafeClass = Class.forName("sun.misc.Unsafe");
        Field unsafe = unsafeClass.getDeclaredField("theUnsafe");
        unsafe.setAccessible(true);
        HorseBaseEntity h = (HorseBaseEntity) unsafeClass.getMethod("allocateInstance", Class.class)
            .invoke(unsafe.get(null), HorseBaseEntity.class);
        SyncedData data = new SyncedData(h);
        data.register(16, 0);
        data.register(19, (byte) 0);
        data.register(20, 0);
        data.register(21, "");
        data.register(22, 0);
        set(h, "syncedData", data);
        call(h, "setType", int.class, type);
        set(h, "lastBodyYaw", 0.0f);
        set(h, "bodyYaw", 0.0f);
        set(h, "lastHeadYaw", 30.0f);
        set(h, "headYaw", 30.0f);
        set(h, "lastPitch", 15.0f);
        set(h, "pitch", 15.0f);
        set(h, "ticks", 7);
        return h;
    }

    private static void pose(String label, HorseBaseEntity h, float swing, float amount) throws Exception {
        HorseModel m = new HorseModel();
        m.prepare(h, swing, amount, 0.0f);
        dump(m, label + " (swing " + swing + ", amount " + amount + ", ticks 7, head yaw 30, pitch 15)");
    }

    public static void main(String[] args) throws Exception {
        Bootstrap.init();
        layout(new HorseModel());
        HorseBaseEntity h = horse(0);
        progress(h, 0f, 0f, 0f);
        pose("horse at rest", h, 0f, 0f);
        pose("horse walking", h, 1.3f, 0.6f);
        h.getSyncedData().update(16, 4);
        pose("saddled horse walking", h, 1.3f, 0.6f);
        set(h, "rider", horse(0));
        progress(h, 0f, 0.6f, 0.5f);
        pose("saddled ridden horse rearing 0.6, mouth 0.5", h, 0.4f, 0.3f);
        progress(h, 0.7f, 0f, 0.5f);
        pose("saddled ridden horse eating grass 0.7, mouth 0.5", h, 0f, 0f);
        HorseBaseEntity d = horse(1);
        d.getSyncedData().update(16, 8);
        progress(d, 0f, 0f, 0f);
        pose("donkey with chest walking", d, 1.3f, 0.6f);
    }
}
