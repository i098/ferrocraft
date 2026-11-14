package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.launchwrapper.IClassTransformer;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.AnnotationNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.FrameNode;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.JumpInsnNode;
import org.objectweb.asm.tree.LabelNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.TypeInsnNode;
import org.objectweb.asm.tree.VarInsnNode;

/**
 * Mode A of docs/mod-host.md for the loop methods of {@code Minecraft}: {@code runTick}, {@code runGameLoop} and
 * {@code startGame}. Rust runs the loop, so the vanilla instructions of these methods have no use. What a mod added does: a
 * Mixin injector (at the head, at the return, or at an invoke the host does not need) leaves a block in the method, which
 * makes a {@code CallbackInfo} (or passes null when the handler does not use it), calls the handler (a method that Mixin
 * merged into the class) and, when the handler may cancel, returns if it did. This keeps those blocks, in order, and drops
 * everything else, so that the host calls the method and the handlers run with no hand-made copy.
 *
 * <p>A handler that reads a local of the vanilla body, and a redirect or a modified variable, needs the state of the vanilla
 * instructions around it, and loses its place. {@link #lost()} lists each such handler call. Registered after
 * {@link HookIndex.Scanner}, so that the hook table still sees the hooks of the whole method.
 */
public final class ShellTransformer implements IClassTransformer {
    private static final String MERGED = "Lorg/spongepowered/asm/mixin/transformer/meta/MixinMerged;";
    private static final String CALLBACK = "org/spongepowered/asm/mixin/injection/callback/CallbackInfo";
    private static final int MAX_BLOCK = 40;
    /** The methods in SRG names (what the host sees) and in MCP names; all take no argument and return nothing. */
    private static final Set<String> SHELLS = new HashSet<>();

    static {
        for (String n : new String[] {"func_71407_l", "runTick", "func_71411_J", "runGameLoop", "func_71384_a", "startGame"}) {
            SHELLS.add(n + "()V");
        }
    }

    private static final Set<String> kept = new LinkedHashSet<>();
    private static final Set<String> lost = new LinkedHashSet<>();

    @Override
    public byte[] transform(String name, String transformedName, byte[] bytes) {
        if (bytes == null || !transformedName.equals("net.minecraft.client.Minecraft")) return bytes;
        return shell(bytes);
    }

    /** Handler calls kept in a shell, as {@code method: handler}. */
    public static List<String> kept() {
        synchronized (kept) {
            return new ArrayList<>(kept);
        }
    }

    /** Handler calls that the vanilla body of a shell held and that are gone, as {@code method: handler}. */
    public static List<String> lost() {
        synchronized (kept) {
            return new ArrayList<>(lost);
        }
    }

