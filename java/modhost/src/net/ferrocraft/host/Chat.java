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
 * formatted text as u32 length and UTF-8 bytes. When a mod changed the chat, see {@link ChatGui}, a line of type 0 or 1
 * goes to the real handleChat and the verdict is 3: Java has it. Type 3 and 4 tell that Rust's chat input opened or
 * closed (the text is empty); the verdict is 3 when Java keeps the chat, else 0.
 */
final class Chat {
    private static final int MAX_TEXT = 1 << 16;
    private static final int INPUT_OPEN = 3;
    private static final int INPUT_CLOSED = 4;
    private static final int TAKEN = 3;

    private final Object forgeBus = Glue.forgeBus();
    private final ChatGui gui = new ChatGui();

    /** The overlay of one frame, drawn through {@link ChatGui}. */
    void renderOverlay(Object mc, float partial) {
        gui.render(mc, partial);
    }

    void line(DataInputStream in, DataOutputStream out) throws IOException {
        int type = in.readUnsignedByte();
        int len = in.readInt();
        if (len < 0 || len > MAX_TEXT) throw new IOException(len + " byte chat line");
        byte[] raw = new byte[len];
        in.readFully(raw);
        String text = new String(raw, StandardCharsets.UTF_8);
        if (type == INPUT_OPEN || type == INPUT_CLOSED) {
            gui.setOpen(type == INPUT_OPEN);
            out.writeByte(ChatGui.owned() ? TAKEN : 0);
            out.flush();
            return;
        }
        if (type < 2 && ChatGui.owned()) {
            handle(type, text);
            out.writeByte(TAKEN);
            out.flush();
            return;
        }
        if (forgeBus == null) { // no event bus to give the line to: keep it
            out.writeByte(0);
            out.flush();
            return;
        }
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
        } else if (after.getBytes(StandardCharsets.UTF_8).length > MAX_TEXT) {
            out.writeByte(0);
        } else {
            byte[] b = after.getBytes(StandardCharsets.UTF_8);
            out.writeByte(2);
            out.writeInt(b.length);
            out.write(b);
        }
        out.flush();
    }

    private void handle(int type, String text) {
        try {
            gui.handle(R.get(R.cls("net.minecraft.client.Minecraft"), "field_71432_P"), type, R.neu("net.minecraft.util.ChatComponentText", text));
        } catch (RuntimeException e) {
            // A failing mod method must not stop the next line.
            e.printStackTrace();
        }
    }
}
