import com.mojang.authlib.GameProfile;
import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.lang.reflect.Field;
import java.lang.reflect.Proxy;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import net.minecraft.flat.Bootstrap;
import net.minecraft.flat.Block;
import net.minecraft.flat.Blocks;
import net.minecraft.flat.BlockState;
import net.minecraft.flat.PlayerEntity;
import net.minecraft.flat.LocalClientPlayerEntity;
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
import net.minecraft.flat.SquidEntity;
import net.minecraft.flat.GuardianEntity;
import net.minecraft.flat.EnderDragonEntity;
import net.minecraft.flat.EmptyWorldStorage;
import net.minecraft.flat.MagmaCubeEntity;
import net.minecraft.flat.SlimeEntity;

/**
 * Ground-truth oracle: drives the real, remapped 1.8.9 player movement code.
 *
 * stdin commands (one per line):
 *   floor <block>                 every block with y <= 63 becomes <block>
 *   block <x> <y> <z> <block>     extra block; must come before "start"
 *   start <x> <y> <z> <yaw>           spawn the player and build the world
 *   tick <n> <fwd> <strafe> <jump> <sprint> <fly speed, 0 = not flying> <sneak> <using item>
 *                                 a flying player also gets the vertical keys of EntityPlayerSP.onLivingUpdate
 *   pushout                       each tick, the real LocalClientPlayerEntity.pushAwayFrom runs at the four corners of the box
 *   noclip                        the player moves with noClip set and onGround false each tick, as a spectator does
 *   state <x> <y> <z> <id> <meta>  like block, any block state of 1.8.9; prints ok, or bad for a meta the block rejects
 *   boots <level>                 wear leather boots with Depth Strider <level> (0 = none); needs "start"
 *   other <x> <y> <z>             spawn a still second player; must come after "start"
 *   props <id>                    liquid, solid, blocksMovement (0/1) and slipperiness float bits of a block
 *   boxes <x0> <y0> <z0> <x1> <y1> <z1>  World.getCollisions of the player in the box, one line of six doubles each, then "end"
 *   mine <block id>               hardness, empty-hand mining speed on the ground and in the air
 *                                 (float bits in hex), toolNotRequired; needs "start"
 *   minewith <block> <held item id|0> <efficiency> <haste amp|-1> <fatigue amp|-1> <aqua 0/1>
 *                                 mining speed on the ground and in the air (float bits in hex) with that tool and effects; needs "start"
 *   offset <x> <y> <z>            MathHelper.hashCode(BlockPos) (the position random the block offset uses), then
 *                                 name|offset type of tallgrass, deadbush, yellow_flower, red_flower, double_plant
 *   bidi <UTF-8 hex>              FontRenderer.bidiReorder of the text: UTF-8 hex of the result
 *   signjson <UTF-8 hex>          the JSON that C12PacketUpdateSign writes for a sign line of that text: UTF-8 hex
 *   achievements                  AchievementList: key|column|row|parent|challenge|item id|damage per line, then the display bounds
 *   stats                         Stats: G|key|formatter, M|item id|key, U|item id|key, A<m|c|u|b>|id|key per array slot, I|item id|registry name|translation key, E|entity key|kill key|killed-by key
 *   statfmt <kind> <ints...>      Stat.format of the formatter (number, time, distance, tenth): one line per int
 *   mip <levels> <side> <hex of side*side ARGB ints>  TextureUtil.generateMipmaps: one line per level 1.., hex ARGB ints
 *   eat <yaw> <pitch> <seed> <count> <item id>  PlayerEntity.onConsumeItem of that food: one line per ITEM_CRACK particle,
 *                                 "x y z vx vy vz" as decimal doubles; needs "start"
 *   rail <x> <y> <z> <offset|none>  MinecartEntity.snapPositionToRail (or ...WithOffset) of a minecart there: "null" or three doubles as
 *                                 16 hex digits each; needs "start"
 *   squish <magma 0/1> <ticks> <kick tick>  a slime or magma cube dropped from y 66 onto the floor, which gets a jump
 *                                 at the kick tick; per tick: onGround, stretch and lastStretch as float bits in hex; needs "start"
 *   squid <ticks> <water 0/1> <vx> <vy> <vz> <yaw> <status tick>  a client squid given that velocity at tick 0 and
 *                                 the status 19 at the status tick; per tick: see squidCommand; needs "start"
 *   body <swing tick, -1 = none> <ticks> <x z yaw>*  a remote player moved to each x z with that yaw, then ticked;
 *                                 first line: body yaw before the first tick; per tick: body yaw and last body yaw (float bits); needs "start"
 *   guardian <ticks> <water 0/1> <moving 0/1> <seed> <switch tick>  a client guardian; per tick: see guardianCommand;
 *                                 needs "start"
 *   dragon <ticks> <vx> <vy> <vz> <yaw0> <dyaw> <y0> <dy>  a client ender dragon; per tick: see dragonCommand; needs "start"
 * stdout: one line per tick: x y z vx vy vz onGround, doubles as 16 hex digits of raw bits.
 * Framing: the process serves many scripts. A script ends with a "%%run" line; per-script state is reset, the output
 * follows, then "%%done", or "%%error <hex stack trace>" if the script failed. The process runs until stdin closes.
 */
