import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;
import java.util.zip.*;

/**
 * Static compatibility scan of Forge 1.8.9 mod jars against the vanilla 1.8.9 SRG mapping.
 * Usage: Accept joined.srg MOD.jar...
 * It does NOT run the mods. It reads each jar's constant strings and checks that every
 * net/minecraft class, func_ method and field_ field the mod names exists in vanilla 1.8.9.
 * That list is the minimum shape the stand-in Minecraft jar must provide.
 */
public final class Accept {
    private static final Pattern CLASS = Pattern.compile("net/minecraft/(?!launchwrapper)[A-Za-z0-9_/$]+");
    private static final Pattern MEMBER = Pattern.compile("\\b(?:func|field)_\\d+_[A-Za-z0-9_]+");
    private static final Pattern FORGE = Pattern.compile("net/minecraftforge/[A-Za-z0-9_/$]+");

    public static void main(String[] args) throws IOException {
        Set<String> classes = new HashSet<>();
        Set<String> members = new HashSet<>();
        for (String line : Files.readAllLines(Paths.get(args[0]))) {
            if (line.startsWith("CL: ")) {
                classes.add(line.split(" ")[2]);
            } else if (line.startsWith("MD: ")) {
                members.add(name(line.split(" ")[3]));
            } else if (line.startsWith("FD: ")) {
                members.add(name(line.split(" ")[2]));
            }
        }
        if (classes.isEmpty() || members.isEmpty()) {
            throw new IllegalStateException("no CL/MD/FD lines in " + args[0]);
        }
        System.out.println("| Mod jar | Classes (named/missing) | SRG members (named/missing) | Mixin refmap (named/missing) | Forge classes named | Static result |");
        System.out.println("|---|---|---|---|---|---|");
        StringBuilder detail = new StringBuilder();
        for (int i = 1; i < args.length; i++) {
            scan(args[i], classes, members, detail);
        }
        System.out.print("\n" + detail);
    }

    private static void scan(String jar, Set<String> classes, Set<String> members, StringBuilder detail) throws IOException {
        Set<String> cls = new TreeSet<>(), mem = new TreeSet<>(), ref = new TreeSet<>(), forge = new TreeSet<>();
        Set<String> refMem = new TreeSet<>();
        try (ZipFile zip = new ZipFile(jar)) {
            for (Enumeration<? extends ZipEntry> e = zip.entries(); e.hasMoreElements();) {
                ZipEntry entry = e.nextElement();
                String n = entry.getName();
                try (InputStream in = zip.getInputStream(entry)) {
                    if (n.endsWith(".class")) {
                        for (String s : utf8(new DataInputStream(new BufferedInputStream(in)))) {
                            collect(s, cls, mem, forge);
                        }
                    } else if (n.endsWith(".refmap.json")) {
                        collect(new String(readAll(in), "UTF-8"), ref, refMem, forge);
                    }
                }
            }
        }
        List<String> missCls = missing(cls, classes), missMem = missing(mem, members);
        List<String> missRef = missing(ref, classes), missRefMem = missing(refMem, members);
        missRef.addAll(missRefMem);
        boolean pass = missCls.isEmpty() && missMem.isEmpty() && missRef.isEmpty();
        String name = Paths.get(jar).getFileName().toString();
        System.out.println("| " + name + " | " + cls.size() + "/" + missCls.size() + " | " + mem.size() + "/" + missMem.size()
                + " | " + (ref.size() + refMem.size()) + "/" + missRef.size() + " | " + forge.size() + " | "
                + (pass ? "PASS" : "FAIL") + " |");
        for (String m : missCls) detail.append(name).append(" missing class ").append(m).append('\n');
        for (String m : missMem) detail.append(name).append(" missing member ").append(m).append('\n');
        for (String m : missRef) detail.append(name).append(" missing refmap target ").append(m).append('\n');
    }

    private static void collect(String s, Set<String> cls, Set<String> mem, Set<String> forge) {
        for (Matcher m = CLASS.matcher(s); m.find();) cls.add(trim(m.group()));
        for (Matcher m = MEMBER.matcher(s); m.find();) mem.add(m.group());
        for (Matcher m = FORGE.matcher(s); m.find();) forge.add(trim(m.group()));
    }

    /** A class token inside a descriptor or a path can run into trailing text; descriptors end at ';'. */
    private static String trim(String s) {
        return s.endsWith("/") ? s.substring(0, s.length() - 1) : s;
    }

    private static List<String> missing(Set<String> named, Set<String> known) {
        List<String> out = new ArrayList<>();
        for (String s : named) if (!known.contains(s)) out.add(s);
        return out;
    }

    private static String name(String owner) {
        return owner.substring(owner.lastIndexOf('/') + 1);
    }

    private static byte[] readAll(InputStream in) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[8192];
        for (int n; (n = in.read(buf)) > 0;) out.write(buf, 0, n);
        return out.toByteArray();
    }

    /** Every CONSTANT_Utf8 string of a class file. */
    private static List<String> utf8(DataInputStream in) throws IOException {
        if (in.readInt() != 0xCAFEBABE) throw new IOException("not a class file");
        in.readUnsignedShort();
        in.readUnsignedShort();
        int count = in.readUnsignedShort();
        List<String> out = new ArrayList<>();
        for (int i = 1; i < count; i++) {
            int tag = in.readUnsignedByte();
            switch (tag) {
                case 1: out.add(in.readUTF()); break;
                case 3: case 4: case 9: case 10: case 11: case 12: case 17: case 18: skip(in, 4); break;
                case 5: case 6: skip(in, 8); i++; break;
                case 7: case 8: case 16: case 19: case 20: skip(in, 2); break;
                case 15: skip(in, 3); break;
                default: throw new IOException("bad constant pool tag " + tag);
            }
        }
        return out;
    }

    private static void skip(DataInputStream in, int n) throws IOException {
        in.readFully(new byte[n]);
    }
}
