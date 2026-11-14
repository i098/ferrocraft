package net.ferrocraft.fabric;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.ProtectionDomain;
import java.util.jar.JarFile;
import net.ferrocraft.host.BridgeTransformer;
import net.ferrocraft.host.HookIndex;
import net.ferrocraft.host.MinecraftBridgeTransformer;
import net.ferrocraft.host.ShellTransformer;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

/**
 * The Java agent of the Fabric host. Knot defines the game classes itself, after Mixin ran, so the host cannot register a
 * transformer with it as it does with LaunchWrapper. The agent sees the bytes that Knot hands to {@code defineClass} and
 * gives them the same three steps as the Forge host: the hook table scan, the loop method shells and the bridge. It also
 * replaces {@code net.minecraft.client.main.Main}, the class Knot calls, with a call of {@code Host.run}.
 *
 * <p>System properties: {@code ferro.tiny} is the name table (Tiny v2, MCP names to intermediary names); the agent turns it
 * into the file that {@code Names} reads and sets {@code ferro.names}. {@code ferro.gameDir} is the game directory.
 */
public final class Agent {
    private static final String MAIN = "net/minecraft/client/main/Main";
    private static final String MC = "net/minecraft/";

    private Agent() {}

    public static void premain(String args, Instrumentation inst) throws IOException {
        Path names = Files.createTempFile("ferro-names", ".txt");
        names.toFile().deleteOnExit();
        flatten(Paths.get(System.getProperty("ferro.tiny")), names);
        System.setProperty("ferro.names", names.toString());
        // Loader 0.20 lets Knot read only the class path jars it knows; this jar holds the host classes the swapped Main starts.
        try {
            String self = Paths.get(Agent.class.getProtectionDomain().getCodeSource().getLocation().toURI()).toString();
            String libs = System.getProperty("fabric.systemLibraries");
            System.setProperty("fabric.systemLibraries", libs == null ? self : libs + java.io.File.pathSeparator + self);
        } catch (java.net.URISyntaxException e) {
            throw new IOException(e);
        }
        inst.addTransformer(new Dispatch(), false);
        log("installed");
    }

    private static void log(String s) {
        System.out.println("[ferro-agent] " + s);
    }

    /**
     * Tiny v2 to the lines of {@code Names}: a name, a tab, the loader's name. A class name has dots; a field or a method keeps
     * its name, which is unique ({@code field_71439_g}, {@code func_71384_a}).
     */
    static void flatten(Path tiny, Path out) throws IOException {
        try (BufferedReader in = Files.newBufferedReader(tiny, StandardCharsets.UTF_8);
             BufferedWriter w = Files.newBufferedWriter(out, StandardCharsets.UTF_8)) {
            String line = in.readLine();
            if (line == null || !line.startsWith("tiny\t2\t")) throw new IOException(tiny + " is not a Tiny v2 file");
            while ((line = in.readLine()) != null) {
                String[] p = line.split("\t", -1);
                boolean cls = p.length == 3 && p[0].equals("c");
                boolean member = p.length == 5 && p[0].isEmpty() && (p[1].equals("f") || p[1].equals("m"));
                String from = cls ? p[1].replace('/', '.') : member ? p[3] : null;
                String to = cls ? p[2].replace('/', '.') : member ? p[4] : null;
                if (from == null || to.isEmpty() || from.equals(to)) continue;
                w.write(from + "\t" + to);
                w.newLine();
            }
        }
    }

    /**
     * Runs in the system class loader, where the agent is. Knot defines the host classes (they are on its class path) in its own
     * loader, so the work is done by {@link Knot} as Knot loads it: one copy of {@code Sink}, {@code HookIndex} and the rest,
     * the one the swapped {@code Main} starts.
     */
    private static final class Dispatch implements ClassFileTransformer {
        private static final ThreadLocal<Boolean> BUSY = new ThreadLocal<Boolean>();
        private Method knot;

