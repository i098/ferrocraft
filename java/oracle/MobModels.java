import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import net.minecraft.flat.BatModel;
import net.minecraft.flat.BlazeModel;
import net.minecraft.flat.Bootstrap;
import net.minecraft.flat.DyeColor;
import net.minecraft.flat.EnderCrystalModel;
import net.minecraft.flat.GhastModel;
import net.minecraft.flat.LivingEntity;
import net.minecraft.flat.MagmaCubeModel;
import net.minecraft.flat.Model;
import net.minecraft.flat.ModelPart;
import net.minecraft.flat.OcelotEntity;
import net.minecraft.flat.OcelotModel;
import net.minecraft.flat.EndermiteModel;
import net.minecraft.flat.RabbitEntity;
import net.minecraft.flat.RabbitModel;
import net.minecraft.flat.SheepEntity;
import net.minecraft.flat.SlimeModel;
import net.minecraft.flat.SnowGolemModel;
import net.minecraft.flat.SquidModel;
import net.minecraft.flat.WolfEntity;
import net.minecraft.flat.WitchModel;
import net.minecraft.flat.WolfModel;

/**
 * Prints the part tree of the slime, magma cube, blaze, ghast, bat, ender crystal, wolf, ocelot, rabbit, endermite, squid, snow golem and witch models of the
 * real 1.8.9 client: texture offset, pivot, rotation, mirror flag and box corners of every part. The
 * blaze, ghast and magma cube models are also run through setupAnimation (the bat one reads entity
 * metadata, which needs a world, so only its constructor runs). The wolf and ocelot models run in each
 * pose through prepare and setupAnimation, on entities built with no world. Java 8 source level.
 *
 * Run: harness/mobmodels.sh
 */
public class MobModels {
    private static int intField(Object o, String name) throws Exception {
        Field f = ModelPart.class.getDeclaredField(name);
        f.setAccessible(true);
        return f.getInt(o);
    }

