import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import net.minecraft.flat.BatModel;
import net.minecraft.flat.BlazeModel;
import net.minecraft.flat.GhastModel;
import net.minecraft.flat.MagmaCubeModel;
import net.minecraft.flat.Model;
import net.minecraft.flat.ModelPart;
import net.minecraft.flat.SlimeModel;

/**
 * Prints the part tree of the slime, magma cube, blaze, ghast and bat models of the real 1.8.9
 * client: texture offset, pivot, rotation, mirror flag and box corners of every part. The blaze,
 * ghast and magma cube models are also run through setupAnimation (the bat one reads entity
 * metadata, which needs a world, so only its constructor runs). Java 8 source level.
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
    }
}
