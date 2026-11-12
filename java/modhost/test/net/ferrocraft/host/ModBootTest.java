package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.launchwrapper.Launch;

/**
 * Boots FML as the host does, with the mod jars in GAMEDIR/mods, and checks that FML took every mod through its
 * lifecycle: each loaded mod must be in state AVAILABLE, which FML sets after the mod's init and postinit ran.
 */
public final class ModBootTest {
    private ModBootTest() {}

    public static void run() {
        int code = 1;
        try {
            Object mc = Minecraft.create(Launch.minecraftHome);
            Minecraft.loadMods(mc);
            System.out.println("[mod-boot] Minecraft class loaded from "
                    + R.cls("net.minecraft.client.Minecraft").getProtectionDomain().getCodeSource().getLocation());
            Object loader = R.call(R.cls("net.minecraftforge.fml.common.Loader"), "instance");
            List<String> mods = new ArrayList<>();
            for (Object c : (List<?>) R.call(loader, "getModList")) {
                String id = (String) R.call(c, "getModId");
                String state = String.valueOf(R.call(loader, "getModState", c));
                System.out.println("[mod-boot] " + id + " " + state);
                if (!state.equalsIgnoreCase("AVAILABLE")) throw new AssertionError(id + " is " + state + ", not AVAILABLE");
                if (!id.equals("mcp") && !id.equals("FML") && !id.equals("Forge")) mods.add(id);
            }
            for (String want : System.getProperty("ferro.mods", "").split(",")) {
                if (!want.isEmpty() && mods.stream().noneMatch(want::equalsIgnoreCase)) {
                    throw new AssertionError("expected mod " + want + " is not loaded, loaded: " + mods);
                }
            }
            if (mods.isEmpty()) throw new AssertionError("no mod from GAMEDIR/mods was loaded");
            System.out.println("ModBootTest passed: " + mods);
            code = 0;
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Host.halt(code);
    }
}
