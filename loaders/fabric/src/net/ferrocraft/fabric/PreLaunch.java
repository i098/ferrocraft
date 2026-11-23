package net.ferrocraft.fabric;

import java.io.File;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.fabricmc.loader.api.entrypoint.PreLaunchEntrypoint;

/**
 * Fabric pre-launch entrypoint. Runs the Rust engine with the game's launch arguments, then exits
 * the JVM with the engine's exit code, so the vanilla client never starts.
 */
public final class PreLaunch implements PreLaunchEntrypoint {
    @Override
    public void onPreLaunch() {
        int code;
        try {
            code = run();
        } catch (Exception e) {
            e.printStackTrace();
            code = 1;
        }
        System.exit(code);
    }

    private static int run() throws Exception {
        FabricLoader loader = FabricLoader.getInstance();
        File engine = findEngine(loader.getGameDir());
        List<String> cmd = new ArrayList<String>();
        cmd.add(engine.getPath());
        String[] launchArgs = loader.getLaunchArguments(false);
        String token = null;
        for (int i = 0; i < launchArgs.length; i++) {
            String arg = launchArgs[i];
            if (arg.equals("--accessToken")) {
                if (i + 1 < launchArgs.length && !launchArgs[i + 1].startsWith("--")) token = launchArgs[++i];
            } else if (arg.startsWith("--accessToken=")) {
                throw new IllegalArgumentException("Use --accessToken VALUE.");
            } else {
                cmd.add(arg);
            }
        }
        File jar = findClientJar(loader);
        if (jar != null) {
            cmd.add("--assetsDir");
            cmd.add(jar.getPath());
        }
        System.out.println("ferrocraft-fabric: starting " + cmd);
        long start = System.currentTimeMillis();
        ProcessBuilder pb = new ProcessBuilder(cmd).inheritIO();
        String fallback = pb.environment().remove("FERROCRAFT_ACCESS_TOKEN");
        if (token == null) token = fallback;
        // The mod folder of a OneClient cluster sits in the cluster directory, which holds .oneclient-mods.json.
        String modsFolder = System.getProperty("fabric.modsFolder");
        if (modsFolder != null && !modsFolder.isEmpty()) {
            File parent = new File(modsFolder).getAbsoluteFile().getParentFile();
            if (parent != null) pb.environment().put("FERROCRAFT_CLUSTER", parent.getPath());
        }
        if (token != null) {
            cmd.add("--access-token-stdin");
            pb.redirectInput(ProcessBuilder.Redirect.PIPE);
        }
        Process child = pb.start();
        if (token != null) {
            try (OutputStream input = child.getOutputStream()) {
                input.write(token.getBytes(StandardCharsets.UTF_8));
            }
        }
        int code = child.waitFor();
        // Java reports 128+N when the engine died from signal N; the engine's own exit reasons are in ~/.ferrocraft/engine.log.
        System.out.println("ferrocraft-fabric: engine exited with code " + code + " after " + (System.currentTimeMillis() - start) / 1000 + "s");
        return code;
    }

    /** Vanilla jar of the `minecraft` mod: the engine reads textures and the font from it. Last flag wins in the engine's parser. */
    private static File findClientJar(FabricLoader loader) {
        ModContainer mc = loader.getModContainer("minecraft").orElse(null);
        if (mc == null) return null;
        for (Path p : mc.getOrigin().getPaths()) {
            if (p.toString().endsWith(".jar")) return p.toFile();
        }
        return null;
    }

    /** `FERROCRAFT_ENGINE`, then `<gameDir>/ferrocraft/`, then `~/.ferrocraft/`. */
    private static File findEngine(Path gameDir) {
        String env = System.getenv("FERROCRAFT_ENGINE");
        if (env != null && !env.isEmpty()) return new File(env);
        File[] dirs = {
            gameDir.resolve("ferrocraft").toFile(),
            new File(System.getProperty("user.home"), ".ferrocraft"),
        };
        for (File dir : dirs) {
            for (String name : new String[] {"ferrocraft-engine", "ferrocraft-engine.exe"}) {
                File exe = new File(dir, name);
                if (exe.isFile()) return exe;
            }
        }
        throw new IllegalStateException("ferrocraft-engine not found: set FERROCRAFT_ENGINE or put it in <gameDir>/ferrocraft/ or ~/.ferrocraft/");
    }
}
