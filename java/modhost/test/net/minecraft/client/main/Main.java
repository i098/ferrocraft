package net.minecraft.client.main;

/** Shadows the host's entry point (see test.sh): FML launches this class, which runs the tests instead of the host. */
public final class Main {
    private Main() {}

    public static void main(String[] args) {
        net.ferrocraft.host.PlayerTest.run();
    }
}
