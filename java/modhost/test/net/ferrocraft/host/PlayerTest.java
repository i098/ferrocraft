package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import net.minecraft.launchwrapper.Launch;

/**
 * Runs the stand-in player under FML with the host's transformers, as the host does. Rust reads no flight state back
 * and sends the abilities packet itself, so a flight toggle in the JVM must neither throw nor send anything.
 */
public final class PlayerTest {
    private PlayerTest() {}

    public static void run() {
        int code = 1;
        try {
            doubleTapJumpTogglesFlightWhenFlyingIsAllowed();
            System.out.println("PlayerTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    private static void doubleTapJumpTogglesFlightWhenFlyingIsAllowed() throws IOException {
        Object mc = Minecraft.create(Launch.minecraftHome);
        Object settings = R.get(mc, "field_71474_y");
        Player player = new Player(mc, R.get(mc, "field_71439_g"), settings);
        Object caps = R.get(R.get(mc, "field_71439_g"), "field_71075_bZ");
        int jump = (Integer) R.call(R.get(settings, "field_74314_A"), "func_151463_i"); // keyBindJump.getKeyCode
        Class<?> keyBinding = R.cls("net.minecraft.client.settings.KeyBinding");

        boolean[] jumpHeld = {true, false, true};
        for (boolean held : jumpHeld) {
            R.call(keyBinding, "func_74510_a", jump, held);
            tick(player, 1); // allowFlying
        }
        check(Boolean.TRUE.equals(R.get(caps, "field_75100_b")), "the second press of jump must start flying");
    }

    private static void tick(Player player, int flags) throws IOException {
        ByteArrayOutputStream request = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(request);
        w.writeByte(0); // onGround
        w.writeByte(0); // collidedHorizontally
        w.writeInt(20); // foodLevel
        w.writeByte(flags);
        ByteArrayOutputStream reply = new ByteArrayOutputStream();
        player.tick(new DataInputStream(new ByteArrayInputStream(request.toByteArray())), new DataOutputStream(reply));
        check(reply.size() == 2, "the reply is the sprint and sneak bytes");
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
