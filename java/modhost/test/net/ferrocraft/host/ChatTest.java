package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import net.minecraftforge.client.event.ClientChatReceivedEvent;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/** Posts chat lines through {@link Chat} to a Forge handler that cancels "spam" and rewrites "hello". */
public final class ChatTest {
    private ChatTest() {}

    public static void run() {
        int code = 1;
        try {
            Minecraft.create(net.minecraft.launchwrapper.Launch.minecraftHome);
            MinecraftForge.EVENT_BUS.register(new ChatTest());
            Chat chat = new Chat();
            check(same(send(chat, "just chatting"), new byte[] {0}), "an untouched line is kept");
            check(same(send(chat, "buy spam now"), new byte[] {1}), "a cancelled line is cancelled");
            byte[] r = send(chat, "\u00a7ahello\u00a7r");
            check(r[0] == 2, "a changed line is replaced");
            check(new String(r, 5, r.length - 5, StandardCharsets.UTF_8).contains("bye"), "the replacement text is the handler's");
            System.out.println("ChatTest passed");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    @SubscribeEvent
    public void onChat(ClientChatReceivedEvent e) {
        // The Minecraft jar keeps Notch names, so the message is read by SRG name.
        String text = (String) R.call(R.get(e, "message"), "func_150260_c"); // getUnformattedText
        if (text.contains("spam")) e.setCanceled(true);
        else if (text.contains("hello")) R.set(e, "message", R.neu("net.minecraft.util.ChatComponentText", "bye"));
    }

    private static byte[] send(Chat chat, String text) throws IOException {
        byte[] b = text.getBytes(StandardCharsets.UTF_8);
        ByteArrayOutputStream request = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(request);
        w.writeByte(0);
        w.writeInt(b.length);
        w.write(b);
        ByteArrayOutputStream reply = new ByteArrayOutputStream();
        chat.line(new DataInputStream(new ByteArrayInputStream(request.toByteArray())), new DataOutputStream(reply));
        return reply.toByteArray();
    }

    private static boolean same(byte[] a, byte[] b) {
        return java.util.Arrays.equals(a, b);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
