package net.ferrocraft.prepare;

import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.Enumeration;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import java.util.zip.ZipOutputStream;
import net.minecraftforge.fml.common.patcher.ClassPatchManager;
import net.minecraftforge.fml.relauncher.Side;

/**
 * Makes the two jars the mod host boots on, from the player's own 1.8.9 client jar and the Forge universal jar:
 * ferro-mc-1.8.9.jar (the client jar with Forge's binpatches applied, so FML patches nothing at boot) and
 * ferro-forge.jar (the Forge jar without binpatches.pack.lzma). Neither jar is committed; nothing Mojang or Forge
 * made ships with this project.
 */
public final class Prepare {
    private Prepare() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("usage: Prepare <client-1.8.9.jar> <forge-universal.jar> <out-dir>");
        ZipFile client = new ZipFile(args[0]);
        ZipFile forge = new ZipFile(args[1]);
        if (forge.getEntry("binpatches.pack.lzma") == null) throw new IllegalStateException("no binpatches.pack.lzma in the Forge jar");
        java.lang.reflect.Field side = net.minecraftforge.fml.relauncher.FMLRelaunchLog.class.getDeclaredField("side");
        side.setAccessible(true);
        side.set(null, Side.CLIENT);
        ClassPatchManager patcher = ClassPatchManager.INSTANCE;
        patcher.setup(Side.CLIENT);

        int patched = 0;
        try (ZipOutputStream out = new ZipOutputStream(new FileOutputStream(args[2] + "/ferro-mc-1.8.9.jar"))) {
            for (Enumeration<? extends ZipEntry> e = client.entries(); e.hasMoreElements(); ) {
                ZipEntry entry = e.nextElement();
                // The vanilla jar is signed; a patched class breaks its digests, so the copy carries no META-INF.
                if (entry.isDirectory() || entry.getName().startsWith("META-INF/")) continue;
                byte[] bytes = read(client, entry);
                if (entry.getName().endsWith(".class") && !entry.getName().startsWith("META-INF/")) {
                    String name = entry.getName().substring(0, entry.getName().length() - ".class".length()).replace('/', '.');
                    byte[] next = patcher.applyPatch(name, name, bytes);
                    if (next != null && next != bytes) {
                        patched++;
                        bytes = next;
                    }
                }
                out.putNextEntry(new ZipEntry(entry.getName()));
                out.write(bytes);
                out.closeEntry();
            }
        }
        try (ZipOutputStream out = new ZipOutputStream(new FileOutputStream(args[2] + "/ferro-forge.jar"))) {
            for (Enumeration<? extends ZipEntry> e = forge.entries(); e.hasMoreElements(); ) {
                ZipEntry entry = e.nextElement();
                if (entry.isDirectory() || entry.getName().equals("binpatches.pack.lzma")) continue;
                out.putNextEntry(new ZipEntry(entry.getName()));
                out.write(read(forge, entry));
                out.closeEntry();
            }
        }
        System.out.println("[prepare] patched " + patched + " classes into " + args[2] + "/ferro-mc-1.8.9.jar");
    }

    private static byte[] read(ZipFile zip, ZipEntry entry) throws Exception {
        try (InputStream in = zip.getInputStream(entry)) {
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
            return out.toByteArray();
        }
    }
}
