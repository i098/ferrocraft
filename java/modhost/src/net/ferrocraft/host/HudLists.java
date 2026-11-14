package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * The lists half of the stand-in state: puts the engine's tab list, scoreboard, boss bar, title and action bar where the
 * vanilla classes keep them, so HUD mods read the same names, scores and texts that Rust draws.
 *
 * <p>Message, after the tag 'L' (see hud_lists.rs for the layout): u32 length and the list part (players, header, footer,
 * objectives with their scores, display slots, teams), then the timed part (boss bar, title, action bar). The list part
 * is skipped when it equals the last one. The tab list goes into the player-info map that {@link Mirror} made, the
 * scoreboard replaces {@code World.worldScoreboard}, and the other values are set in the fields of {@code BossStatus} and
 * {@code GuiIngame}. This must run after the mirror message of the same frame, which rewrites the local player's entry.
 */
final class HudLists {
    private static final int MAX_TEXT = 1 << 16;
    private static final int MAX_LISTS = 1 << 22;

    private final Object world;
    private final Object gui;
    private final Map<Object, Object> players;
    private final Object profile;
    private final UUID uuid;
    private final Object listItem = R.alloc("net.minecraft.network.play.server.S38PacketPlayerListItem");
    private final Class<?> gameType = R.cls("net.minecraft.world.WorldSettings$GameType");
    private byte[] last = new byte[0];
    private String localName;
    private String localDisplay;
    private int localMode;

    @SuppressWarnings("unchecked")
    HudLists(Object mc, Object player) {
        this.world = R.get(mc, "field_71441_e");
        this.gui = R.get(mc, "field_71456_v");
        this.profile = R.get(player, "field_146106_i");
        this.uuid = (UUID) R.call(profile, "getId");
        // NetHandlerPlayClient.playerInfoMap is its only Map: the one that Mirror filled.
        Object handler = R.get(player, "field_71174_a");
        Map<Object, Object> found = null;
        for (Class<?> k = handler.getClass(); k != null && found == null; k = k.getSuperclass()) {
            for (Field f : k.getDeclaredFields()) {
                if (f.getType() == Map.class && !java.lang.reflect.Modifier.isStatic(f.getModifiers())) {
                    found = (Map<Object, Object>) R.get(handler, f.getName());
                    break;
                }
            }
        }
        if (found == null) throw new IllegalStateException("no player info map on " + handler.getClass().getName());
        this.players = found;
    }

    /** Reads one message after its tag and applies it. */
    void apply(DataInputStream in) throws IOException {
        int len = in.readInt();
        if (len < 0 || len > MAX_LISTS) throw new IOException(len + " bytes of lists");
        byte[] part = new byte[len];
        in.readFully(part);
        if (!Arrays.equals(part, last)) {
            last = part;
            try {
                lists(new DataInputStream(new ByteArrayInputStream(part)));
            } catch (RuntimeException e) {
                // A list that a vanilla class refuses must not stop the host.
                e.printStackTrace();
            }
        }
        patchLocal();

        boss(in);
        title(in);
    }

    /** BossStatus is static; statusBarTime 0 hides the bar. */
    private void boss(DataInputStream in) throws IOException {
        Class<?> boss = R.cls("net.minecraft.entity.boss.BossStatus");
        if (in.readUnsignedByte() != 0) {
            R.set(boss, "field_82827_c", string(in)); // bossName
            R.set(boss, "field_82828_a", in.readFloat()); // healthScale
            R.set(boss, "field_82826_b", in.readInt()); // statusBarTime
        } else {
            R.set(boss, "field_82826_b", 0);
        }
    }

    private void title(DataInputStream in) throws IOException {
        R.set(gui, "field_175201_x", string(in)); // displayedTitle
        R.set(gui, "field_175200_y", string(in)); // displayedSubTitle
        R.set(gui, "field_175195_w", in.readInt()); // titlesTimer
        R.set(gui, "field_175199_z", in.readInt()); // titleFadeIn
        R.set(gui, "field_175192_A", in.readInt()); // titleDisplayTime
        R.set(gui, "field_175193_B", in.readInt()); // titleFadeOut
        if (in.readUnsignedByte() != 0) {
            R.set(gui, "field_73838_g", string(in)); // recordPlaying
            R.set(gui, "field_73845_h", in.readInt()); // recordPlayingUpFor
            R.set(gui, "field_73844_j", false); // recordIsPlaying: an action bar, not a music disc
        } else {
            R.set(gui, "field_73845_h", 0);
        }
    }

    private void lists(DataInputStream in) throws IOException {
        players(in);
        Object board = R.neu("net.minecraft.scoreboard.Scoreboard");
        Map<String, Object> objectives = objectives(in, board);
        slots(in, board, objectives);
        teams(in, board);
        R.set(world, "field_96442_D", board); // worldScoreboard
    }

