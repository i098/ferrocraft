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

            HudLists lists = new HudLists(mc, player);
            lists.apply(lists());
            checkLists(mc, player, handler);
            // The same lists again, after the mirror rewrote the local entry: the local entry keeps the engine's name.
            mirror.apply(message());
            lists.apply(lists());
            checkLists(mc, player, handler);
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

    /** Bytes as hud_lists.rs writes them, without the tag: two players (the local one first), a sidebar, a team, boss, title, action bar. */
    private static DataInputStream lists() throws IOException {
        ByteArrayOutputStream part = new ByteArrayOutputStream();
        DataOutputStream p = new DataOutputStream(part);
        p.writeShort(2);
        p.writeLong(0);
        p.writeLong(7);
        string(p, "Alex");
        p.writeInt(37);
        p.writeByte(1);
        p.writeByte(0b11);
        string(p, "\u00a7aAlex");
        p.writeLong(0);
        p.writeLong(8);
        string(p, "Bo");
        p.writeInt(0);
        p.writeByte(0);
        p.writeByte(0);
        string(p, "top");
        string(p, "");
        p.writeShort(1);
        string(p, "o");
        string(p, "Obj");
        p.writeByte(0);
        p.writeShort(2);
        string(p, "Bo");
        p.writeInt(-3);
        string(p, "Cy");
        p.writeInt(5);
        p.writeShort(1);
        p.writeByte(1);
        string(p, "o");
        p.writeShort(1);
        string(p, "red");
        string(p, "[R] ");
        string(p, "");
        string(p, "never");
        p.writeByte(12);
        p.writeShort(1);
        string(p, "Bo");

        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeInt(part.size());
        w.write(part.toByteArray());
        w.writeByte(1);
        string(w, "Wither");
        w.writeFloat(0.5f);
        w.writeInt(90);
        string(w, "T");
        string(w, "S");
        for (int v : new int[] {50, 10, 70, 20}) w.writeInt(v);
        w.writeByte(1);
        string(w, "bar");
        w.writeInt(40);
        return new DataInputStream(new ByteArrayInputStream(b.toByteArray()));
    }

    private static void string(DataOutputStream w, String s) throws IOException {
        byte[] b = s.getBytes(StandardCharsets.UTF_8);
        w.writeInt(b.length);
        w.write(b);
    }

    /** Reads the lists back through the vanilla classes, the way the HUD mods do. */
    private static void checkLists(Object mc, Object player, Object handler) {
        Object gui = R.get(mc, "field_71456_v");
        Object tab = R.call(gui, "func_175181_h"); // getTabList
        java.util.Set<String> names = new java.util.TreeSet<>();
        for (Object info : (java.util.Collection<?>) R.call(handler, "func_175106_d")) { // getPlayerInfoMap
            names.add((String) R.call(tab, "func_175243_a", info)); // getPlayerName
        }
        check(names.equals(new java.util.TreeSet<>(java.util.Arrays.asList("\u00a7aAlex\u00a7r", "[R] Bo"))), "tab list names " + names);
        Object local = R.call(handler, "func_175102_a", R.call(player, "func_110124_au")); // getPlayerInfo
        check("Alex".equals(R.call(R.call(local, "func_178845_a"), "getName")) && 37 == (Integer) R.call(local, "func_178853_c"), "local entry " + R.call(R.call(local, "func_178845_a"), "getName")); // getGameProfile, getResponseTime
        check("top".equals(R.call(R.get(tab, "field_175256_i"), "func_150260_c")), "tab header"); // header, getUnformattedText

        Object board = R.call(R.get(mc, "field_71441_e"), "func_96441_U"); // getScoreboard
        Object objective = R.call(board, "func_96539_a", 1); // getObjectiveInDisplaySlot
        check(objective != null && "Obj".equals(R.call(objective, "func_96678_d")), "sidebar objective"); // getDisplayName
        StringBuilder rows = new StringBuilder();
        for (Object score : (java.util.Collection<?>) R.call(board, "func_96534_i", objective)) { // getSortedScores
            rows.append(R.call(score, "func_96653_e")).append('=').append(R.call(score, "func_96652_c")).append(' ');
        }
        check("Bo=-3 Cy=5 ".equals(rows.toString()), "sidebar scores " + rows);
        Object team = R.call(board, "func_96509_i", "Bo"); // getPlayersTeam
        check(team != null && "[R] ".equals(R.call(team, "func_96668_e")), "team prefix"); // getColorPrefix

        Class<?> boss = R.cls("net.minecraft.entity.boss.BossStatus");
        check("Wither".equals(R.get(boss, "field_82827_c")) && (Float) R.get(boss, "field_82828_a") == 0.5f && (Integer) R.get(boss, "field_82826_b") == 90, "boss bar");
        check("T".equals(R.get(gui, "field_175201_x")) && "S".equals(R.get(gui, "field_175200_y")) && (Integer) R.get(gui, "field_175195_w") == 50 && (Integer) R.get(gui, "field_175193_B") == 20, "title");
        check("bar".equals(R.get(gui, "field_73838_g")) && (Integer) R.get(gui, "field_73845_h") == 40 && Boolean.FALSE.equals(R.get(gui, "field_73844_j")), "action bar");
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
