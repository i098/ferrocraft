package net.minecraft.client.main;

/** Forge's FMLTweaker launches this class name; it shadows the vanilla entry point on the class path. */
public final class Main {
    private Main() {}

    public static void main(String[] args) throws Exception {
        net.ferrocraft.host.Host.run(args);
    }
}
