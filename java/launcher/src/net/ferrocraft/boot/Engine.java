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

        File engine = findEngine(args);
        List<String> cmd = new ArrayList<String>();
        cmd.add(engine.getPath());
        for (String a : args) cmd.add(a);
        System.out.println("ferrocraft-boot: starting " + engine);
        return new ProcessBuilder(cmd).inheritIO().start().waitFor();
    }

    private static File findEngine(String[] args) {
        String env = System.getenv("FERROCRAFT_ENGINE");
        if (env != null && !env.isEmpty()) return new File(env);
        for (int i = 0; i + 1 < args.length; i++) {
            if (args[i].equals("--gameDir")) return new File(args[i + 1], "ferrocraft/ferrocraft-engine");
        }
        throw new IllegalStateException("no engine: set FERROCRAFT_ENGINE or pass --gameDir");
    }
}
