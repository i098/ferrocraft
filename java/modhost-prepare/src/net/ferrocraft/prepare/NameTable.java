package net.ferrocraft.prepare;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Reader;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.fabricmc.mappingio.MappingReader;
import net.fabricmc.mappingio.MappingWriter;
import net.fabricmc.mappingio.adapter.MappingDstNsReorder;
import net.fabricmc.mappingio.adapter.MappingNsRenamer;
import net.fabricmc.mappingio.adapter.MappingSourceNsSwitch;
import net.fabricmc.mappingio.format.MappingFormat;
import net.fabricmc.mappingio.tree.MemoryMappingTree;

/**
 * Builds the name table of the Fabric host: Forge's MCP names (srg: net/minecraft/client/Minecraft, field_71439_g,
 * func_71411_J) to the intermediary names of the Fabric profile (class_310, field_1724, method_1574), as a Tiny v2 file.
 * The host code and the mods' hooks use the Forge names; Knot loads the game under the intermediary names.
 * Both inputs are read from their archives: the SRG file joined.srg from the MCP zip, and mappings/mappings.tiny
 * (official to intermediary) from the intermediary jar. Nothing Mojang or Forge made is committed.
 */
public final class NameTable {
    private NameTable() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("usage: NameTable <mcp-srg.zip> <intermediary.jar> <out.tiny>");
        MemoryMappingTree tree = new MemoryMappingTree();
        try (ZipFile zip = new ZipFile(args[0]); Reader in = entry(zip, "joined.srg")) {
            Map<String, String> rename = new HashMap<String, String>();
            rename.put("source", "official");
            rename.put("target", "srg");
            MappingReader.read(in, MappingFormat.SRG_FILE, new MappingNsRenamer(tree, rename));
        }
        try (ZipFile zip = new ZipFile(args[1]); Reader in = entry(zip, "mappings/mappings.tiny")) {
            MappingReader.read(in, MappingFormat.TINY_FILE, tree);
        }
        try (Writer out = new BufferedWriter(new OutputStreamWriter(Files.newOutputStream(Paths.get(args[2])), StandardCharsets.UTF_8));
             MappingWriter w = MappingWriter.create(out, MappingFormat.TINY_2_FILE)) {
            tree.accept(new MappingSourceNsSwitch(new MappingDstNsReorder(w, Arrays.asList("intermediary")), "srg"));
        }
        System.out.println("[names] " + tree.getClasses().size() + " classes -> " + args[2]);
    }

    private static Reader entry(ZipFile zip, String name) throws Exception {
        ZipEntry e = zip.getEntry(name);
        if (e == null) throw new IllegalStateException("no " + name + " in " + zip.getName());
        return new BufferedReader(new InputStreamReader(zip.getInputStream(e), StandardCharsets.UTF_8));
    }
}