    private void players(DataInputStream in) throws IOException {
        players.keySet().retainAll(java.util.Collections.singleton(uuid));
        localName = null;
        int n = in.readUnsignedShort();
        for (int i = 0; i < n; i++) {
            UUID id = new UUID(in.readLong(), in.readLong());
            String name = string(in);
            int ping = in.readInt();
            int mode = in.readUnsignedByte();
            int flags = in.readUnsignedByte();
            String display = (flags & 2) != 0 ? string(in) : null;
            if ((flags & 1) != 0) {
                localName = name;
                localDisplay = display;
                localMode = mode;
                continue;
            }
            Object remote = R.neu("com.mojang.authlib.GameProfile", id, name);
            Object data = R.neu("net.minecraft.network.play.server.S38PacketPlayerListItem$AddPlayerData", listItem, remote, ping, gameType(mode), component(display));
            players.put(id, R.neu("net.minecraft.client.network.NetworkPlayerInfo", data));
        }
        Object tab = R.call(gui, "func_175181_h"); // getTabList
        R.call(tab, "func_175244_b", component(string(in))); // setHeader
        R.call(tab, "func_175248_a", component(string(in))); // setFooter
    }

    private Map<String, Object> objectives(DataInputStream in, Object board) throws IOException {
        Map<String, Object> objectives = new HashMap<>();
        Object dummy = R.get(R.cls("net.minecraft.scoreboard.IScoreObjectiveCriteria"), "field_96641_b");
        Object health = R.get(R.cls("net.minecraft.scoreboard.IScoreObjectiveCriteria"), "field_96638_f");
        int n = in.readUnsignedShort();
        for (int i = 0; i < n; i++) {
            String name = string(in);
            String display = string(in);
            boolean hearts = in.readUnsignedByte() != 0;
            int scores = in.readUnsignedShort();
            Object objective = R.call(board, "func_96535_a", name, hearts ? health : dummy); // addScoreObjective
            R.call(objective, "func_96681_a", display); // setDisplayName
            objectives.put(name, objective);
            for (int s = 0; s < scores; s++) {
                String player = string(in);
                Object score = R.call(board, "func_96529_a", player, objective); // getValueFromObjective
                R.call(score, "func_96647_c", in.readInt()); // setScorePoints
            }
        }
        return objectives;
    }

    private static void slots(DataInputStream in, Object board, Map<String, Object> objectives) throws IOException {
        int n = in.readUnsignedShort();
        for (int i = 0; i < n; i++) {
            int slot = in.readUnsignedByte();
            R.call(board, "func_96530_a", slot, objectives.get(string(in))); // setObjectiveInDisplaySlot
        }
    }

    private static void teams(DataInputStream in, Object board) throws IOException {
        int n = in.readUnsignedShort();
        for (int i = 0; i < n; i++) {
            String name = string(in);
            Object team = R.call(board, "func_96527_f", name); // createTeam
            R.call(team, "func_96666_b", string(in)); // setNamePrefix
            R.call(team, "func_96662_c", string(in)); // setNameSuffix
            Object visible = R.call(R.cls("net.minecraft.scoreboard.Team$EnumVisible"), "func_178824_a", string(in)); // getByName
            if (visible != null) R.call(team, "func_178772_a", visible); // setNameTagVisibility
            int color = in.readByte();
            Object format = color < 0 ? null : R.call(R.cls("net.minecraft.util.EnumChatFormatting"), "func_175744_a", color); // getByColorIndex
            if (format != null) R.call(team, "func_178774_a", format); // setChatFormat
            int members = in.readUnsignedShort();
            for (int m = 0; m < members; m++) R.call(board, "func_151392_a", string(in), name); // addPlayerToTeam
        }
    }

    /**
     * Mirror rewrites the local player's entry each frame with the stand-in name, the survival mode and no display name:
     * gives it the engine's own entry.
     */
    private void patchLocal() {
        Object info = players.get(uuid);
        if (info == null || localName == null) return;
        R.set(profile, "name", localName);
        R.call(info, "func_178859_a", component(localDisplay)); // setDisplayName
        R.call(info, "func_178839_a", gameType(localMode)); // setGameType
    }

    private Object gameType(int id) {
        return R.call(gameType, "func_77146_a", id); // getByID
    }

    /** A chat component with the text's format codes; null for no text. */
    private static Object component(String text) {
        return text == null || text.isEmpty() ? null : R.neu("net.minecraft.util.ChatComponentText", text);
    }

    private static String string(DataInputStream in) throws IOException {
        int len = in.readInt();
        if (len < 0 || len > MAX_TEXT) throw new IOException(len + " byte string");
        byte[] b = new byte[len];
        in.readFully(b);
        return new String(b, StandardCharsets.UTF_8);
    }
}
