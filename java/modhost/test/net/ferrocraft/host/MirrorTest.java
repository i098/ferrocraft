package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.UUID;
import net.minecraft.launchwrapper.Launch;

/** Sends one mirror message, as mirror.rs writes it, and reads the values back the way the HUD mods do. */
public final class MirrorTest {
    private MirrorTest() {}

    public static void run() {
        int code = 1;
        try {
            Object mc = Minecraft.create(Launch.minecraftHome);
            Object player = R.get(mc, "field_71439_g");
            new Player(mc, player, R.get(mc, "field_71474_y"));
            Mirror mirror = new Mirror(mc, player);
            mirror.apply(message());

            check((Integer) R.get(R.cls("net.minecraft.client.Minecraft"), "field_71470_ab") == 144, "fps");
            check((Double) R.get(player, "field_70165_t") == 1.5 && (Double) R.get(player, "field_70163_u") == 64.0 && (Double) R.get(player, "field_70161_v") == -2.5, "position");
            check((Float) R.get(player, "field_70177_z") == 90f && (Float) R.get(player, "field_70125_A") == -10f, "rotation");
            check((Float) R.call(player, "func_110143_aJ") == 18.5f, "health"); // getHealth
            Object handler = R.get(player, "field_71174_a");
            UUID uuid = (UUID) R.call(player, "func_110124_au"); // getUniqueID
            Object info = R.call(handler, "func_175102_a", uuid); // getPlayerInfo
            check((Integer) R.call(info, "func_178853_c") == 37, "ping"); // getResponseTime
            Object world = R.get(mc, "field_71441_e");
            Object pos = R.neu("net.minecraft.util.BlockPos", 1, 64, -3);
            check("Taiga".equals(String.valueOf(R.get(R.call(world, "func_180494_b", pos), "field_76791_y"))), "biome"); // getBiomeGenForCoords, biomeName
            Object inventory = R.get(player, "field_71071_by");
            Object held = R.call(inventory, "func_70448_g"); // getCurrentItem
            check(held != null && (Integer) R.call(held, "func_77960_j") == 5 && (Integer) R.get(held, "field_77994_a") == 3, "held item");
            check(R.call(inventory, "func_70440_f", 3) != null && R.call(inventory, "func_70440_f", 0) == null, "armor"); // armorItemInSlot: 3 is the helmet
            Object effect = ((Map<?, ?>) R.get(player, "field_70713_bf")).get(1);
            check(effect != null && (Integer) R.call(effect, "func_76459_b") == 600, "effect"); // getDuration
            Object server = R.call(mc, "func_147104_D"); // getCurrentServerData
            check(server != null && "mc.example:25565".equals(R.get(server, "field_78845_b")), "server address");
            System.out.println("MirrorTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    /** Bytes as mirror.rs writes them, without the tag. */
    private static DataInputStream message() throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeInt(144);
        w.writeInt(37);
        w.writeDouble(1.5);
        w.writeDouble(64.0);
        w.writeDouble(-2.5);
        w.writeFloat(90f);
        w.writeFloat(-10f);
        w.writeByte(5); // taiga
        w.writeFloat(18.5f);
        w.writeByte(2); // held slot
        w.writeShort(1);
        w.writeByte(1);
        w.writeByte(0);
        w.writeInt(600);
        for (int i = 0; i < 40; i++) {
            boolean sword = i == 2;
            boolean helmet = i == 39;
            w.writeShort(sword ? 267 : helmet ? 298 : -1);
            w.writeByte(sword ? 3 : helmet ? 1 : 0);
            w.writeShort(sword ? 5 : 0);
        }
        byte[] name = "mc.example:25565".getBytes(StandardCharsets.UTF_8);
        w.writeShort(name.length);
        w.write(name);
        return new DataInputStream(new ByteArrayInputStream(b.toByteArray()));
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
