package net.ferrocraft.host;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/**
 * Chat lines for the mods. Rust receives the line from the server; this posts Forge's
 * ClientChatReceivedEvent on the Forge bus as NetHandlerPlayClient does, and Rust acts on the result.
 *
 * <p>Request (Rust to Java), after the tag 'C': u8 type (0 chat, 1 system, 2 action bar), u32 length and
 * UTF-8 bytes of the formatted text. Reply: u8 verdict (0 keep, 1 cancel, 2 replace) and, for 2, the new
 * formatted text as u32 length and UTF-8 bytes.
 */
final class Chat {
    private static final int MAX_TEXT = 1 << 16;

    private final Object forgeBus = R.get(R.cls("net.minecraftforge.common.MinecraftForge"), "EVENT_BUS");

    void line(DataInputStream in, DataOutputStream out) throws IOException {
        int type = in.readUnsignedByte();
        int len = in.readInt();
        if (len < 0 || len > MAX_TEXT) throw new IOException(len + " byte chat line");
        byte[] raw = new byte[len];
        in.readFully(raw);
        String text = new String(raw, StandardCharsets.UTF_8);
        String after = text;
        boolean cancelled;
        try {
            Object message = R.neu("net.minecraft.util.ChatComponentText", text);
            String before = (String) R.call(message, "func_150254_d"); // getFormattedText
            Object event = R.neu("net.minecraftforge.client.event.ClientChatReceivedEvent", (byte) type, message);
            cancelled = Boolean.TRUE.equals(R.call(forgeBus, "post", event));
            Object result = R.get(event, "message");
            after = (String) R.call(result, "func_150254_d");
            if (before.equals(after)) after = text;
        } catch (RuntimeException e) {
            // A failing mod handler must not lose the line.
            e.printStackTrace();
            cancelled = false;
        }
        if (cancelled) {
            out.writeByte(1);
        } else if (after.equals(text)) {
            out.writeByte(0);
        } else {
            byte[] b = after.getBytes(StandardCharsets.UTF_8);
            out.writeByte(2);
            out.writeInt(b.length);
            out.write(b);
        }
        out.flush();
    }
}
