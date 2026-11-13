package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * The HUD half of the stand-in state: puts the engine values that Rust sends in message 'M' (see mirror.rs) where
 * HUD mods read them: the debug FPS, the player's position, rotation, health, inventory, armor and effects, the
 * player-list entry that holds the ping, the server data, and the biome that the world returns at the player.
 *
 * <p>Message, after the tag 'M': i32 fps, i32 ping, f64 x, y, z, f32 yaw, f32 pitch, u8 biome, f32 health, u8 held,
 * u16 effect count and per effect i8 id, i8 amplifier, i32 ticks; 40 stacks (36 mainInventory, then 4 armorInventory
 * from the boots) as i16 item id (-1 empty), i8 count, i16 damage; u16 length and UTF-8 bytes of the server address.
 */
final class Mirror {
    private static final int MAIN = 36;
    private static final int SLOTS = 40;

    private final Object mc;
    private final Object player;
    private final UUID uuid;
    private final Object inventory;
    private final Map<Object, Object> players;
    private final Map<Object, Object> effects;
    private final Object listItem = R.alloc("net.minecraft.network.play.server.S38PacketPlayerListItem");
    private final Object gameProfile;
    private final Object survival = R.get(R.cls("net.minecraft.world.WorldSettings$GameType"), "SURVIVAL");
    private final byte[] biomes;
    private String server = "";

    @SuppressWarnings("unchecked")
    Mirror(Object mc, Object player) {
        this.mc = mc;
        this.player = player;
        this.gameProfile = R.get(player, "field_146106_i");
        this.uuid = (UUID) R.call(gameProfile, "getId");
        R.set(player, "field_96093_i", uuid); // entityUniqueID, which getUniqueID returns
        this.inventory = R.neu("net.minecraft.entity.player.InventoryPlayer", player);
        R.set(player, "field_71071_by", inventory);
        this.effects = (Map<Object, Object>) R.get(player, "field_70713_bf");

        // NetHandlerPlayClient.playerInfoMap is its only Map: getPlayerInfo(uuid).getResponseTime() reads the ping from it.
        Object handler = R.get(player, "field_71174_a");
        this.players = new HashMap<>();
        setByType(handler, Map.class, players);
        // The tab list overlay reads getNetworkManager().getIsencrypted() once it finds players.
        setByType(handler, R.cls("net.minecraft.network.NetworkManager"), R.alloc("net.minecraft.network.NetworkManager"));

        // Forge's World.getBiomeGenForCoords goes through the provider, which calls back into the world.
        Object world = R.get(mc, "field_71441_e");
        Object chunk = R.neu("net.minecraft.world.chunk.Chunk", world, 0, 0);
        this.biomes = new byte[256];
        setByType(chunk, byte[].class, biomes);
        Object worldProvider = R.alloc("net.minecraft.world.WorldProviderSurface");
        setByType(worldProvider, R.cls("net.minecraft.world.World"), world);
        setByType(world, R.cls("net.minecraft.world.WorldProvider"), worldProvider);
        // World.chunkProvider is the only IChunkProvider: one chunk whose biomes are all the biome at the player.
        Class<?> provider = R.cls("net.minecraft.world.chunk.IChunkProvider");
        Class<?> chunkClass = R.cls("net.minecraft.world.chunk.Chunk");
        setByType(world, provider, Proxy.newProxyInstance(provider.getClassLoader(), new Class<?>[] {provider}, (proxy, method, args) -> {
            if (method.getReturnType() == chunkClass) return chunk;
            return method.getReturnType() == boolean.class && method.getParameterTypes().length == 2 ? Boolean.TRUE : null;
        }));
    }

    /** Reads one message after its tag and applies it. */
    void apply(DataInputStream in) throws IOException {
        int fps = in.readInt();
        int ping = in.readInt();
        double x = in.readDouble();
        double y = in.readDouble();
        double z = in.readDouble();
        float yaw = in.readFloat();
        float pitch = in.readFloat();
        int biome = in.readUnsignedByte();
        float health = in.readFloat();
        int held = in.readUnsignedByte();

        R.set(R.cls("net.minecraft.client.Minecraft"), "field_71470_ab", fps); // debugFPS
        R.call(player, "func_70107_b", x, y, z); // setPosition
        for (String[] f : new String[][] {{"field_70169_q", "field_70142_S", "field_70165_t"}, {"field_70167_r", "field_70137_T", "field_70163_u"}, {"field_70166_s", "field_70136_U", "field_70161_v"}}) {
            Object pos = R.get(player, f[2]);
            R.set(player, f[0], pos); // prevPos
            R.set(player, f[1], pos); // lastTickPos
        }
        R.set(player, "field_70177_z", yaw);
        R.set(player, "field_70126_B", yaw);
        R.set(player, "field_70125_A", pitch);
        R.set(player, "field_70127_C", pitch);
        R.call(player, "func_70606_j", health); // setHealth
        Arrays.fill(biomes, (byte) biome);
        R.set(inventory, "field_70461_c", held); // currentItem

        effects.clear();
        int n = in.readUnsignedShort();
        for (int i = 0; i < n; i++) {
            int id = in.readByte();
            int amplifier = in.readByte();
            int ticks = in.readInt();
            effects.put(id, R.neu("net.minecraft.potion.PotionEffect", id, ticks, amplifier));
        }

        Object[] main = (Object[]) R.get(inventory, "field_70462_a");
        Object[] armor = (Object[]) R.get(inventory, "field_70460_b");
        for (int i = 0; i < SLOTS; i++) {
            Object stack = stack(in.readShort(), in.readByte(), in.readShort());
            if (i < MAIN) main[i] = stack;
            else armor[i - MAIN] = stack;
        }

        byte[] name = new byte[in.readUnsignedShort()];
        in.readFully(name);
        String address = new String(name, StandardCharsets.UTF_8);
        if (!address.equals(server)) {
            server = address;
            R.set(mc, "field_71422_O", address.isEmpty() ? null : R.neu("net.minecraft.client.multiplayer.ServerData", "Server", address, false)); // currentServerData
        }

        Object data = R.neu("net.minecraft.network.play.server.S38PacketPlayerListItem$AddPlayerData", listItem, gameProfile, ping, survival, null);
        players.put(uuid, R.neu("net.minecraft.client.network.NetworkPlayerInfo", data));
    }

    private static Object stack(int id, int count, int damage) {
        if (id < 0) return null;
        Object item = R.call(R.cls("net.minecraft.item.Item"), "func_150899_d", id); // getItemById
        return item == null ? null : R.neu("net.minecraft.item.ItemStack", item, count, damage);
    }

    /** Sets the only field of `target` whose type is `type`. */
    private static void setByType(Object target, Class<?> type, Object value) {
        for (Class<?> k = target.getClass(); k != null; k = k.getSuperclass()) {
            for (Field f : k.getDeclaredFields()) {
                if (f.getType() == type && !java.lang.reflect.Modifier.isStatic(f.getModifiers())) {
                    R.set(target, f.getName(), value);
                    return;
                }
            }
        }
        throw new IllegalStateException("no field of type " + type.getName() + " on " + target.getClass().getName());
    }
}
