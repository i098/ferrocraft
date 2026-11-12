package net.ferrocraft.boot;

import java.io.File;
import java.util.ArrayList;
import java.util.List;

/** Starts the Rust engine binary and waits for it. Loaded by {@link Main}'s stand-in-first loader. */
public final class Engine {
    private Engine() {}

    public static int run(String[] args) throws Exception {
        Class<?> brand = Class.forName("net.minecraft.client.ClientBrandRetriever");
        System.out.println("ferrocraft-boot: ClientBrandRetriever=" + net.minecraft.client.ClientBrandRetriever.getClientModName()
                + " from " + brand.getProtectionDomain().getCodeSource().getLocation());

        File engine = findEngine();
        List<String> cmd = new ArrayList<String>();
        cmd.add(engine.getPath());
        for (String a : args) cmd.add(a);
        String clientJar = findClientJar();
        if (clientJar != null) {
            cmd.add("--assetsDir");
            cmd.add(clientJar);
        }
        System.out.println("ferrocraft-boot: starting " + engine);
        return new ProcessBuilder(cmd).inheritIO().start().waitFor();
    }

    /** The classpath jar that holds the vanilla font: the engine reads textures and text from it, wherever the launcher keeps it. */
    private static String findClientJar() {
        String cp = System.getProperty("java.class.path");
        if (cp == null) return null;
        for (String entry : cp.split(File.pathSeparator)) {
            if (!entry.endsWith(".jar")) continue;
            try (java.util.zip.ZipFile zip = new java.util.zip.ZipFile(entry)) {
                if (zip.getEntry("assets/minecraft/textures/font/ascii.png") != null) return new File(entry).getAbsolutePath();
            } catch (java.io.IOException e) {
                // not a readable jar: skip it
            }
        }
        return null;
    }

    private static File findEngine() throws Exception {
        String env = System.getenv("FERROCRAFT_ENGINE");
        if (env != null && !env.isEmpty()) return new File(env);
        File jar = new File(Engine.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        File mc = jar;
        for (int i = 0; i < 6 && mc != null; i++) mc = mc.getParentFile();
        if (mc == null) throw new IllegalStateException("no engine: boot jar is not under <minecraft-dir>/libraries");
        File dir = new File(mc, "ferrocraft");
        File exe = new File(dir, "ferrocraft-engine.exe");
        return exe.isFile() ? exe : new File(dir, "ferrocraft-engine");
    }
}
