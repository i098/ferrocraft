package net.ferrocraft.host;

/** {@link BridgeTransformer} for Minecraft's classes; registered after FML's deobfuscation transformer. */
public final class MinecraftBridgeTransformer extends BridgeTransformer {
    @Override
    protected boolean minecraftClasses() {
        return true;
    }
}