    private static void dump(String indent, ModelPart p) throws Exception {
        StringBuilder sb = new StringBuilder();
        sb.append(indent).append("part uv=(").append(intField(p, "u")).append(',').append(intField(p, "v"))
            .append(") pivot=(").append(p.x).append(',').append(p.y).append(',').append(p.z)
            .append(") rot=(").append(p.rotationX).append(',').append(p.rotationY).append(',')
            .append(p.rotationZ).append(") flipped=").append(p.flipped);
        for (Object b : p.boxes) {
            Class<?> c = b.getClass();
            sb.append(" box[");
            for (String n : new String[] {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"}) {
                sb.append(c.getField(n).getFloat(b)).append(' ');
            }
            sb.append(']');
        }
        System.out.println(sb);
        if (p.children != null) {
            for (ModelPart child : p.children) {
                dump(indent + "  ", child);
            }
        }
    }

    private static void model(String name, Model m) throws Exception {
        System.out.println("== " + name + " textureSize=" + m.textureWidth + "x" + m.textureHeight
            + " parts=" + m.parts.size());
        for (ModelPart p : (List<ModelPart>) m.parts) {
            dump("", p);
        }
    }

    private static void call(Object o, String name, boolean v) throws Exception {
        Method m = null;
        for (Class<?> c = o.getClass(); c != null && m == null; c = c.getSuperclass()) {
            try {
                m = c.getDeclaredMethod(name, boolean.class);
            } catch (NoSuchMethodException e) {
                // keep looking in the superclass
            }
        }
        m.setAccessible(true);
        m.invoke(o, v);
    }

    private static void pose(String label, Model m, LivingEntity e, float age) throws Exception {
        m.prepare(e, 1.3f, 0.6f, 0.0f);
        m.setupAnimation(1.3f, 0.6f, age, 30.0f, 15.0f, 0.0625f, e);
        model(label + " (swing 1.3, amount 0.6, head yaw 30, pitch 15)", m);
    }

    private static void wolfAndOcelot() throws Exception {
        Bootstrap.init();
        for (int b = 0; b < 16; b++) {
            DyeColor c = DyeColor.byMetadata(b);
            float[] rgb = SheepEntity.getColorRgb(c);
            System.out.println("collar byte " + b + " -> " + c + " rgb=" + rgb[0] + "," + rgb[1] + "," + rgb[2]);
        }
        model("WolfModel", new WolfModel());
        WolfEntity wolf = new WolfEntity(null);
        System.out.println("tail angle wild=" + wolf.getTailBob());
        pose("WolfModel wild", new WolfModel(), wolf, wolf.getTailBob());
        wolf.setAngry(true);
        System.out.println("tail angle angry=" + wolf.getTailBob());
        pose("WolfModel angry", new WolfModel(), wolf, wolf.getTailBob());
        wolf.setAngry(false);
        wolf.setTamed(true);
        call(wolf, "setSitting", true);
        System.out.println("tail angle tame, metadata health 1=" + wolf.getTailBob());
        pose("WolfModel tame sitting", new WolfModel(), wolf, wolf.getTailBob());
        Class<?> wc = WolfEntity.class;
        java.lang.reflect.Field last = wc.getDeclaredField("lastShakeProgress");
        java.lang.reflect.Field now = wc.getDeclaredField("shakeProgress");
        last.setAccessible(true);
        now.setAccessible(true);
        last.setFloat(wolf, 0.8f);
        now.setFloat(wolf, 0.85f);
        for (float off : new float[] {0.0f, -0.08f, -0.16f, -0.2f}) {
            System.out.println("shake angle partial 0.5 offset " + off + "=" + wolf.getShakeAngle(0.5f, off));
        }
        call(wolf, "setSitting", false);
        java.lang.reflect.Field headNow = wc.getDeclaredField("headRollAngle");
        java.lang.reflect.Field headLast = wc.getDeclaredField("lastHeadRollAngle");
        headNow.setAccessible(true);
        headLast.setAccessible(true);
        headLast.setFloat(wolf, 0.64f);
        headNow.setFloat(wolf, 0.84f);
        System.out.println("head roll angle partial 0.5=" + wolf.getHeadRollAngle(0.5f));
        System.out.println("shake progress partial 0.5 (shading while wet)=" + wolf.getShakeProgress(0.5f));
        pose("WolfModel tame shaking (prev 0.8, now 0.85, partial 1.0)", new WolfModel(), wolf, wolf.getTailBob());
        model("OcelotModel", new OcelotModel());
        OcelotEntity cat = new OcelotEntity(null);
        pose("OcelotModel walking", new OcelotModel(), cat, 0.0f);
        call(cat, "setSneaking", true);
        pose("OcelotModel sneaking", new OcelotModel(), cat, 0.0f);
        call(cat, "setSneaking", false);
        call(cat, "setSprinting", true);
        pose("OcelotModel sprinting", new OcelotModel(), cat, 0.0f);
        call(cat, "setSprinting", false);
        call(cat, "setSitting", true);
        pose("OcelotModel sitting", new OcelotModel(), cat, 0.0f);
        call(cat, "setSneaking", true);
        pose("OcelotModel sitting and sneaking", new OcelotModel(), cat, 0.0f);
    }

    private static void setInt(Object o, String name, int v) throws Exception {
        Field f = RabbitEntity.class.getDeclaredField(name);
        f.setAccessible(true);
        f.setInt(o, v);
    }

    /** The rabbit model at rest and at three points of a jump (`getJumpCompletion` is ticks + partial over 10). */
    private static void rabbit() throws Exception {
        model("RabbitModel", new RabbitModel());
        RabbitEntity rabbit = new RabbitEntity(null);
        pose("RabbitModel idle", new RabbitModel(), rabbit, 0.0f);
        setInt(rabbit, "jumpDuration", 10);
        for (int ticks : new int[] {2, 5, 8}) {
            setInt(rabbit, "jumpTicks", ticks);
            pose("RabbitModel jump tick " + ticks + " partial 0", new RabbitModel(), rabbit, 0.0f);
        }
    }

    public static void main(String[] args) throws Exception {
        model("SlimeModel(16)", new SlimeModel(16));
        model("SlimeModel(0)", new SlimeModel(0));
        model("MagmaCubeModel", new MagmaCubeModel());
        BlazeModel blaze = new BlazeModel();
        model("BlazeModel", blaze);
        blaze.setupAnimation(0f, 0f, 10f, 30f, 15f, 0f, null);
        model("BlazeModel after setupAnimation(age 10, headYaw 30, pitch 15)", blaze);
        GhastModel ghast = new GhastModel();
        model("GhastModel", ghast);
        ghast.setupAnimation(0f, 0f, 10f, 0f, 0f, 0f, null);
        model("GhastModel after setupAnimation(age 10)", ghast);
        model("BatModel", new BatModel());
        model("EnderCrystalModel(0, true)", new EnderCrystalModel(0.0f, true));
        wolfAndOcelot();
        rabbit();
        EndermiteModel mite = new EndermiteModel();
        model("EndermiteModel", mite);
        mite.setupAnimation(1.3f, 0.6f, 10.0f, 30.0f, 15.0f, 0.0625f, new net.minecraft.flat.EndermiteEntity(null));
        model("EndermiteModel after setupAnimation(swing 1.3, amount 0.6, age 10)", mite);
        SquidModel squid = new SquidModel();
        model("SquidModel", squid);
        squid.setupAnimation(0f, 0f, 0.4f, 0f, 0f, 0.0625f, null);
        model("SquidModel after setupAnimation(age 0.4)", squid);
        SnowGolemModel golem = new SnowGolemModel();
        model("SnowGolemModel", golem);
        golem.setupAnimation(0f, 0f, 0f, 30f, 15f, 0.0625f, null);
        model("SnowGolemModel after setupAnimation(head yaw 30, pitch 15)", golem);
        WitchModel witch = new WitchModel(0.0f);
        model("WitchModel", witch);
    }
}
