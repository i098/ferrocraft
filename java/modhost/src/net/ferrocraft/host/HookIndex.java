package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import net.minecraft.launchwrapper.IClassTransformer;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.AnnotationNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;

/**
 * The hook table: the vanilla methods that a mod changed. {@link Scanner} shows each Minecraft class to {@link #scan}
 * after Mixin ran. Mixin marks every method it merged into the class with {@code @MixinMerged}, and an injector (Inject,
 * Redirect, ModifyArg, ModifyVariable, ModifyConstant) leaves a call to such a method in the body of the vanilla method. So a
 * vanilla method is changed when
 *
 * <ul>
 *   <li>its body calls a merged method of its own class, or
 *   <li>it is itself a merged method that replaces a vanilla one: an {@code @Overwrite}.
 * </ul>
 *
 * Methods are in the names the mods see (SRG): {@code owner.name desc} with the owner as an internal class name.
 */
public final class HookIndex {
    private static final String MERGED = "Lorg/spongepowered/asm/mixin/transformer/meta/MixinMerged;";
    private static final String OVERWRITE = "Lorg/spongepowered/asm/mixin/Overwrite;";
    private static final byte[] MERGED_UTF8 = MERGED.substring(1, MERGED.length() - 1).getBytes(StandardCharsets.US_ASCII);

    private static final Set<String> changed = new TreeSet<>();

    private HookIndex() {}

    /**
     * Sees each Minecraft class after Mixin. Mixin's transformer moves itself to the end of the list when the launch
     * arguments are read, behind every transformer a tweaker registered while it was injected, so
     * {@link net.ferrocraft.tweak.HostLastTweaker} registers this one at that point.
     */
    public static final class Scanner implements IClassTransformer {
        @Override
        public byte[] transform(String name, String transformedName, byte[] bytes) {
            if (bytes != null && transformedName.startsWith("net.minecraft")) scan(bytes);
            return bytes;
        }
    }

    /** Records the changed methods of one class, as the class arrives from Mixin. Classes with no merged method cost one byte search. */
    public static void scan(byte[] bytes) {
        if (indexOf(bytes, MERGED_UTF8) < 0) return;
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, ClassReader.SKIP_DEBUG | ClassReader.SKIP_FRAMES);
        List<String> found = changedMethods(cls);
        if (found.isEmpty()) return;
        synchronized (changed) {
            changed.addAll(found);
        }
    }

    static List<String> changedMethods(ClassNode cls) {
        Set<String> merged = new HashSet<>();
        for (MethodNode m : methods(cls)) {
            if (has(m, MERGED)) merged.add(m.name + m.desc);
        }
        List<String> out = new ArrayList<>();
        for (MethodNode m : methods(cls)) {
            // Mixin and MixinExtras name their own helpers with a dollar sign (lambda$, handler$, mixinextras$wrapped$).
            if (m.name.indexOf('$') >= 0) continue;
            String key = cls.name + "." + m.name + m.desc;
            if (merged.contains(m.name + m.desc)) {
                if (has(m, OVERWRITE)) out.add(key);
                continue;
            }
            for (AbstractInsnNode i = m.instructions.getFirst(); i != null; i = i.getNext()) {
                if (i instanceof MethodInsnNode) {
                    MethodInsnNode call = (MethodInsnNode) i;
                    if (call.owner.equals(cls.name) && merged.contains(call.name + call.desc)) {
                        out.add(key);
                        break;
                    }
                }
            }
        }
        return out;
    }

    /** The methods found so far, sorted. */
    public static List<String> list() {
        synchronized (changed) {
            return new ArrayList<>(changed);
        }
    }

    /**
     * Mixin changes a class when the class loads, and the host has loaded only the classes it needs by now. This loads, without
     * running any initializer, every Minecraft class that a Mixin config targets, so that the table holds them all. It does
     * nothing when no Mixin is on the class path, and a Mixin whose classes do not answer leaves the table as found.
     */
    @SuppressWarnings("unchecked")
    public static void settle() {
        Set<String> targets = new TreeSet<>();
        try {
            // Mixins.getConfigs() holds only the configs that Mixin has not applied yet; the created ones stay in this map.
            Map<String, ?> configs = (Map<String, ?>) R.call(R.cls("org.spongepowered.asm.mixin.transformer.Config"), "getAllConfigs");
            for (Object config : configs.values()) {
                targets.addAll((Set<String>) R.call(R.call(config, "getConfig"), "getTargets"));
            }
        } catch (RuntimeException e) {
            Host.log("hook table: no Mixin config list: " + e);
            return;
        }
        ClassLoader loader = Thread.currentThread().getContextClassLoader();
        for (String target : targets) {
            if (!target.startsWith("net.minecraft")) continue;
            try {
                Class.forName(target.replace('/', '.'), false, loader);
            } catch (ClassNotFoundException | LinkageError e) {
                Host.log("hook table: " + target + " did not load: " + e);
            }
        }
    }

    /**
     * Part of the hello: u32 count, then per method three writeUTF strings: owner, name, descriptor. Each string stays far
     * below the 64 KiB limit of writeUTF.
     */
    public static void write(DataOutputStream out) throws IOException {
        List<String> all = list();
        out.writeInt(all.size());
        for (String s : all) {
            int dot = s.indexOf('.');
            int paren = s.indexOf('(');
            out.writeUTF(s.substring(0, dot));
            out.writeUTF(s.substring(dot + 1, paren));
            out.writeUTF(s.substring(paren));
        }
    }

    /** Reads the table {@link #write} wrote, for a reader that does not need it: the engine's side of the hello. */
    public static void skip(DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            in.readUTF();
            in.readUTF();
            in.readUTF();
        }
    }

    /** ASM 5's node fields are raw lists. */
    @SuppressWarnings("unchecked")
    private static List<MethodNode> methods(ClassNode cls) {
        return cls.methods;
    }

    private static boolean has(MethodNode m, String desc) {
        return contains(m.visibleAnnotations, desc) || contains(m.invisibleAnnotations, desc);
    }

    @SuppressWarnings("unchecked")
    private static boolean contains(List<?> list, String desc) {
        if (list != null) {
            for (AnnotationNode a : (List<AnnotationNode>) list) {
                if (a.desc.equals(desc)) return true;
            }
        }
        return false;
    }

    private static int indexOf(byte[] hay, byte[] needle) {
        outer:
        for (int i = 0; i + needle.length <= hay.length; i++) {
            for (int j = 0; j < needle.length; j++) {
                if (hay[i + j] != needle[j]) continue outer;
            }
            return i;
        }
        return -1;
    }
}
