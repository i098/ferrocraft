import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.*;
import net.minecraft.flat.*;
import sun.misc.Unsafe;

/**
 * Prints the 1.8.9 item tables of crates/world/src/inventory (*.tsv) from the real client classes.
 *
 * Build and run against the remapped client jar and its libraries (crates/oracle makes them):
 *   javac -cp "named.jar:libs/*" -d . Dumper.java
 *   java -cp "named.jar:libs/*:." Dumper crates/world/src/inventory
 */
public class Dumper {
    static String esc(String s) {
        return s.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n");
    }

    static String list(List<String> l) {
        StringBuilder b = new StringBuilder();
        for (String s : l) {
            if (b.length() > 0) b.append("\\n");
            b.append(esc(s).replace("\\n", "\\\\n"));
        }
        return b.toString();
    }

    static Object get(Object o, Class<?> c, String f) throws Exception {
        Field fl = c.getDeclaredField(f);
        fl.setAccessible(true);
        return fl.get(o);
    }

    static String enchNbt(ItemStack s) {
        NbtCompound tag = s.getNbt();
        if (tag == null) return "";
        NbtList l = tag.getList("StoredEnchantments", 10);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < l.size(); i++) {
            NbtCompound e = l.getCompound(i);
            b.append(e.getShort("id")).append(':').append(e.getShort("lvl"));
        }
        return b.toString();
    }

    static final String HDR = "# Generated from the Minecraft 1.8.9 client classes (Item registry, creative tabs, RenderItem model\n"
        + "# registrations); see crates/world/src/inventory/items.rs.\n";

    static java.io.PrintWriter tsv(String dir, String name, String columns) throws Exception {
        java.io.PrintWriter w = new java.io.PrintWriter(dir + "/" + name + ".tsv", "UTF-8");
        w.print(HDR + columns + "\n");
        return w;
    }

    /** RenderItem's ItemModelMesher, filled by registerGuiModels without a GL context. */
    static ItemModelShaper shaper() throws Exception {
        Field uf = Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        Unsafe u = (Unsafe) uf.get(null);
        ModelManager mm = (ModelManager) u.allocateInstance(ModelManager.class);
        Field reg = ModelManager.class.getDeclaredField("registry");
        reg.setAccessible(true);
        reg.set(mm, new MappedRegistry());
        ItemModelShaper shaper = new ItemModelShaper(mm);
        ItemRenderer ir = (ItemRenderer) u.allocateInstance(ItemRenderer.class);
        Field sh = ItemRenderer.class.getDeclaredField("modelShaper");
        sh.setAccessible(true);
        sh.set(ir, shaper);
        Method rg = ItemRenderer.class.getDeclaredMethod("registerGuiModels");
        rg.setAccessible(true);
        rg.invoke(ir);
        return shaper;
    }

    static Map<String, String> lang() throws Exception {
        Map<String, String> lang = new HashMap<>();
        try (java.io.BufferedReader r = new java.io.BufferedReader(new java.io.InputStreamReader(
                Dumper.class.getResourceAsStream("/assets/minecraft/lang/en_US.lang"), "UTF-8"))) {
            for (String l; (l = r.readLine()) != null; ) {
                int i = l.indexOf('=');
                if (i > 0) lang.put(l.substring(0, i), l.substring(i + 1));
            }
        }
        return lang;
    }

    static List<ItemStack> creativeStacks(Item it) {
        List<ItemStack> subs = new ArrayList<>();
        try {
            it.addToCreativeMenu(it, it.getCreativeModeTab(), subs);
        } catch (Throwable t) {
            subs.clear();
        }
        if (subs.isEmpty()) subs.add(new ItemStack(it, 1, 0));
        return subs;
    }

    static String variantRow(Item it, int id, ItemStack s) {
        List<String> info = new ArrayList<>();
        try {
            it.addHoverText(s, null, info, false);
        } catch (Throwable t) {
            info.clear();
        }
        List<String> tip;
        try {
            tip = s.getTooltip(null, false);
        } catch (Throwable t) {
            tip = new ArrayList<>(Collections.singletonList(s.getHoverName()));
            tip.addAll(info);
        }
        List<String> attrs = tip.subList(Math.min(tip.size(), 1 + info.size()), tip.size());
        boolean smelts = SmeltingManager.getInstance().getResult(s) != null;
        return id + "\t" + s.getMetadata() + "\t" + esc(s.getHoverName()) + "\t" + s.getRarity().ordinal() + "\t" + (s.hasEnchantmentGlint() ? 1 : 0) + "\t"
            + (FurnaceBlockEntity.isFuel(s) ? 1 : 0) + "\t" + (smelts ? 1 : 0) + "\t" + Integer.toHexString(it.getColorTint(s, 0)) + "\t" + Integer.toHexString(it.getColorTint(s, 1)) + "\t" + list(info) + "\t" + list(attrs);
    }

    /** GrassColors is empty until the grass colormap is set; getColorTint then returns 0 (black icons). */
    static void loadGrassColormap() throws Exception {
        java.awt.image.BufferedImage img = javax.imageio.ImageIO.read(Dumper.class.getResourceAsStream("/assets/minecraft/textures/colormap/grass.png"));
        int w = img.getWidth();
        int[] px = img.getRGB(0, 0, w, img.getHeight(), null, 0, w);
        for (int i = 0; i < px.length; i++) px[i] &= 0xFFFFFF; // getRGB yields ARGB; tints are RGB
        GrassColors.set(px);
    }

    /** Writes items.tsv and variants.tsv; returns every creative stack except enchanted books. */
    static List<ItemStack> writeItems(String out) throws Exception {
        List<ItemStack> all = new ArrayList<>();
        Set<Long> seen = new HashSet<>();
        try (java.io.PrintWriter items = tsv(out, "items", "# id\tkey\tmax_stack\tmax_damage\tsubtypes\tarmor_slot");
             java.io.PrintWriter vars = tsv(out, "variants", "# id\tmeta\tname\trarity\tglint\tfuel\tsmelts\ttint0\ttint1\tinfo\tattributes (lines split by \\n)")) {
            for (Item it : Item.REGISTRY) {
                int id = Item.getId(it);
                int armor = it instanceof ArmorItem ? ((ArmorItem) it).slot : -1;
                items.println(id + "\t" + Item.REGISTRY.getKey(it).toString().replaceFirst("^minecraft:", "") + "\t" + it.getMaxStackSize() + "\t" + it.getMaxDamage() + "\t" + (it.hasCustomData() ? 1 : 0) + "\t" + armor);
                if (it instanceof EnchantedBookItem) continue;
                List<ItemStack> subs = creativeStacks(it);
                all.addAll(subs);
                for (ItemStack s : subs) {
                    if (s.getItem() == it && seen.add(((long) id << 16) | s.getMetadata())) vars.println(variantRow(it, id, s));
                }
            }
        }
        return all;
    }

    /** Model locations: fixed (item, meta) pairs, then providers asked with each known variant. */
    @SuppressWarnings("unchecked")
    static void writeModels(String out, List<ItemStack> all) throws Exception {
        ItemModelShaper shaper = shaper();
        Map<Integer, ModelIdentifier> locs = (Map<Integer, ModelIdentifier>) get(shaper, ItemModelShaper.class, "modelLocations");
        Map<Item, ItemModelLocationProvider> provs = (Map<Item, ItemModelLocationProvider>) get(shaper, ItemModelShaper.class, "modelProviders");
        Set<Long> seen = new HashSet<>();
        try (java.io.PrintWriter models = tsv(out, "models", "# id\tmeta\tmodel (models/item/<model>.json)")) {
            for (Map.Entry<Integer, ModelIdentifier> e : new TreeMap<>(locs).entrySet()) {
                if (seen.add((long) e.getKey())) models.println((e.getKey() >>> 16) + "\t" + (e.getKey() & 0xFFFF) + "\t" + e.getValue().getPath());
            }
            for (Map.Entry<Item, ItemModelLocationProvider> e : provs.entrySet()) {
                Item it = e.getKey();
                int id = Item.getId(it);
                Set<Integer> metas = new TreeSet<>();
                for (ItemStack s : all) if (s.getItem() == it) metas.add(s.getMetadata());
                metas.add(0);
                for (int m : metas) {
                    if (seen.add(((long) id << 16) | m)) models.println(id + "\t" + m + "\t" + e.getValue().getModelLocation(new ItemStack(it, 1, m)).getPath());
                }
            }
        }
    }

    static List<ItemStack> tabStacks(CreativeModeTab t) {
        List<ItemStack> l = new ArrayList<>();
        if (t == CreativeModeTab.SEARCH) {
            for (Item it : Item.REGISTRY) {
                if (it.getCreativeModeTab() != null) it.addToCreativeMenu(it, null, l);
            }
            for (Enchantment e : Enchantment.ALL) {
                if (e != null && e.category != null) ((EnchantedBookItem) Items.ENCHANTED_BOOK).addToCreativeMenu(e, l);
            }
        } else if (t != CreativeModeTab.INVENTORY) {
            t.addItems(l);
        }
        return l;
    }

    static void writeTabs(String out, Map<String, String> lang) throws Exception {
        try (java.io.PrintWriter tabs = tsv(out, "tabs", "# tab\tlabel\ticon_id\ticon_meta\ttexture\tscrollbar\ttitle\tentries (id:meta, enchanted books id:meta:ench:level)")) {
            for (CreativeModeTab t : CreativeModeTab.ALL) {
                if (t == null) continue;
                StringBuilder b = new StringBuilder();
                for (ItemStack s : tabStacks(t)) {
                    if (b.length() > 0) b.append(' ');
                    b.append(Item.getId(s.getItem())).append(':').append(s.getMetadata());
                    String en = enchNbt(s);
                    if (!en.isEmpty()) b.append(':').append(en);
                }
                ItemStack icon = t.getIcon();
                tabs.println(t.getId() + "\t" + lang.get(t.getDisplayName()) + "\t" + Item.getId(icon.getItem()) + "\t" + icon.getMetadata() + "\t" + t.getTexture() + "\t" + (t.hasScrollbar() ? 1 : 0) + "\t" + (t.hasTooltips() ? 1 : 0) + "\t" + b);
            }
        }
    }

    public static void main(String[] a) throws Exception {
        Bootstrap.init();
        loadGrassColormap();
        String out = a[0];
        List<ItemStack> all = writeItems(out);
        writeModels(out, all);
        writeTabs(out, lang());
        try (java.io.PrintWriter ench = tsv(out, "enchantments", "# id\tname\tmax_level")) {
            for (Enchantment e : Enchantment.ALL) {
                if (e != null) ench.println(e.id + "\t" + esc(e.getName(1)).replaceFirst(" I$", "") + "\t" + e.getMaxLevel());
            }
        }
    }
}
