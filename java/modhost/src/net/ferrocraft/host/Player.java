package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;

/**
 * The input half of EntityPlayerSP, run as vanilla runs it. Rust owns the world and the movement; the
 * stand-in player only takes the state that EntityPlayerSP.onLivingUpdate reads (ground, collision, food,
 * flight, item use), runs the real method, and Rust reads back what the mods can change: the sprint and
 * sneak state. Mods hook that method with Mixin (PolySprint redirects KeyBinding.isKeyDown there), so
 * the hooks only act when the real method runs.
 *
 * <p>The world parts of onLivingUpdate do nothing here (see BridgeTransformer.playerMethod): the block
 * push-out and EntityPlayer.onLivingUpdate, which move the entity. Rust does that.
 *
 * <p>Request (Rust to Java), after the tag 'P': u8 onGround, u8 collidedHorizontally, i32 foodLevel, u8 flags
 * (1 allowFlying, 2 isFlying, 4 usingItem). Reply: u8 sprinting, u8 sneaking.
 */
final class Player {
    private final Object player;
    private final Object input;

    Player(Object mc, Object player, Object settings) {
        this.player = player;
        Object dataWatcher = R.neu("net.minecraft.entity.DataWatcher", player);
        R.set(player, "field_70180_af", dataWatcher);
        R.set(player, "field_70713_bf", new java.util.HashMap<Object, Object>()); // activePotionsMap
        R.set(player, "field_71075_bZ", R.neu("net.minecraft.entity.player.PlayerCapabilities"));
        R.set(player, "field_70130_N", 0.6f); // width
        R.set(player, "field_70121_D", R.neu("net.minecraft.util.AxisAlignedBB", 0.0, 0.0, 0.0, 0.6, 1.8, 0.6));
        R.call(dataWatcher, "func_75682_a", 0, Byte.valueOf((byte) 0)); // the flags, as the Entity constructor adds them
        R.call(dataWatcher, "func_75682_a", 1, Short.valueOf((short) 300)); // air
        R.call(player, "func_70088_a"); // entityInit: the flags and health in the data watcher
        R.call(player, "func_110147_ax"); // applyEntityAttributes: the movement speed that sprinting modifies
        R.set(player, "field_71159_c", mc);
        R.set(player, "field_71174_a", R.alloc("net.minecraft.client.network.NetHandlerPlayClient")); // sendQueue: sends nothing, see BridgeTransformer
        this.input = R.neu("net.minecraft.util.MovementInputFromOptions", settings);
        R.set(player, "field_71158_b", input);
    }

    /** Reads one request, runs one tick of EntityPlayerSP.onLivingUpdate and writes the reply. */
    void tick(DataInputStream in, DataOutputStream out) throws IOException {
        boolean onGround = in.readUnsignedByte() != 0;
        boolean collided = in.readUnsignedByte() != 0;
        int food = in.readInt();
        int flags = in.readUnsignedByte();
        R.set(player, "field_70122_E", onGround);
        R.set(player, "field_70123_F", collided);
        R.set(R.get(player, "field_71100_bB"), "field_75127_a", food); // foodStats.foodLevel
        Object caps = R.get(player, "field_71075_bZ");
        R.set(caps, "field_75101_c", (flags & 1) != 0); // allowFlying
        R.set(caps, "field_75100_b", (flags & 2) != 0); // isFlying
        R.set(player, "field_71074_e", (flags & 4) != 0 ? R.alloc("net.minecraft.item.ItemStack") : null); // itemInUse
        R.call(player, "func_70636_d");
        out.writeByte(Boolean.TRUE.equals(R.call(player, "func_70051_ag")) ? 1 : 0); // isSprinting
        out.writeByte(R.get(input, "field_78899_d") == Boolean.TRUE ? 1 : 0); // MovementInput.sneak
        out.flush();
    }
}
