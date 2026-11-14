package net.ferrocraft.host;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.launchwrapper.Launch;

/**
 * Boots FML as the host does and reads the hook table. With no jar in GAMEDIR/mods the table must be empty. With the
 * OneClient jars it must hold the GuiIngame crosshair method, which PolyCrosshair changes, and a method of a class the host
 * never loads. Also reads the hello part back the way {@code hooks.rs} does.
 */
public final class HookIndexTest {
    private static final String CROSSHAIR = "net/minecraft/client/gui/GuiIngame.func_175183_b()Z"; // GuiIngame.showCrosshair
    // GuiTextField.drawTextBox. The host never loads it; the table holds it because settle() loads each Mixin target.
    private static final String UNLOADED = "net/minecraft/client/gui/GuiTextField.func_146194_f()V";

    private HookIndexTest() {}

    public static void run() {
        int code = 1;
        try {
            Object mc = Minecraft.create(Launch.minecraftHome);
            Minecraft.loadMods(mc);
            HookIndex.settle();
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            HookIndex.write(new DataOutputStream(bytes));
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes.toByteArray()));
            List<String> sent = new ArrayList<>();
            for (int n = in.readInt(); n > 0; n--) {
                String owner = in.readUTF();
                String name = in.readUTF();
                sent.add(owner + "." + name + in.readUTF());
            }
            check(in.available() == 0, "the table is the whole message");
            check(sent.equals(HookIndex.list()), "the message holds the list");
            File[] jars = new File(Launch.minecraftHome, "mods").listFiles((d, f) -> f.endsWith(".jar"));
            if (jars == null || jars.length == 0) {
                check(sent.isEmpty(), "no mod, so no changed method: " + sent);
            } else {
                for (String s : sent) System.out.println("[hooks] " + s);
                check(sent.contains(CROSSHAIR), "the crosshair method is in the table of " + sent.size() + " methods");
                check(sent.contains(UNLOADED), "a class the host never loaded is too: " + sent.size() + " methods");
                check(sent.stream().noneMatch(s -> s.substring(0, s.indexOf('(')).contains("$")), "generated methods are not listed");
            }
            System.out.println("HookIndexTest passed: " + sent.size() + " changed methods");
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
