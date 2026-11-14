package net.ferrocraft.host;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;

/**
 * The name of each Minecraft class, field and method as the running loader knows it. The host code writes SRG names
 * (the Forge 1.8.9 names); a loader with other names gives a table file in {@code -Dferro.names}, one line per
 * name: the SRG name, a tab, the loader's name. A name that is not in the table stays as it is, so Forge needs no table.
 */
final class Names {
    private static final Map<String, String> TABLE = load(System.getProperty("ferro.names"));

    private Names() {}

    /** The loader's name for an SRG name. A shell suffix ("func_71407_l$head") stays after the mapped name. */
    static String of(String name) {
        int dollar = name.indexOf('$');
        if (dollar > 0 && name.startsWith("func_")) return of(name.substring(0, dollar)) + name.substring(dollar);
        String mapped = TABLE.get(name);
        return mapped != null ? mapped : name;
    }

    static Map<String, String> load(String file) {
        Map<String, String> table = new HashMap<>();
        if (file == null) return table;
        try (BufferedReader in = Files.newBufferedReader(Paths.get(file), StandardCharsets.UTF_8)) {
            String line;
            while ((line = in.readLine()) != null) {
                if (line.isEmpty()) continue;
                int tab = line.indexOf('\t');
                if (tab <= 0 || tab == line.length() - 1) throw new IllegalStateException("bad line in " + file + ": " + line);
                table.put(line.substring(0, tab), line.substring(tab + 1));
            }
        } catch (IOException e) {
            throw new IllegalStateException("cannot read name table " + file, e);
        }
        return table;
    }
}
