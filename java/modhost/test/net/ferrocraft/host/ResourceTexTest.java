package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import net.minecraft.launchwrapper.Launch;

/** A texture that the real TextureManager loads from a ResourceLocation reaches {@link GlState} like a DynamicTexture. */
public final class ResourceTexTest {
    private ResourceTexTest() {}

    public static void run() {
        int code = 1;
        try {
            Object mc = Minecraft.create(Launch.minecraftHome);
            Object tm = R.get(mc, "field_71446_o");
            Object rl = R.neu("net.minecraft.util.ResourceLocation", "textures/gui/icons.png");
            R.call(tm, "func_110577_a", rl); // bindTexture
            GlState.quad(10, 10, 0f, 0f, 16, 16, 16, 16, 256f, 256f);
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(Sink.take(false)));
            in.readUnsignedByte();
            int count = in.readInt();
            System.out.println("commands " + count);
            if (count != 2) throw new AssertionError("a ResourceLocation texture sends a define and an image, got " + count);
            System.out.println("ResourceTexTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }
}
