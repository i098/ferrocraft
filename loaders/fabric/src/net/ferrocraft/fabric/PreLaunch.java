package net.ferrocraft.fabric;

import java.io.File;
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
        // Unsanitized, because the sanitized list also drops --username. The token goes to the engine in its environment, not its argv: other users can read argv.
        String[] launchArgs = loader.getLaunchArguments(false);
        String token = null;
        for (int i = 0; i < launchArgs.length; i++) {
            if (launchArgs[i].equals("--accessToken") && i + 1 < launchArgs.length) token = launchArgs[++i];
            else cmd.add(launchArgs[i]);
        }
        File jar = findClientJar(loader);
        if (jar != null) {
            cmd.add("--assetsDir");
            cmd.add(jar.getPath());
        }
        System.out.println("ferrocraft-fabric: starting " + cmd);
        long start = System.currentTimeMillis();
        ProcessBuilder pb = new ProcessBuilder(cmd).inheritIO();
        if (token != null) pb.environment().put("FERROCRAFT_ACCESS_TOKEN", token);
        int code = pb.start().waitFor();
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
