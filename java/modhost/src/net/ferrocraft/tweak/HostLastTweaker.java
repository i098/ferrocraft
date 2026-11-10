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

    @Override
    public void injectIntoClassLoader(LaunchClassLoader cl) {
        cl.registerTransformer("net.ferrocraft.host.MinecraftBridgeTransformer");
    }

    @Override
    public String getLaunchTarget() {
        return null;
    }

    @Override
    public String[] getLaunchArguments() {
        return new String[0];
    }
}