        @Override
        public byte[] transform(ClassLoader loader, String cn, Class<?> redefined, ProtectionDomain pd, byte[] bytes) {
            // The host classes and ASM load while Knot is set up here: they need no patch and must not re-enter.
            if (cn == null || bytes == null || loader == null || BUSY.get() != null || cn.startsWith("net/ferrocraft/")) return null;
            // Only Knot's loader: the launcher's own loader defines a Main of its own while the crash handler runs.
            if (!loader.getClass().getName().endsWith("KnotClassLoader")) return null;
            BUSY.set(Boolean.TRUE);
            try {
                if (knot == null) knot = Class.forName("net.ferrocraft.fabric.Agent$Knot", true, loader).getMethod("transform", String.class, byte[].class);
                return (byte[]) knot.invoke(null, cn, bytes);
            } catch (Throwable t) {
                // An exception out of a transformer makes the JVM skip it silently and leave the class unpatched.
                log("transform failed for " + cn + ": " + t);
                return null;
            } finally {
                BUSY.remove();
            }
        }
    }

    /** The three steps of the Forge host on the bytes Knot hands over, after Mixin: the hook table scan, the loop shells, the bridge. */
    public static final class Knot {
        private static final BridgeTransformer OTHER = new BridgeTransformer();
        private static final BridgeTransformer MINECRAFT = new MinecraftBridgeTransformer();
        private static final ShellTransformer SHELL = new ShellTransformer();

        private static final String DISPLAY = "org/lwjgl/opengl/Display";

        private Knot() {}

        private static byte[] standIn(String cn) {
            try {
                File jar = new File(Knot.class.getProtectionDomain().getCodeSource().getLocation().toURI());
                try (JarFile j = new JarFile(jar); InputStream in = j.getInputStream(j.getEntry(cn + ".class"))) {
                    ByteArrayOutputStream out = new ByteArrayOutputStream();
                    byte[] buf = new byte[4096];
                    for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
                    return out.toByteArray();
                }
            } catch (Exception e) {
                throw new IllegalStateException("no stand-in for " + cn + " in the host jar", e);
            }
        }

        /** The new bytes of the class, or null to leave it. */
        public static byte[] transform(String cn, byte[] bytes) {
            if (cn.equals(MAIN)) {
                log("swap Main");
                return mainStub();
            }
            // LWJGL 2's Display loads a native library and needs a display at class load: the host owns the window, and its jar holds a stand-in.
            if (cn.equals(DISPLAY)) return standIn(cn);
            String dotted = cn.replace('/', '.');
            byte[] out = bytes;
            boolean game = cn.startsWith(MC);
            if (game) {
                HookIndex.scan(out);
                out = SHELL.transform(dotted, dotted, out);
            }
            out = (game ? MINECRAFT : OTHER).transform(dotted, dotted, out);
            return out == bytes ? null : out;
        }
    }

    /**
     * {@code Main} with one method: {@code FabricGlue.initEntrypoints(); Host.run(args);}.
     */
    static byte[] mainStub() {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cw.visit(Opcodes.V1_8, Opcodes.ACC_PUBLIC, MAIN, null, "java/lang/Object", null);
        MethodVisitor mv = cw.visitMethod(Opcodes.ACC_PUBLIC | Opcodes.ACC_STATIC, "main", "([Ljava/lang/String;)V", null, null);
        mv.visitCode();
        mv.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/FabricGlue", "initEntrypoints", "()V", false);
        mv.visitVarInsn(Opcodes.ALOAD, 0);
        mv.visitMethodInsn(Opcodes.INVOKESTATIC, "net/ferrocraft/host/Host", "run", "([Ljava/lang/String;)V", false);
        mv.visitInsn(Opcodes.RETURN);
        mv.visitMaxs(0, 0);
        mv.visitEnd();
        cw.visitEnd();
        return cw.toByteArray();
    }
}
