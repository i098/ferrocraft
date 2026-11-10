package net.ferrocraft.tweak;

import java.io.File;
import java.util.List;
import java.util.Set;
import net.minecraft.launchwrapper.ITweaker;
import net.minecraft.launchwrapper.Launch;
import net.minecraft.launchwrapper.LaunchClassLoader;

/**
 * Registers the bridge transformer before any mod tweaker runs, so classes a mod loads while it
 * starts (OneConfig's platform classes) are patched too. Passed on the command line after FML's tweaker.
 */
public final class HostTweaker implements ITweaker {
    @Override
    public void acceptOptions(List<String> args, File gameDir, File assetsDir, String profile) {
    }

    @Override
    @SuppressWarnings("unchecked")
    public void injectIntoClassLoader(LaunchClassLoader cl) {
        // Runs after FML's deobfuscation tweaker, when the transformer's Minecraft patches match method names.
        ((List<String>) Launch.blackboard.get("TweakClasses")).add(HostLateTweaker.class.getName());
        cl.addTransformerExclusion("net.ferrocraft.host.");
        // LWJGL loads through the class loader so the transformer can patch it; no native library is used.
        try {
            removeLoaderException(cl, "org.lwjgl.");
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        cl.registerTransformer("net.ferrocraft.host.BridgeTransformer");
        System.out.println("[ferro-host] transformer registered");
    }

    @Override
    public String getLaunchTarget() {
        return null;
    }

    @Override
    public String[] getLaunchArguments() {
        return new String[0];
    }

    @SuppressWarnings("unchecked")
    private static void removeLoaderException(LaunchClassLoader cl, String prefix) throws ReflectiveOperationException {
        java.lang.reflect.Field f = LaunchClassLoader.class.getDeclaredField("classLoaderExceptions");
        f.setAccessible(true);
        ((Set<String>) f.get(cl)).remove(prefix);
    }

}
