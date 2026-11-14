package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.IOException;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * World and entity events for the mods. Rust says what happened, and this posts the Forge event that the same
 * change posts on the real client: the FML connect and disconnect events, {@code WorldEvent.Load} and
 * {@code Unload}, {@code EntityJoinWorldEvent} and {@code AttackEntityEvent}. Nothing goes back to Rust.
 *
 * <p>Message, after the tag 'E': u8 code, then ('c' connect, 'd' disconnect, 'l' world load, 'u' world unload: no
 * payload) ('j' join: i32 entity id, u8 kind, i32 type, i64 uuid high, i64 uuid low, three f64 position; kind 0 local
 * player, 1 other player, 2 mob, 3 object, 4 experience orb, 5 painting) ('a' attack: i32 entity id).
 *
 * <p>The entities are stand-ins: the real vanilla entity class, built without its constructor (it reads the world),
 * with the id and the position Rust knows. Handlers see the id, the class and the position; other fields are empty.
 * The network manager of the FML events has no channel: the host has no connection.
 */
final class Events {
    /** Spawn object ids of the 1.8.9 SpawnObject packet, as vanilla's {@code NetHandlerPlayClient.handleSpawnObject} reads them. */
    private static final Map<Integer, String> OBJECTS = new HashMap<>();

    static {
        String item = "net.minecraft.entity.item.";
        String proj = "net.minecraft.entity.projectile.";
        OBJECTS.put(1, item + "EntityBoat");
        OBJECTS.put(2, item + "EntityItem");
        OBJECTS.put(50, item + "EntityTNTPrimed");
        OBJECTS.put(51, item + "EntityEnderCrystal");
        OBJECTS.put(60, proj + "EntityArrow");
        OBJECTS.put(61, proj + "EntitySnowball");
        OBJECTS.put(62, proj + "EntityEgg");
        OBJECTS.put(63, proj + "EntityLargeFireball");
        OBJECTS.put(64, proj + "EntitySmallFireball");
        OBJECTS.put(65, proj + "EntityEnderPearl");
        OBJECTS.put(66, proj + "EntityWitherSkull");
        OBJECTS.put(70, item + "EntityFallingBlock");
        OBJECTS.put(71, "net.minecraft.entity.item.EntityItemFrame");
        OBJECTS.put(72, item + "EntityEnderEye");
        OBJECTS.put(73, proj + "EntityPotion");
        OBJECTS.put(75, proj + "EntityExpBottle");
        OBJECTS.put(76, item + "EntityFireworkRocket");
        OBJECTS.put(77, "net.minecraft.entity.EntityLeashKnot");
        OBJECTS.put(78, item + "EntityArmorStand");
        OBJECTS.put(90, proj + "EntityFishHook");
    }

    private final Object mc;
    private final Object forgeBus = Glue.forgeBus();
    private final Object fmlBus = Glue.fmlBus();
    /** The connection object of the FML events: it holds no channel, because Rust owns the socket. */
    private final Object manager = R.neu("net.minecraft.network.NetworkManager", R.get(R.cls("net.minecraft.network.EnumPacketDirection"), "CLIENTBOUND"));
    /** The entities that joined the world, by id, until it unloads. */
    private final Map<Integer, Object> entities = new HashMap<>();

    Events(Object mc) {
        this.mc = mc;
    }

    void apply(DataInputStream in) throws IOException {
        int code = in.readUnsignedByte();
        switch (code) {
            case 'c':
                post(fmlBus, "net.minecraftforge.fml.common.network.FMLNetworkEvent$ClientConnectedToServerEvent", manager, "VANILLA");
                break;
            case 'd':
                post(fmlBus, "net.minecraftforge.fml.common.network.FMLNetworkEvent$ClientDisconnectionFromServerEvent", manager);
                break;
            case 'l':
                post(forgeBus, "net.minecraftforge.event.world.WorldEvent$Load", world());
                break;
            case 'u':
                post(forgeBus, "net.minecraftforge.event.world.WorldEvent$Unload", world());
                entities.clear();
                break;
            case 'j':
                join(in);
                break;
            case 'a':
                attack(in.readInt());
                break;
            case 'r':
                entities.remove(in.readInt());
                break;
            default:
                throw new IOException("unknown event code " + code);
        }
    }

