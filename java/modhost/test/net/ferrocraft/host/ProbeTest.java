package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import net.minecraftforge.client.event.RenderLivingEvent;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Runs the real Probe on real FML: with no handler the reply is vanilla (red 0.3 hurt tint, no render events); with a
 * RenderLivingEvent.Pre handler that cancels one id, the reply marks that id (a mob, then the own player id) and only it.
 */
public final class ProbeTest {
    private static final int SELF = 1;
    private static final int PIG = 7;
    private static final int COW = 9;
    private final List<String> seen = new ArrayList<>();
    private int cancel = -1;

    private ProbeTest() {}

    public static void run() {
        int code = 1;
        try {
            Object mc = Minecraft.create(net.minecraft.launchwrapper.Launch.minecraftHome);
            R.set(R.get(mc, "field_71439_g"), "field_145783_c", SELF); // the mirror owns the player id
            Events events = new Events(mc);
            join(events, 0, 0, SELF);
            join(events, 2, 90, PIG); // a pig
            join(events, 2, 92, COW); // a cow
            Probe probe = new Probe(mc, events);
            int[] ids = {SELF, PIG, COW};

            byte[] r = ask(probe, 7, ids);
            check((r[0] & 4) == 0, "no handler: render events not valid");
            check((r[0] & 1) != 0, "hurt tint valid, got " + r[0]);
            DataInputStream d = new DataInputStream(new ByteArrayInputStream(r, 1, r.length - 1));
            float red = d.readFloat();
            check(Math.abs(red - 1f) < 1e-6, "vanilla hurt tint red=1, got " + red);
            check(Math.abs(d.readFloat()) < 1e-6 && Math.abs(d.readFloat()) < 1e-6, "vanilla tint g,b = 0");
            check(Math.abs(d.readFloat() - 0.3f) < 1e-6, "vanilla tint alpha 0.3");

            ProbeTest t = new ProbeTest();
            MinecraftForge.EVENT_BUS.register(t);
            t.cancel = PIG;
            r = ask(probe, 4, ids);
            check((r[0] & 4) != 0, "handler listening: valid bit 4");
            byte[] c = java.util.Arrays.copyOfRange(r, r.length - 3, r.length);
            check(c[0] == 0 && c[1] == 1 && c[2] == 0, "only the pig cancelled, got " + java.util.Arrays.toString(c));
            check(t.seen.toString().equals("[pre 1, post 1, pre 7, pre 9, post 9]"), "events: " + t.seen);
            t.seen.clear();
            t.cancel = SELF;
            r = ask(probe, 4, ids);
            c = java.util.Arrays.copyOfRange(r, r.length - 3, r.length);
            check(c[0] == 1 && c[1] == 0 && c[2] == 0, "self id cancelled, got " + java.util.Arrays.toString(c));
            check(t.seen.toString().equals("[pre 1, pre 7, post 7, pre 9, post 9]"), "events: " + t.seen);
            System.out.println("ProbeTest passed");
            code = 0;
        } catch (Throwable e) {
            e.printStackTrace();
        }
        Host.halt(code);
    }

    @SubscribeEvent
    public void pre(RenderLivingEvent.Pre e) {
        int id = (Integer) R.call(R.get(e, "entity"), "func_145782_y");
        seen.add("pre " + id);
        if (id == cancel) e.setCanceled(true);
    }

    @SubscribeEvent
    public void post(RenderLivingEvent.Post e) {
        seen.add("post " + R.call(R.get(e, "entity"), "func_145782_y"));
    }

    private static byte[] ask(Probe probe, int want, int[] ids) throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeByte(want);
        w.writeByte(10);
        w.writeInt(ids.length);
        for (int id : ids) {
            w.writeInt(id);
            for (int k = 0; k < 3; k++) w.writeDouble(k);
        }
        ByteArrayOutputStream reply = new ByteArrayOutputStream();
        probe.answer(new DataInputStream(new ByteArrayInputStream(b.toByteArray())), new DataOutputStream(reply));
        return reply.toByteArray();
    }

    private static void join(Events events, int kind, int type, int id) throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream w = new DataOutputStream(b);
        w.writeByte('j');
        w.writeInt(id);
        w.writeByte(kind);
        w.writeInt(type);
        w.writeLong(1);
        w.writeLong(2);
        for (double v : new double[] {1.5, 64, -2}) w.writeDouble(v);
        events.apply(new DataInputStream(new ByteArrayInputStream(b.toByteArray())));
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
