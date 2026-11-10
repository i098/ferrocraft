package net.ferrocraft.tweak;

import java.io.File;
import java.util.List;
import net.minecraft.launchwrapper.ITweaker;
import net.minecraft.launchwrapper.Launch;
import net.minecraft.launchwrapper.LaunchClassLoader;

/** Queued by {@link HostTweaker}; queues {@link HostLastTweaker}, since this runs before FML's deobfuscation tweaker. */
public final class HostLateTweaker implements ITweaker {
    @Override
    public void acceptOptions(List<String> args, File gameDir, File assetsDir, String profile) {
    }

    @Override
    @SuppressWarnings("unchecked")
    public void injectIntoClassLoader(LaunchClassLoader cl) {
        ((List<String>) Launch.blackboard.get("TweakClasses")).add(HostLastTweaker.class.getName());
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