    private Object world() {
        return R.get(mc, "field_71441_e");
    }

    private void join(DataInputStream in) throws IOException {
        int id = in.readInt();
        int kind = in.readUnsignedByte();
        int type = in.readInt();
        long hi = in.readLong();
        long lo = in.readLong();
        double x = in.readDouble();
        double y = in.readDouble();
        double z = in.readDouble();
        Object entity = null;
        try {
            entity = create(kind, type, hi, lo);
        } catch (RuntimeException e) {
            Host.log("no stand-in entity for kind " + kind + " type " + type + ": " + e);
        }
        if (entity == null) return;
        if (kind != 0) place(entity, id, x, y, z);
        entities.put(id, entity);
        post(forgeBus, "net.minecraftforge.event.entity.EntityJoinWorldEvent", entity, world());
    }

    private void place(Object entity, int id, double x, double y, double z) {
        R.set(entity, "field_145783_c", id); // entityId
        R.set(entity, "field_70170_p", world()); // worldObj
        // The watcher slots Entity's constructor registers, so a mod can read the name and flags. Forge's join handler reads an item's stack from slot 10.
        Object watcher = R.neu("net.minecraft.entity.DataWatcher", entity);
        R.call(watcher, "func_75682_a", 0, (byte) 0); // addObject
        R.call(watcher, "func_75682_a", 1, (short) 300);
        R.call(watcher, "func_75682_a", 3, (byte) 0);
        R.call(watcher, "func_75682_a", 2, "");
        R.call(watcher, "func_75682_a", 4, (byte) 0);
        if (entity.getClass().getName().equals("net.minecraft.entity.item.EntityItem")) R.call(watcher, "func_82709_a", 10, 5); // an empty ItemStack
        R.set(entity, "field_70180_af", watcher); // dataWatcher
        try {
            R.call(entity, "func_70107_b", x, y, z); // setPosition
        } catch (RuntimeException e) {
            R.set(entity, "field_70165_t", x); // posX
            R.set(entity, "field_70163_u", y);
            R.set(entity, "field_70161_v", z);
        }
    }

    private Object create(int kind, int type, long hi, long lo) {
        switch (kind) {
            case 0:
                return R.get(mc, "field_71439_g"); // thePlayer
            case 1:
                Object player = R.alloc("net.minecraft.client.entity.EntityOtherPlayerMP");
                R.set(player, "field_146106_i", R.neu("com.mojang.authlib.GameProfile", new UUID(hi, lo), "")); // gameProfile
                return player;
            case 2:
                Class<?> c = (Class<?>) R.call(R.cls("net.minecraft.entity.EntityList"), "func_90035_a", type); // getClassFromID
                return c == null ? null : R.alloc(c.getName());
            case 3:
                String name = OBJECTS.get(type);
                return name == null ? null : R.alloc(name);
            case 4:
                return R.alloc("net.minecraft.entity.item.EntityXPOrb");
            case 5:
                return R.alloc("net.minecraft.entity.item.EntityPainting");
            default:
                return null;
        }
    }

    private void attack(int id) {
        Object target = entities.get(id);
        if (target == null) return;
        post(forgeBus, "net.minecraftforge.event.entity.player.AttackEntityEvent", R.get(mc, "field_71439_g"), target);
    }

    /** A failing mod handler is logged and must not stop the host. */
    private static void post(Object bus, String event, Object... args) {
        if (bus == null) return; // no event bus on this loader
        try {
            R.call(bus, "post", R.neu(event, args));
        } catch (RuntimeException e) {
            e.printStackTrace();
        }
    }
}
