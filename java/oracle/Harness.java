import com.mojang.authlib.GameProfile;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.lang.reflect.Field;
import java.lang.reflect.Proxy;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import net.minecraft.flat.Bootstrap;
import net.minecraft.flat.Block;
import net.minecraft.flat.Blocks;
import net.minecraft.flat.BlockState;
import net.minecraft.flat.PlayerEntity;
import net.minecraft.flat.BlockPos;
import net.minecraft.flat.Profiler;
import net.minecraft.flat.World;
import net.minecraft.flat.WorldData;
import net.minecraft.flat.WorldGeneratorType;
import net.minecraft.flat.WorldSettings;
import net.minecraft.flat.ChunkSource;
import net.minecraft.flat.WorldChunk;
import net.minecraft.flat.Dimension;
import net.minecraft.flat.ProtoChunk;
import net.minecraft.flat.EmptyWorldStorage;

/**
 * Ground-truth oracle: drives the real, remapped 1.8.9 player movement code.
 *
 * stdin commands (one per line):
 *   floor <block>                 every block with y <= 63 becomes <block>
 *   block <x> <y> <z> <block>     extra block; must come before "start"
 *   start <x> <y> <z> <yaw>           spawn the player and build the world
 *   tick <n> <fwd> <strafe> <jump> <sprint> <fly speed, 0 = not flying> <sneak> <using item>
 *                                 a flying player also gets the vertical keys of EntityPlayerSP.onLivingUpdate
 *   state <x> <y> <z> <id> <meta>  like block, any block state of 1.8.9; prints ok, or bad for a meta the block rejects
 *   other <x> <y> <z>             spawn a still second player; must come after "start"
 *   props <id>                    liquid, solid, blocksMovement (0/1) and slipperiness float bits of a block
 *   boxes <x0> <y0> <z0> <x1> <y1> <z1>  World.getCollisions of the player in the box, one line of six doubles each, then "end"
 *   mine <block id>               hardness, empty-hand mining speed on the ground and in the air
 *                                 (float bits in hex), toolNotRequired; needs "start"
 *   bidi <UTF-8 hex>              FontRenderer.bidiReorder of the text: UTF-8 hex of the result
 *   signjson <UTF-8 hex>          the JSON that C12PacketUpdateSign writes for a sign line of that text: UTF-8 hex
 * stdout: one line per tick: x y z vx vy vz onGround, doubles as 16 hex digits of raw bits.
 */
public class Harness {
    static final Map<String, Block> BLOCKS = new HashMap<>();
    static final Map<Long, ProtoChunk> PROTO = new HashMap<>();
    static final Map<Long, WorldChunk> CHUNKS = new HashMap<>();
    static final Map<Long, BlockState> EXTRA = new HashMap<>();
    static Block floor;
    static World world;

    static String fromHex(String hexText) {
        byte[] raw = new byte[hexText.length() / 2];
        for (int i = 0; i < raw.length; i++) {
            raw[i] = (byte) Integer.parseInt(hexText.substring(2 * i, 2 * i + 2), 16);
        }
        return new String(raw, java.nio.charset.StandardCharsets.UTF_8);
    }

    static String toHex(String text) {
        StringBuilder sb = new StringBuilder();
        for (byte b : text.getBytes(java.nio.charset.StandardCharsets.UTF_8)) {
            sb.append(String.format("%02x", b));
        }
        return sb.toString();
    }

    /** `FontRenderer.bidiReorder`, UTF-8 hex in and out. */
    static String bidiReorder(String hexText) throws Exception {
        String text = fromHex(hexText);
        String out = text;
        if (com.ibm.icu.text.Bidi.requiresBidi(text.toCharArray(), 0, text.length())) {
            com.ibm.icu.text.Bidi bidi = new com.ibm.icu.text.Bidi(new com.ibm.icu.text.ArabicShaping(8).shape(text), 127);
            bidi.setReorderingMode(0);
            out = bidi.writeReordered(2);
        }
        return toHex(out);
    }

    /** One line of `C12PacketUpdateSign.writePacketData`: `IChatComponent.Serializer.componentToJson(new ChatComponentText(line))`. */
    static String signLine(String hexText) {
        return toHex(net.minecraft.flat.Text.Serializer.toJson(new net.minecraft.flat.LiteralText(fromHex(hexText))));
    }