    /** Turns the loop methods of the class into shells; with no mod hook there, a shell holds only the return. */
    static byte[] shell(byte[] bytes) {
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, ClassReader.SKIP_DEBUG);
        Set<String> merged = new HashSet<>();
        for (MethodNode m : methods(cls)) {
            if (isMerged(m)) merged.add(m.name + m.desc);
        }
        for (MethodNode m : methods(cls)) {
            if (SHELLS.contains(m.name + m.desc)) shell(cls, m, merged);
        }
        ClassWriter out = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cls.accept(out);
        return out.toByteArray();
    }

    private static void shell(ClassNode cls, MethodNode m, Set<String> merged) {
        InsnList old = m.instructions;
        List<AbstractInsnNode> keep = new ArrayList<>();
        Set<String> seen = new HashSet<>();
        Map<LabelNode, Integer> frames = new HashMap<>();
        for (AbstractInsnNode n = old.getFirst(); n != null; ) {
            Block b = n.getOpcode() == Opcodes.NEW && ((TypeInsnNode) n).desc.equals(CALLBACK) ? block(n, cls, merged) : nullBlock(n, cls, merged);
            if (b == null) {
                n = n.getNext();
                continue;
            }
            // shortcut: a block in a forward branch is kept as if it always ran, upgrade when a mod hook there must stay conditional.
            if (inLoop(old, n)) {
                n = b.end.getNext();
                continue;
            }
            if (seen.add(b.handler)) {
                for (AbstractInsnNode k = n; ; k = k.getNext()) {
                    // The frames of the vanilla body do not fit the shell; the cancel check gets its own below.
                    if (!(k instanceof FrameNode)) keep.add(k);
                    if (k == b.end) break;
                }
                if (b.cancelLabel != null) frames.put(b.cancelLabel, b.ci);
            }
            n = b.end.getNext();
        }
        Set<AbstractInsnNode> keptSet = new HashSet<>(keep);
        List<String> gone = new ArrayList<>();
        List<String> here = new ArrayList<>();
        for (AbstractInsnNode n = old.getFirst(); n != null; n = n.getNext()) {
            if (!(n instanceof MethodInsnNode)) continue;
            MethodInsnNode call = (MethodInsnNode) n;
            if (!call.owner.equals(cls.name) || !merged.contains(call.name + call.desc)) continue;
            (keptSet.contains(n) ? here : gone).add(m.name + ": " + call.name + call.desc);
        }
        InsnList shell = new InsnList();
        for (AbstractInsnNode n : keep) {
            old.remove(n);
            shell.add(n);
            if (n instanceof LabelNode && frames.containsKey(n)) shell.add(frame(cls.name, frames.get(n)));
        }
        shell.add(new InsnNode(Opcodes.RETURN));
        m.instructions = shell;
        m.tryCatchBlocks.clear();
        m.localVariables = null;
        synchronized (ShellTransformer.kept) {
            ShellTransformer.kept.addAll(here);
            lost.addAll(gone);
        }
        for (String s : here) Host.log("shell: kept " + s);
        for (String s : gone) Host.log("shell: lost, inside the body: " + s);
    }

    /**
     * One injector block: from the {@code new CallbackInfo} to the end of the cancel check (or of the handler call), inclusive.
     * {@code ci} is the local of the CallbackInfo; {@code cancelLabel} is the label that ends the cancel check, or null.
     */
    private static final class Block {
        final AbstractInsnNode end;
        final String handler;
        final int ci;
        final LabelNode cancelLabel;

        Block(AbstractInsnNode end, String handler, int ci, LabelNode cancelLabel) {
            this.end = end;
            this.handler = handler;
            this.ci = ci;
            this.cancelLabel = cancelLabel;
        }
    }

    /** The frame at the end of a cancel check: the host and the CallbackInfo are set, every other local is not. */
    private static FrameNode frame(String owner, int ci) {
        Object[] locals = new Object[ci + 1];
        Arrays.fill(locals, Opcodes.TOP);
        locals[0] = owner;
        locals[ci] = CALLBACK;
        return new FrameNode(Opcodes.F_NEW, locals.length, locals, 0, new Object[0]);
    }

    /** True when a backward jump encloses {@code n}: the block runs once per event of a loop the host drives itself (key and mouse events). */
    private static boolean inLoop(InsnList list, AbstractInsnNode n) {
        int at = list.indexOf(n);
        for (AbstractInsnNode k = n.getNext(); k != null; k = k.getNext()) {
            if (k instanceof JumpInsnNode && list.indexOf(((JumpInsnNode) k).label) <= at) return true;
        }
        return false;
    }

    /**
     * Mixin passes null for a CallbackInfo that its handler does not use: {@code aload_0; aconst_null; invokespecial handler}.
     * Null when {@code start} does not begin such a call.
     */
    private static Block nullBlock(AbstractInsnNode start, ClassNode cls, Set<String> merged) {
        if (start.getOpcode() != Opcodes.ALOAD || ((VarInsnNode) start).var != 0) return null;
        AbstractInsnNode a = start.getNext();
        AbstractInsnNode c = a == null ? null : a.getNext();
        if (a == null || a.getOpcode() != Opcodes.ACONST_NULL || !(c instanceof MethodInsnNode)) return null;
        MethodInsnNode call = (MethodInsnNode) c;
        if (!call.owner.equals(cls.name) || !merged.contains(call.name + call.desc) || !call.desc.equals("(L" + CALLBACK + ";)V")) return null;
        return new Block(call, call.name + call.desc, -1, null);
    }

    /**
     * Reads the block that starts at {@code start}: null when it is not one, or when it reads a local of the vanilla body (a
     * handler with captured locals), which the shell no longer has.
     */
    private static Block block(AbstractInsnNode start, ClassNode cls, Set<String> merged) {
        int ci = -1;
        MethodInsnNode call = null;
        int steps = 0;
        for (AbstractInsnNode n = start.getNext(); n != null && steps < MAX_BLOCK; n = n.getNext(), steps++) {
            if (n instanceof JumpInsnNode || n.getOpcode() == Opcodes.RETURN) return null;
            if (n.getOpcode() == Opcodes.ASTORE && ci < 0) ci = ((VarInsnNode) n).var;
            else if (n instanceof VarInsnNode && ((VarInsnNode) n).var != 0 && ((VarInsnNode) n).var != ci) return null;
            if (n instanceof MethodInsnNode) {
                MethodInsnNode c = (MethodInsnNode) n;
                if (c.owner.equals(cls.name) && merged.contains(c.name + c.desc)) {
                    call = c;
                    break;
                }
            }
        }
        if (call == null || ci < 0) return null;
        // Mixin's cancel check: aload ci; invokevirtual isCancelled; ifeq L; return; L:
        AbstractInsnNode a = nextReal(call);
        AbstractInsnNode b = a == null ? null : nextReal(a);
        AbstractInsnNode c = b == null ? null : nextReal(b);
        AbstractInsnNode d = c == null ? null : nextReal(c);
        if (a == null || a.getOpcode() != Opcodes.ALOAD || ((VarInsnNode) a).var != ci || !(b instanceof MethodInsnNode)
                || !((MethodInsnNode) b).owner.equals(CALLBACK) || !((MethodInsnNode) b).name.equals("isCancelled")
                || !(c instanceof JumpInsnNode) || c.getOpcode() != Opcodes.IFEQ || d == null || d.getOpcode() != Opcodes.RETURN) {
            return new Block(call, call.name + call.desc, ci, null);
        }
        LabelNode after = ((JumpInsnNode) c).label;
        if (d.getNext() != after) return null;
        return new Block(after, call.name + call.desc, ci, after);
    }

    private static AbstractInsnNode nextReal(AbstractInsnNode n) {
        for (n = n.getNext(); n != null; n = n.getNext()) {
            if (n.getOpcode() >= 0) return n;
        }
        return null;
    }

    private static boolean isMerged(MethodNode m) {
        return contains(m.visibleAnnotations) || contains(m.invisibleAnnotations);
    }

    @SuppressWarnings("unchecked")
    private static boolean contains(List<?> list) {
        if (list != null) {
            for (AnnotationNode a : (List<AnnotationNode>) list) {
                if (a.desc.equals(MERGED)) return true;
            }
        }
        return false;
    }

    /** ASM 5's node fields are raw lists. */
    @SuppressWarnings("unchecked")
    private static List<MethodNode> methods(ClassNode cls) {
        return cls.methods;
    }
}