public class Harness {
    static final Map<String, Block> BLOCKS = new HashMap<>();
    static final Map<Long, ProtoChunk> PROTO = new HashMap<>();
    static final Map<Long, WorldChunk> CHUNKS = new HashMap<>();
    static final Map<Long, BlockState> EXTRA = new HashMap<>();
    static Block floor;
    static World world;
    /** `pushout`: run the real `LocalClientPlayerEntity.pushAwayFrom` at its four corners each tick. */
    static boolean pushOut;
    /** `noclip`: the player is a spectator body (`Entity.noClip`, never on the ground). */
    static boolean noClip;
    static LocalClientPlayerEntity pusher;

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

    /**
     * The real `TextureUtil.generateMipmaps`. Its class initializer needs OpenGL (a dynamic texture) and the
     * client jar ships without LWJGL, so the class is loaded with a replacement initializer that only creates
     * `MIPMAP_BUFFER`, the one static the method uses. Everything else is the vanilla bytecode.
     */
    static int[][] vanillaMipmaps(int levels, int side, int[] base) throws Exception {
        final String name = "net/minecraft/flat/TextureUtil";
        byte[] raw;
        try (InputStream in = Harness.class.getResourceAsStream("/" + name + ".class")) {
            java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
            byte[] chunk = new byte[8192];
            for (int n; (n = in.read(chunk)) > 0; ) {
                buf.write(chunk, 0, n);
            }
            raw = buf.toByteArray();
        }
        ClassWriter out = new ClassWriter(0);
        new ClassReader(raw).accept(new ClassVisitor(Opcodes.ASM9, out) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String desc, String sig, String[] ex) {
                if (!n.equals("<clinit>")) {
                    return super.visitMethod(access, n, desc, sig, ex);
                }
                MethodVisitor mv = super.visitMethod(access, n, desc, sig, ex);
                mv.visitCode();
                mv.visitInsn(Opcodes.ICONST_4);
                mv.visitIntInsn(Opcodes.NEWARRAY, Opcodes.T_INT);
                mv.visitFieldInsn(Opcodes.PUTSTATIC, name, "MIPMAP_BUFFER", "[I");
                mv.visitInsn(Opcodes.RETURN);
                mv.visitMaxs(1, 0);
                mv.visitEnd();
                return null;
            }
        }, 0);
        byte[] patched = out.toByteArray();
        Class<?> cls = new ClassLoader(Harness.class.getClassLoader()) {
            Class<?> define() {
                return defineClass(name.replace('/', '.'), patched, 0, patched.length);
            }
        }.define();
        java.lang.reflect.Method m = cls.getMethod("generateMipmaps", int.class, int.class, int[][].class);
        int[][] frame = new int[levels + 1][];
        frame[0] = base;
        return (int[][]) m.invoke(null, levels, side, frame);
    }

    /** The `mip` command: prints one line per mip level of the sprite in `t`. */
    static void mipLines(String[] t) throws Exception {
        int levels = Integer.parseInt(t[1]);
        int side = Integer.parseInt(t[2]);
        int[] base = new int[side * side];
        for (int i = 0; i < base.length; i++) {
            base[i] = (int) Long.parseLong(t[3].substring(8 * i, 8 * i + 8), 16);
        }
        int[][] chain = vanillaMipmaps(levels, side, base);
        for (int l = 1; l <= levels; l++) {
            StringBuilder sb = new StringBuilder();
            for (int v : chain[l]) {
                sb.append(String.format("%08x", v));
            }
            System.out.println(sb);
        }
    }

    /** Copies the alpha arithmetic of `renderShadowOnBlock` into `target`, with its inputs as arguments. */
    static class ShadowAlphaCopier extends MethodVisitor {
        final MethodVisitor target;
        boolean copying;
        boolean extracted;

        ShadowAlphaCopier(MethodVisitor target) {
            super(Opcodes.ASM9);
            this.target = target;
        }

        @Override
        public void visitVarInsn(int opcode, int var) {
            if (opcode == Opcodes.FLOAD && var == 9) copying = true;
            if (!copying) return;
            if (opcode == Opcodes.DSTORE && var == 19) {
                target.visitInsn(Opcodes.DRETURN);
                copying = false;
                extracted = true;
            } else if (opcode == Opcodes.FLOAD && var == 9) {
                target.visitVarInsn(Opcodes.FLOAD, 0);
            } else if (opcode == Opcodes.DLOAD && var == 4) {
                target.visitVarInsn(Opcodes.DLOAD, 1);
            } else if (opcode == Opcodes.DLOAD && var == 13) {
                target.visitInsn(Opcodes.DCONST_0);
            }
        }

        @Override
        public void visitInsn(int opcode) {
            if (copying) target.visitInsn(opcode);
        }

        @Override
        public void visitLdcInsn(Object value) {
            if (copying) target.visitLdcInsn(value);
        }

        @Override
        public void visitMethodInsn(int opcode, String owner, String name, String desc, boolean itf) {
            if (!copying) return;
            if (name.equals("getY")) target.visitVarInsn(Opcodes.ILOAD, 3);
            else if (name.equals("getBrightness")) target.visitVarInsn(Opcodes.FLOAD, 4);
            else if (!name.equals("getWorld")) throw new IllegalStateException(name);
        }
    }

    /** Runs the real shadow alpha bytecode, with the world brightness supplied as an argument. */
    static java.lang.reflect.Method shadowAlphaMethod;
    static double shadowAlpha(float alpha, double y, int blockY, float brightness) throws Exception {
        if (shadowAlphaMethod == null) {
            ClassWriter output = new ClassWriter(ClassWriter.COMPUTE_MAXS);
            output.visit(Opcodes.V1_8, Opcodes.ACC_PUBLIC, "ShadowAlpha", null, "java/lang/Object", null);
            MethodVisitor target = output.visitMethod(Opcodes.ACC_PUBLIC | Opcodes.ACC_STATIC,
                "eval", "(FDIF)D", null, null);
            target.visitCode();
            ShadowAlphaCopier copier = new ShadowAlphaCopier(target);
            try (InputStream input = Harness.class.getResourceAsStream("/net/minecraft/flat/EntityRenderer.class")) {
                new ClassReader(input).accept(new ClassVisitor(Opcodes.ASM9) {
                    @Override
                    public MethodVisitor visitMethod(int access, String name, String desc, String signature, String[] exceptions) {
                        return name.equals("renderShadowOnBlock") ? copier : null;
                    }
                }, 0);
            }
            if (!copier.extracted) throw new IllegalStateException("shadow alpha bytecode not found");
            target.visitMaxs(0, 0);
            target.visitEnd();
            output.visitEnd();
            byte[] bytes = output.toByteArray();
            Class<?> type = new ClassLoader(Harness.class.getClassLoader()) {
                Class<?> load() { return defineClass("ShadowAlpha", bytes, 0, bytes.length); }
            }.load();
            shadowAlphaMethod = type.getMethod("eval", float.class, double.class, int.class, float.class);
        }
        return (Double) shadowAlphaMethod.invoke(null, alpha, y, blockY, brightness);
    }

    /** One line per block id 0..255 of {@code layer}: the 1.8.9 render layer name. */
    static void layerLines() {
        for (int id = 0; id < 256; id++) {
            System.out.println(Block.byId(id).getRenderLayer());
        }
    }

    /** One line of {@code minewith}: the mining speed on the ground and in the air. */
    static void minewithCommand(PlayerEntity player, String[] t) {
        // minewith <block> <held item id, 0 = none> <efficiency> <haste amp, -1 = none>
        //          <fatigue amp, -1 = none> <aqua affinity 0/1>
        Block b = Block.byId(Integer.parseInt(t[1]));
        BlockPos at = new BlockPos(0, 10, 0);
        int item = Integer.parseInt(t[2]);
        net.minecraft.flat.ItemStack held = null;
        if (item != 0) {
            held = new net.minecraft.flat.ItemStack(net.minecraft.flat.Item.byId(item));
            int eff = Integer.parseInt(t[3]);
            if (eff > 0) {
                held.addEnchantment(net.minecraft.flat.Enchantment.EFFICIENCY, eff);
            }
        }
        player.inventory.items[player.inventory.selectedSlot] = held;
        net.minecraft.flat.ItemStack helmet = null;
        if (t[6].equals("1")) {
            helmet = new net.minecraft.flat.ItemStack(net.minecraft.flat.Items.LEATHER_HELMET);
            helmet.addEnchantment(net.minecraft.flat.Enchantment.AQUA_AFFINITY, 1);
        }
        player.setEquipment(3, helmet);
        player.clearStatusEffects();
        int haste = Integer.parseInt(t[4]);
        int fatigue = Integer.parseInt(t[5]);
        if (haste >= 0) {
            player.addStatusEffect(new net.minecraft.flat.StatusEffectInstance(net.minecraft.flat.StatusEffect.HASTE.id, 600, haste));
        }
        if (fatigue >= 0) {
            player.addStatusEffect(new net.minecraft.flat.StatusEffectInstance(net.minecraft.flat.StatusEffect.MINING_FATIGUE.id, 600, fatigue));
        }
        player.onGround = true;
        float grounded = b.getMiningSpeed(player, world, at);
        player.onGround = false;
        float airborne = b.getMiningSpeed(player, world, at);
        System.out.println(Integer.toHexString(Float.floatToRawIntBits(grounded)) + " "
            + Integer.toHexString(Float.floatToRawIntBits(airborne)));
    }

    /** `shadow <alpha> <y> <blockY> <light>`: the shadow alpha with the brightness table entry of `light`. */
    static void shadowCommand(String[] t) throws Exception {
        java.lang.reflect.Method init = Dimension.class.getDeclaredMethod("initBrightnessTable");
        init.setAccessible(true);
        init.invoke(world.dimension);
        System.out.println(shadowAlpha(Float.parseFloat(t[1]), Double.parseDouble(t[2]),
            Integer.parseInt(t[3]), world.dimension.getBrightnessTable()[Integer.parseInt(t[4])]));
    }

    /** `eat <yaw> <pitch> <seed> <count> <item id>`: the particles the real `onConsumeItem` adds for that food. */
    static void eatCommand(PlayerEntity player, String[] t) throws Exception {
        player.yaw = Float.parseFloat(t[1]);
        player.pitch = Float.parseFloat(t[2]);
        java.lang.reflect.Field random = net.minecraft.flat.Entity.class.getDeclaredField("random");
        random.setAccessible(true);
        ((java.util.Random) random.get(player)).setSeed(Long.parseLong(t[3]));
        world.addEventListener((net.minecraft.flat.WorldEventListener) Proxy.newProxyInstance(
            Harness.class.getClassLoader(), new Class<?>[] {net.minecraft.flat.WorldEventListener.class}, (proxy, m, args) -> {
                if (m.getName().equals("addParticle")) {
                    System.out.println(args[2] + " " + args[3] + " " + args[4] + " " + args[5] + " " + args[6] + " " + args[7]);
                }
                Class<?> r = m.getReturnType();
                return r == boolean.class ? (Object) false : r == int.class ? (Object) 0 : null;
            }));
        java.lang.reflect.Method m = PlayerEntity.class.getDeclaredMethod("onConsumeItem", net.minecraft.flat.ItemStack.class, int.class);
        m.setAccessible(true);
        m.invoke(player, new net.minecraft.flat.ItemStack(net.minecraft.flat.Item.byId(Integer.parseInt(t[5]))), Integer.parseInt(t[4]));
    }

    /** `SlimeEntity.tick`: the squish state after each tick of a real slime or magma cube. */
    static void squishCommand(String[] t) {
        SlimeEntity s = t[1].equals("1") ? new MagmaCubeEntity(world) : new SlimeEntity(world);
        s.setPosition(0.5, 66, 0.5);
        int kick = Integer.parseInt(t[3]);
        for (int i = 0; i < Integer.parseInt(t[2]); i++) {
            if (i == kick) {
                s.velocityY = 0.42;
            }
            s.tick();
            System.out.println((s.onGround ? 1 : 0) + " " + Integer.toHexString(Float.floatToRawIntBits(s.stretch))
                + " " + Integer.toHexString(Float.floatToRawIntBits(s.lastStretch)));
        }
    }

    /** `rail <x> <y> <z> <offset|none>`: where a real minecart snaps to the rail under that point. */
    static void railCommand(String[] t) {
        net.minecraft.flat.RideableMinecartEntity cart = new net.minecraft.flat.RideableMinecartEntity(world);
        double x = Double.parseDouble(t[1]);
        double y = Double.parseDouble(t[2]);
        double z = Double.parseDouble(t[3]);
        net.minecraft.flat.Vec3d v = t[4].equals("none") ? cart.snapPositionToRail(x, y, z)
            : cart.snapPositionToRailWithOffset(x, y, z, Double.parseDouble(t[4]));
        System.out.println(v == null ? "null" : hex(v.x) + " " + hex(v.y) + " " + hex(v.z));
    }

    private static String bits(float v) {
        return Integer.toHexString(Float.floatToRawIntBits(v));
    }

    private static String bits(double v) {
        return Long.toHexString(Double.doubleToRawLongBits(v));
    }

    /**
     * `SquidEntity.mobTick` as a client runs it: the server velocity is set once, the world is flagged as a
     * client one, and `water` forces the water state. First line: the entity id the constructor seeded its
     * tentacle speed from. Then per tick: x and z movement, yaw, the three velocities (double bits), then pitch,
     * roll, tentacle movement, tentacle rotation, body yaw, last body yaw and head yaw (float bits).
     */
    static void squidCommand(String[] t) throws Exception {
        Field client = net.minecraft.flat.World.class.getField("isClient");
        client.setAccessible(true);
        client.setBoolean(world, true);
        final boolean water = t[2].equals("1");
        SquidEntity s = new SquidEntity(world) {
            @Override
            public boolean checkWaterCollisions() {
                inWater = water;
                return water;
            }
        };
        s.setPositionAndAngles(0.5, 70, 0.5, Float.parseFloat(t[6]), 0f);
        s.bodyYaw = s.yaw;
        s.lastBodyYaw = s.yaw;
        s.headYaw = s.yaw;
        System.out.println(s.getNetworkId());
        for (int i = 0; i < Integer.parseInt(t[1]); i++) {
            if (i == 0) {
                s.velocityX = Double.parseDouble(t[3]);
                s.velocityY = Double.parseDouble(t[4]);
                s.velocityZ = Double.parseDouble(t[5]);
            }
            if (i == Integer.parseInt(t[7])) {
                s.doEvent((byte) 19);
            }
            s.tick();
            System.out.println(bits(s.x - s.lastX) + " " + bits(s.z - s.lastZ) + " " + bits(s.yaw) + " "
                + bits(s.velocityX) + " " + bits(s.velocityY) + " " + bits(s.velocityZ) + " "
                + bits(s.bodyPitch) + " " + bits(s.bodyRoll) + " " + bits(s.tentacleMovement) + " "
                + bits(s.tentacleRotation) + " " + bits(s.bodyYaw) + " " + bits(s.lastBodyYaw) + " " + bits(s.headYaw));
        }
    }

    /**
     * `LivingEntity.tick` -> `bodyMovement` for a `RemoteClientPlayerEntity`: each tick it gets a one-step
     * interpolation to the next position and yaw. Line 1: body yaw of the fresh entity (float bits). Then per
     * tick: body yaw and last body yaw (float bits).
     */
    static void bodyCommand(String[] t) throws Exception {
        Field client = net.minecraft.flat.World.class.getField("isClient");
        client.setAccessible(true);
        client.setBoolean(world, true);
        net.minecraft.flat.RemoteClientPlayerEntity p =
            new net.minecraft.flat.RemoteClientPlayerEntity(world, new GameProfile(UUID.randomUUID(), "body")) {
                @Override
                public boolean isSpectator() {
                    return false;
                }
            };
        p.setPositionAndAngles(Double.parseDouble(t[3]), 70, Double.parseDouble(t[4]), Float.parseFloat(t[5]), 0f);
        System.out.println(bits(p.bodyYaw));
        int swing = Integer.parseInt(t[1]);
        int n = Integer.parseInt(t[2]);
        for (int i = 0; i < n; i++) {
            p.lerpPositionAndAngles(Double.parseDouble(t[3 + 3 * i]), 70, Double.parseDouble(t[4 + 3 * i]),
                Float.parseFloat(t[5 + 3 * i]), 0f, 1, false);
            if (i == swing) {
                p.swingArm();
            }
            p.tick();
            System.out.println(bits(p.bodyYaw) + " " + bits(p.lastBodyYaw));
        }
    }

    /**
     * `GuardianEntity.mobTick` as a client runs it, with `isInWater` and `isMoving` given and the water
     * flipped at the switch tick; the entity's `random` is seeded after the constructor drew the tail phase.
     * Line 1: the tail phase. Then per tick: getTailAngle(0), getTailAngle(1), getSpikesExtension(0),
     * getSpikesExtension(1) as float bits.
     */
    static void guardianCommand(String[] t) throws Exception {
        Field client = net.minecraft.flat.World.class.getField("isClient");
        client.setAccessible(true);
        client.setBoolean(world, true);
        final boolean[] water = {t[2].equals("1")};
        final boolean moving = t[3].equals("1");
        final long seed = Long.parseLong(t[4]);
        GuardianEntity g = new GuardianEntity(world) {
            {
                random = new java.util.Random(seed);
            }

            @Override
            public boolean isInWater() {
                return water[0];
            }

            @Override
            public boolean isMoving() {
                return moving;
            }
        };
        g.setPositionAndAngles(0.5, 70, 0.5, 0f, 0f);
        System.out.println(bits(g.getTailAngle(0f)));
        for (int i = 0; i < Integer.parseInt(t[1]); i++) {
            if (i == Integer.parseInt(t[5])) {
                water[0] = !water[0];
            }
            g.mobTick();
            System.out.println(bits(g.getTailAngle(0f)) + " " + bits(g.getTailAngle(1f)) + " "
                + bits(g.getSpikesExtension(0f)) + " " + bits(g.getSpikesExtension(1f)));
        }
    }

    /**
     * `EnderDragonEntity.mobTick` as a client runs it: yaw and height are set before every tick to
     * yaw0 + dyaw * i and y0 + dy * sin(0.3 i), the velocity is given. Per tick: wingPosition, lastWingPosition
     * as float bits, then yaw and height of getSegmentProperties(k, 0.25) for k 0..23 as double bits.
     */
    static void dragonCommand(String[] t) throws Exception {
        Field client = net.minecraft.flat.World.class.getField("isClient");
        client.setAccessible(true);
        client.setBoolean(world, true);
        EnderDragonEntity d = new EnderDragonEntity(world);
        float yaw0 = Float.parseFloat(t[5]);
        float dyaw = Float.parseFloat(t[6]);
        double y0 = Double.parseDouble(t[7]);
        double dy = Double.parseDouble(t[8]);
        d.setPositionAndAngles(0.5, y0, 0.5, yaw0, 0f);
        for (int i = 0; i < Integer.parseInt(t[1]); i++) {
            d.yaw = yaw0 + dyaw * i;
            d.y = y0 + dy * Math.sin(0.3 * i);
            d.velocityX = Double.parseDouble(t[2]);
            d.velocityY = Double.parseDouble(t[3]);
            d.velocityZ = Double.parseDouble(t[4]);
            d.mobTick();
            StringBuilder line = new StringBuilder(bits(d.wingPosition) + " " + bits(d.lastWingPosition));
            for (int k = 0; k < 24; k++) {
                double[] p = d.getSegmentProperties(k, 0.25f);
                line.append(' ').append(bits(p[0])).append(' ').append(bits(p[1]));
            }
            System.out.println(line);
        }
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

    /**
     * `AchievementList.achievementList`: one line per achievement, `key|column|row|parent|challenge|id|damage`,
     * then `minColumn minRow maxColumn maxRow`. `Bootstrap` must have registered its items first.
     */
    static void achievementLines() {
        for (net.minecraft.flat.AchievementStat a : net.minecraft.flat.Achievements.ALL) {
            System.out.println(a.key + "|" + a.column + "|" + a.row + "|"
                + (a.parent == null ? "-" : a.parent.key) + "|" + a.isChallenge() + "|"
                + net.minecraft.flat.Item.getId(a.icon.getItem()) + "|" + a.icon.getMetadata());
        }
        System.out.println(net.minecraft.flat.Achievements.minColumn + " " + net.minecraft.flat.Achievements.minRow + " "
            + net.minecraft.flat.Achievements.maxColumn + " " + net.minecraft.flat.Achievements.maxRow);
    }

    static String formatterKind(Object f) {
        if (f == net.minecraft.flat.Stat.TIME_FORMATTER) {
            return "time";
        }
        if (f == net.minecraft.flat.Stat.DISTANCE_FORMATTER) {
            return "distance";
        }
        if (f == net.minecraft.flat.Stat.DIVIDE_BY_TEN_FORMATTER) {
            return "tenth";
        }
        if (f == net.minecraft.flat.Stat.NUMBER_FORMATTER) {
            return "number";
        }
        throw new IllegalStateException("unknown formatter");
    }

    /** `statfmt`: the real formatter of `kind` on each int. */
    static void statFormat(String[] t) {
        net.minecraft.flat.StatFormatter f;
        switch (t[1]) {
            case "time": f = net.minecraft.flat.Stat.TIME_FORMATTER; break;
            case "distance": f = net.minecraft.flat.Stat.DISTANCE_FORMATTER; break;
            case "tenth": f = net.minecraft.flat.Stat.DIVIDE_BY_TEN_FORMATTER; break;
            default: f = net.minecraft.flat.Stat.NUMBER_FORMATTER; break;
        }
        for (int i = 2; i < t.length; i++) {
            System.out.println(f.format(Integer.parseInt(t[i])));
        }
    }

    /** `registry name|translation key`: what stat keys and the stat screen print for an item. */
    static String itemNames(net.minecraft.flat.Item item) {
        Object id = net.minecraft.flat.Item.REGISTRY.getKey(item);
        return id.toString().replace(':', '.') + "|" + new net.minecraft.flat.ItemStack(item).getTranslationKey();
    }

    /** `stats`: the `Stats` tables, in registration order. `Bootstrap` must have registered its items first. */
    static void statLines() throws Exception {
        Field formatter = net.minecraft.flat.Stat.class.getDeclaredField("formatter");
        formatter.setAccessible(true);
        java.util.TreeMap<Integer, String> names = new java.util.TreeMap<>();
        for (net.minecraft.flat.Stat s : net.minecraft.flat.Stats.GENERAL) {
            System.out.println("G|" + s.key + "|" + formatterKind(formatter.get(s)));
        }
        String[] tags = {"M", "U"};
        java.util.List<java.util.List<net.minecraft.flat.ItemStat>> lists = java.util.Arrays.asList(
            net.minecraft.flat.Stats.MINED, net.minecraft.flat.Stats.USED);
        for (int l = 0; l < 2; l++) {
            for (net.minecraft.flat.ItemStat s : lists.get(l)) {
                int id = net.minecraft.flat.Item.getId(s.getItem());
                System.out.println(tags[l] + "|" + id + "|" + s.key);
                names.put(id, itemNames(s.getItem()));
            }
        }
        net.minecraft.flat.Stat[][] arrays = {
            net.minecraft.flat.Stats.BLOCKS_MINED, net.minecraft.flat.Stats.ITEMS_CRAFTED,
            net.minecraft.flat.Stats.ITEMS_USED, net.minecraft.flat.Stats.ITEMS_BROKEN};
        String[] arrayTags = {"Am", "Ac", "Au", "Ab"};
        for (int a = 0; a < 4; a++) {
            for (int i = 0; i < arrays[a].length; i++) {
                if (arrays[a][i] != null) {
                    System.out.println(arrayTags[a] + "|" + i + "|" + arrays[a][i].key);
                    net.minecraft.flat.Item item = net.minecraft.flat.Item.byId(i);
                    if (item != null) {
                        names.put(i, itemNames(item));
                    }
                }
            }
        }
        for (java.util.Map.Entry<Integer, String> e : names.entrySet()) {
            System.out.println("I|" + e.getKey() + "|" + e.getValue());
        }
        for (net.minecraft.flat.Entities.SpawnEggData d : net.minecraft.flat.Entities.SPAWN_EGG_DATA.values()) {
            System.out.println("E|" + net.minecraft.flat.Entities.getKey(d.id) + "|" + d.killEntityStat.key + "|"
                + d.entityKilledByStat.key);
        }
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

    /**
     * The four `pushAwayFrom` calls of `LocalClientPlayerEntity.onLivingUpdate` (bytecode offsets 376-546), on the real method.
     * A client player needs a running game, so a bare instance borrows the harness player's world, flags and velocity.
     */
    static void pushOutOfBlocks(PlayerEntity player) throws Exception {
        if (pusher == null) {
            Field unsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
            unsafe.setAccessible(true);
            pusher = (LocalClientPlayerEntity) ((sun.misc.Unsafe) unsafe.get(null)).allocateInstance(LocalClientPlayerEntity.class);
        }
        java.lang.reflect.Method push = LocalClientPlayerEntity.class.getDeclaredMethod("pushAwayFrom", double.class, double.class, double.class);
        push.setAccessible(true);
        pusher.world = player.world;
        pusher.noClip = player.noClip;
        pusher.velocityX = player.velocityX;
        pusher.velocityZ = player.velocityZ;
        double w = (double) player.width * 0.35D;
        double y = player.getShape().minY + 0.5D;
        push.invoke(pusher, player.x - w, y, player.z + w);
        push.invoke(pusher, player.x - w, y, player.z - w);
        push.invoke(pusher, player.x + w, y, player.z - w);
        push.invoke(pusher, player.x + w, y, player.z + w);
        player.velocityX = pusher.velocityX;
        player.velocityZ = pusher.velocityZ;
    }

    /** What runs between the tick's input and `mobTick`: the spectator flags, water, the pushes out of blocks. */
    static void beforeMobTick(PlayerEntity player) throws Exception {
        if (noClip) {
            player.noClip = true;
            player.onGround = false;
        }
        player.checkWaterCollisions();
        if (pushOut) {
            pushOutOfBlocks(player);
        }
    }

    static String hex(double d) {
        return String.format("%016x", Double.doubleToRawLongBits(d));
    }

    static Field field(Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }

    /** Request loop: a script is its lines up to "%%run"; the reply is the output, then "%%done" or "%%error <hex>". */
    public static void main(String[] args) throws Exception {
        Bootstrap.init();
        BLOCKS.put("air", Blocks.AIR);
        BLOCKS.put("stone", Blocks.STONE);
        BLOCKS.put("ice", Blocks.ICE);
        BLOCKS.put("water", Blocks.WATER);
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
        java.util.List<String> lines = new java.util.ArrayList<>();
        for (String line; (line = in.readLine()) != null; ) {
            if (!line.equals("%%run")) {
                lines.add(line);
                continue;
            }
            floor = Blocks.STONE;
            pushOut = false;
            noClip = false;
            world = null;
            EXTRA.clear();
            CHUNKS.clear();
            PROTO.clear();
            try {
                script(lines);
                System.out.println("%%done");
            } catch (Throwable e) {
                java.io.StringWriter text = new java.io.StringWriter();
                e.printStackTrace(new java.io.PrintWriter(text));
                System.out.println("%%error " + toHex(text.toString()));
            }
            System.out.flush();
            lines.clear();
        }
    }

    static void script(java.util.List<String> lines) throws Exception {
        PlayerEntity player = null;
        Field jumping = field(net.minecraft.flat.LivingEntity.class, "jumping");
        for (String line : lines) {
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
                case "pushout":
                    pushOut = true;
                    break;
                case "noclip":
                    noClip = true;
                    break;
                case "boots": {
                    net.minecraft.flat.ItemStack boots = new net.minecraft.flat.ItemStack(net.minecraft.flat.Items.LEATHER_BOOTS);
                    if (Integer.parseInt(t[1]) > 0) {
                        boots.addEnchantment(net.minecraft.flat.Enchantment.DEPTH_STRIDER, Integer.parseInt(t[1]));
                    }
                    player.setEquipment(1, boots);
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
                        beforeMobTick(player);
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
                case "minewith":
                    minewithCommand(player, t);
                    break;
                case "offset": {
                    System.out.println(net.minecraft.flat.MathHelper.hashCode(new net.minecraft.flat.BlockPos(
                            Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]))));
                    net.minecraft.flat.Block[] plants = {Blocks.TALLGRASS, Blocks.DEADBUSH, Blocks.YELLOW_FLOWER,
                            Blocks.RED_FLOWER, Blocks.DOUBLE_PLANT};
                    String[] names = {"tallgrass", "deadbush", "yellow_flower", "red_flower", "double_plant"};
                    for (int i = 0; i < plants.length; i++) {
                        System.out.println(names[i] + "|" + plants[i].getOffsetType());
                    }
                    break;
                }
                case "layer":
                    layerLines();
                    break;
                case "eat":
                    eatCommand(player, t);
                    break;
                case "shadow":
                    shadowCommand(t);
                    break;
                case "rail":
                    railCommand(t);
                    break;
                case "squish":
                    squishCommand(t);
                    break;
                case "body":
                    bodyCommand(t);
                    break;
                case "squid":
                    squidCommand(t);
                    break;
                case "guardian":
                    guardianCommand(t);
                    break;
                case "dragon":
                    dragonCommand(t);
                    break;
                case "mip":
                    mipLines(t);
                    break;
                case "bidi":
                    System.out.println(bidiReorder(t.length > 1 ? t[1] : ""));
                    break;
                case "signjson":
                    System.out.println(signLine(t.length > 1 ? t[1] : ""));
                    break;
                case "achievements":
                    achievementLines();
                    break;
                case "stats":
                    statLines();
                    break;
                case "statfmt":
                    statFormat(t);
                    break;
                default:
                    throw new IllegalArgumentException(line);
            }
        }
    }
}
