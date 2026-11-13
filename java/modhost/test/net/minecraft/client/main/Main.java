package net.minecraft.client.main;

/** Shadows the host's entry point (see test.sh): FML launches this class, which runs the test named by -Dferro.test. */
public final class Main {
    private Main() {}

    public static void main(String[] args) {
        if ("ModBootTest".equals(System.getProperty("ferro.test"))) net.ferrocraft.host.ModBootTest.run();
        else if ("ChatTest".equals(System.getProperty("ferro.test"))) net.ferrocraft.host.ChatTest.run();
        else if ("MirrorHudTest".equals(System.getProperty("ferro.test"))) net.ferrocraft.host.MirrorHudTest.run();
        else if ("MirrorTest".equals(System.getProperty("ferro.test"))) net.ferrocraft.host.MirrorTest.run();
        else net.ferrocraft.host.PlayerTest.run();
    }
}