    static long key(int x, int z) {
        return ((long) x << 32) ^ (z & 0xffffffffL);
    }

    static long blockKey(int x, int y, int z) {
        return ((long) x & 0x3ffffff) << 38 | ((long) z & 0x3ffffff) << 12 | (y & 0xfff);
    }

    static WorldChunk chunk(int cx, int cz) {
        return CHUNKS.computeIfAbsent(key(cx, cz), k -> {
            ProtoChunk p = new ProtoChunk();
            for (int x = 0; x < 16; x++) {
                for (int z = 0; z < 16; z++) {
                    for (int y = 0; y < 256; y++) {
                        BlockState b = EXTRA.get(blockKey(cx * 16 + x, y, cz * 16 + z));
                        if (b == null && y <= 63) {
                            b = floor.defaultState();
                        }
                        if (b != null && b.getBlock() != Blocks.AIR) {
                            p.setBlockState(x, y, z, b);
                        }
                    }
                }
            }
            return new WorldChunk(world, p, cx, cz);
        });
    }

    static World newWorld() {
        ChunkSource source = (ChunkSource) Proxy.newProxyInstance(
            Harness.class.getClassLoader(), new Class<?>[] {ChunkSource.class}, (proxy, m, args) -> {
                switch (m.getName()) {
                    case "hasChunk":
                        return true;
                    case "getChunk":
                        if (args[0] instanceof BlockPos) {
                            BlockPos p = (BlockPos) args[0];
                            return chunk(p.getX() >> 4, p.getZ() >> 4);
                        }
                        return chunk((Integer) args[0], (Integer) args[1]);
                    case "toString":
                        return "proxy";
                    default:
                        return m.getReturnType() == boolean.class ? (Object) false : null;
                }
            });
        EmptyWorldStorage storage = new EmptyWorldStorage();
        return new World(storage, new WorldData(new WorldSettings(0L, WorldSettings.GameMode.SURVIVAL, false, false, WorldGeneratorType.DEFAULT), "oracle"), Dimension.fromId(0), new Profiler(), false) {
            {
                chunkSource = source;
            }

            @Override
            protected ChunkSource createChunkCache() {
                return source;
            }

            @Override
            protected int getChunkViewDistance() {
                return 8;
            }
        };
    }

    static String hex(double d) {
        return String.format("%016x", Double.doubleToRawLongBits(d));
    }

    static Field field(Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }

