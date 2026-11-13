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
            heldSneakKeyIsReadBackAsSneaking();
            overridesRunTheVanillaGetters();
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

    private static void heldSneakKeyIsReadBackAsSneaking() throws IOException {
        Object mc = Minecraft.create(Launch.minecraftHome);
        Object settings = R.get(mc, "field_71474_y");
        Player player = new Player(mc, R.get(mc, "field_71439_g"), settings);
        int sneak = (Integer) R.call(R.get(settings, "field_74311_E"), "func_151463_i"); // keyBindSneak.getKeyCode
        Class<?> keyBinding = R.cls("net.minecraft.client.settings.KeyBinding");

        R.call(keyBinding, "func_74510_a", sneak, false);
        check(tick(player, 0)[1] == 0, "sneak is not read back while the key is up");
        R.call(keyBinding, "func_74510_a", sneak, true);
        check(tick(player, 0)[1] == 1, "sneak is read back while the key is held");
    }

    /** With no mod hooked in, the getters return what Rust sent; a mod's hook changes that one value. */
    private static void overridesRunTheVanillaGetters() throws IOException {
        Object mc = Minecraft.create(Launch.minecraftHome);
        new Player(mc, R.get(mc, "field_71439_g"), R.get(mc, "field_71474_y"));
        Overrides overrides = new Overrides(mc);
        DataInputStream r = ask(overrides, 0, 6000, 0.8f, 0.1f, -1);
        check(r.readUnsignedByte() == 0b101111, "all groups but night vision, which vanilla refuses without the effect");
        check(r.readLong() == 6000, "the world time");
        float rainNow = r.readFloat();
        float thunderNow = r.readFloat();
        check(rainNow == 0.8f && Math.abs(thunderNow - 0.1f * 0.8f) < 1e-6, "rain, and thunder weighted by rain as vanilla: " + rainNow + " " + thunderNow);
        check(Math.abs(r.readFloat() - 30) < 1e-3 && Math.abs(r.readFloat() - 10) < 1e-3, "the camera looks where the player looks");
        r.readFloat(); // distance: only third person has one
        check(Math.abs(r.readFloat() - 70) < 1e-3, "the FOV");
        r.readFloat(); // night vision brightness
        check(r.readFloat() == 0 && r.readFloat() == 0, "no look, no turn");
        r = ask(overrides, 1, 18000, 0, 0, 200);
        check(r.readUnsignedByte() == 0b111111, "night vision answers while the effect is on");
        check(r.readLong() == 18000, "the time follows the request");
        r.skipBytes(4 * 4);
        check(Math.abs(r.readFloat() - 4) < 1e-3, "the third person camera sits 4 blocks back");
    }

    /** One 'O' request after the tag, at yaw 30, pitch 10, FOV 70 and no night vision. */
    private static DataInputStream ask(Overrides overrides, int perspective, long time, float rain, float thunder, int nightVisionTicks) throws IOException {
        ByteArrayOutputStream request = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(request);
        for (float v : new float[] {0.5f, 30, 10, 0, 0}) w.writeFloat(v);
        w.writeByte(perspective);
        w.writeLong(time);
        for (float v : new float[] {rain, thunder, 70, 1, 1}) w.writeFloat(v);
        w.writeInt(nightVisionTicks);
        ByteArrayOutputStream reply = new ByteArrayOutputStream();
        overrides.answer(new DataInputStream(new ByteArrayInputStream(request.toByteArray())), new DataOutputStream(reply));
        return new DataInputStream(new ByteArrayInputStream(reply.toByteArray()));
    }

    /** Runs one tick and returns the reply: the sprint and sneak bytes. */
    private static byte[] tick(Player player, int flags) throws IOException {
        ByteArrayOutputStream request = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(request);
        w.writeByte(0); // onGround
        w.writeByte(0); // collidedHorizontally
        w.writeInt(20); // foodLevel
        w.writeByte(flags);
        ByteArrayOutputStream reply = new ByteArrayOutputStream();
        player.tick(new DataInputStream(new ByteArrayInputStream(request.toByteArray())), new DataOutputStream(reply));
        check(reply.size() == 2, "the reply is the sprint and sneak bytes");
        return reply.toByteArray();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
