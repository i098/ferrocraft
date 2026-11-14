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
            Object mc = Minecraft.create(net.minecraft.launchwrapper.Launch.minecraftHome);
            MinecraftForge.EVENT_BUS.register(new ChatTest());
            Chat chat = new Chat();
            check(same(send(chat, "just chatting"), new byte[] {0}), "an untouched line is kept");
            check(same(send(chat, "buy spam now"), new byte[] {1}), "a cancelled line is cancelled");
            byte[] r = send(chat, "\u00a7ahello\u00a7r");
            check(r[0] == 2, "a changed line is replaced");
            check(new String(r, 5, r.length - 5, StandardCharsets.UTF_8).contains("bye"), "the replacement text is the handler's");
            check(!ChatGui.owns(java.util.Collections.singletonList("net/minecraft/client/gui/GuiIngame.func_175183_b()Z")), "a HUD hook is not a chat hook");
            check(ChatGui.owns(java.util.Collections.singletonList("net/minecraft/client/gui/GuiNewChat.func_146230_a(I)V")), "a GuiNewChat hook is a chat hook");
            check(ChatGui.owns(java.util.Collections.singletonList("net/minecraft/client/network/NetHandlerPlayClient.func_147251_a(Lnet/minecraft/network/play/server/S02PacketChat;)V")), "a handleChat hook is a chat hook");
            check(same(send(chat, 3, ""), new byte[] {0}), "with no chat hook the input state answers 0");
            ChatGui.own(true);
            Object drawn = R.get(R.call(R.get(mc, "field_71456_v"), "func_146158_b"), "field_146253_i"); // drawnChatLines
            check(same(send(chat, "just chatting"), new byte[] {3}), "Java takes a line when it keeps the chat");
            check(((java.util.List<?>) drawn).size() == 1, "the line is in GuiNewChat");
            check(same(send(chat, "buy spam now"), new byte[] {3}), "Java takes a cancelled line too");
            check(((java.util.List<?>) drawn).size() == 1, "a cancelled line is not in GuiNewChat");
            check(same(send(chat, 3, ""), new byte[] {3}), "the input state answers 3 when Java keeps the chat");
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
        return send(chat, 0, text);
    }

    private static byte[] send(Chat chat, int type, String text) throws IOException {
        byte[] b = text.getBytes(StandardCharsets.UTF_8);
        ByteArrayOutputStream request = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(request);
        w.writeByte(type);
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