    public static void main(String[] args) throws Exception {
        Bootstrap.init();
        floor = Blocks.STONE;
        BLOCKS.put("air", Blocks.AIR);
        BLOCKS.put("stone", Blocks.STONE);
        BLOCKS.put("ice", Blocks.ICE);
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
        PlayerEntity player = null;
        Field jumping = field(net.minecraft.flat.LivingEntity.class, "jumping");
        for (String line; (line = in.readLine()) != null; ) {
            String[] t = line.trim().split("\\s+");
            switch (t[0]) {
                case "floor":
                    floor = BLOCKS.get(t[1]);
                    break;
                case "block":
                    EXTRA.put(blockKey(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])), BLOCKS.get(t[4]).defaultState());
                    break;
                case "start":
                    world = newWorld();
                    player = new PlayerEntity(world, new GameProfile(UUID.randomUUID(), "oracle")) {
                        @Override
                        public boolean isSpectator() {
                            return false;
                        }
                    };
                    player.setPositionAndAngles(Double.parseDouble(t[1]), Double.parseDouble(t[2]), Double.parseDouble(t[3]), Float.parseFloat(t[4]), 0f);
                    break;
                case "other": {
                    PlayerEntity other = new PlayerEntity(world, new GameProfile(UUID.randomUUID(), "other")) {
                        @Override
                        public boolean isSpectator() {
                            return false;
                        }
                    };
                    other.setPositionAndAngles(Double.parseDouble(t[1]), Double.parseDouble(t[2]), Double.parseDouble(t[3]), 0f, 0f);
                    world.addEntity(other);
                    break;
                }
                case "state":
                    try {
                        EXTRA.put(blockKey(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])),
                            Block.byId(Integer.parseInt(t[4])).getStateFromMetadata(Integer.parseInt(t[5])));
                        System.out.println("ok");
                    } catch (RuntimeException e) {
                        System.out.println("bad");
                    }
                    break;
                case "props": {
                    Block b = Block.byId(Integer.parseInt(t[1]));
                    net.minecraft.flat.Material m = b.getMaterial();
                    System.out.println((m.isLiquid() ? 1 : 0) + " " + (m.isSolid() ? 1 : 0) + " " + (m.blocksMovement() ? 1 : 0) + " "
                        + Integer.toHexString(Float.floatToRawIntBits(b.slipperiness)));
                    break;
                }
                case "boxes": {
                    net.minecraft.flat.net_minecraft_util_math_Box area = new net.minecraft.flat.net_minecraft_util_math_Box(
                        Double.parseDouble(t[1]), Double.parseDouble(t[2]), Double.parseDouble(t[3]),
                        Double.parseDouble(t[4]), Double.parseDouble(t[5]), Double.parseDouble(t[6]));
                    for (net.minecraft.flat.net_minecraft_util_math_Box b : world.getCollisions(player, area)) {
                        System.out.println(hex(b.minX) + " " + hex(b.minY) + " " + hex(b.minZ) + " " + hex(b.maxX) + " " + hex(b.maxY) + " " + hex(b.maxZ));
                    }
                    System.out.println("end");
                    break;
                }
                case "tick":
                    int n = Integer.parseInt(t[1]);
                    float fwd = Float.parseFloat(t[2]);
                    float strafe = Float.parseFloat(t[3]);
                    boolean jump = t[4].equals("1");
                    boolean sprint = t[5].equals("1");
                    float fly = Float.parseFloat(t[6]);
                    boolean sneak = t[7].equals("1");
                    boolean using = t[8].equals("1");
                    // MovementInputFromOptions (sneak 0.3), then EntityPlayerSP.onLivingUpdate (using item 0.2)
                    if (sneak) {
                        fwd *= 0.3f;
                        strafe *= 0.3f;
                    }
                    if (using) {
                        fwd *= 0.2f;
                        strafe *= 0.2f;
                    }
                    player.setSneaking(sneak);
                    player.abilities.flying = fly > 0f;
                    if (fly > 0f) {
                        player.abilities.setFlySpeed(fly);
                    }
                    for (int i = 0; i < n; i++) {
                        player.forwardSpeed = fwd;
                        player.sidewaysSpeed = strafe;
                        jumping.setBoolean(player, jump);
                        if (player.isSprinting() != sprint) {
                            player.setSprinting(sprint);
                        }
                        if (fly > 0f) {
                            if (sneak) {
                                player.velocityY -= (double) (fly * 3.0f);
                            }
                            if (jump) {
                                player.velocityY += (double) (fly * 3.0f);
                            }
                        }
                        // Entity.onEntityUpdate runs handleWaterMovement before onLivingUpdate.
                        player.checkWaterCollisions();
                        player.mobTick();
                        System.out.println(hex(player.x) + " " + hex(player.y) + " " + hex(player.z) + " "
                            + hex(player.velocityX) + " " + hex(player.velocityY) + " " + hex(player.velocityZ) + " "
                            + (player.onGround ? 1 : 0));
                    }
                    break;
                case "mine": {
                    Block b = Block.byId(Integer.parseInt(t[1]));
                    BlockPos at = new BlockPos(0, 10, 0);
                    player.onGround = true;
                    float grounded = b.getMiningSpeed(player, world, at);
                    player.onGround = false;
                    float airborne = b.getMiningSpeed(player, world, at);
                    System.out.println(Integer.toHexString(Float.floatToRawIntBits(b.getMiningTime(world, at))) + " "
                        + Integer.toHexString(Float.floatToRawIntBits(grounded)) + " "
                        + Integer.toHexString(Float.floatToRawIntBits(airborne)) + " "
                        + (b.getMaterial().isToolNotRequired() ? 1 : 0));
                    break;
                }
                case "bidi":
                    System.out.println(bidiReorder(t.length > 1 ? t[1] : ""));
                    break;
                case "signjson":
                    System.out.println(signLine(t.length > 1 ? t[1] : ""));
                    break;
                default:
                    throw new IllegalArgumentException(line);
            }
        }
    }
}
