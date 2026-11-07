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
 *   tick <n> <fwd> <strafe> <jump> <sprint>
 * stdout: one line per tick: x y z vx vy vz onGround, doubles as 16 hex digits of raw bits.
 */
public class Harness {
    static final Map<String, Block> BLOCKS = new HashMap<>();
    static final Map<Long, ProtoChunk> PROTO = new HashMap<>();
    static final Map<Long, WorldChunk> CHUNKS = new HashMap<>();
    static final Map<Long, Block> EXTRA = new HashMap<>();
    static Block floor;
    static World world;

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
                        Block b = EXTRA.get(blockKey(cx * 16 + x, y, cz * 16 + z));
                        if (b == null && y <= 63) {
                            b = floor;
                        }
                        if (b != null && b != Blocks.AIR) {
                            p.setBlockState(x, y, z, b.defaultState());
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
                    EXTRA.put(blockKey(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])), BLOCKS.get(t[4]));
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
                case "tick":
                    int n = Integer.parseInt(t[1]);
                    float fwd = Float.parseFloat(t[2]);
                    float strafe = Float.parseFloat(t[3]);
                    boolean jump = t[4].equals("1");
                    boolean sprint = t[5].equals("1");
                    for (int i = 0; i < n; i++) {
                        player.forwardSpeed = fwd;
                        player.sidewaysSpeed = strafe;
                        jumping.setBoolean(player, jump);
                        if (player.isSprinting() != sprint) {
                            player.setSprinting(sprint);
                        }
                        player.mobTick();
                        System.out.println(hex(player.x) + " " + hex(player.y) + " " + hex(player.z) + " "
                            + hex(player.velocityX) + " " + hex(player.velocityY) + " " + hex(player.velocityZ) + " "
                            + (player.onGround ? 1 : 0));
                    }
                    break;
                default:
                    throw new IllegalArgumentException(line);
            }
        }
    }
}
