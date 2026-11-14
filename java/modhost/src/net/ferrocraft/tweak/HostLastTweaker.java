package net.ferrocraft.tweak;

import java.io.File;
import java.util.List;
import net.minecraft.launchwrapper.ITweaker;
import net.minecraft.launchwrapper.LaunchClassLoader;

/** Queued by {@link HostLateTweaker}; runs after FML's deobfuscation tweaker, so it registers the Minecraft-class transformer. */
public final class HostLastTweaker implements ITweaker {
    @Override
    public void acceptOptions(List<String> args, File gameDir, File assetsDir, String profile) {
    }

    private LaunchClassLoader loader;

    @Override
    public void injectIntoClassLoader(LaunchClassLoader cl) {
        loader = cl;
        cl.registerTransformer("net.ferrocraft.host.MinecraftBridgeTransformer");
    }

    @Override
    public String getLaunchTarget() {
        return null;
    }

    @Override
    public String[] getLaunchArguments() {
        // Mixin's transformer moves to the end of the list while the launch arguments are read; the hook table must see its result.
        loader.registerTransformer("net.ferrocraft.host.HookIndex$Scanner");
        // After the scanner, which must see the hooks of the whole method before the shell drops the body.
        loader.registerTransformer("net.ferrocraft.host.ShellTransformer");
        return new String[0];
    }
}
