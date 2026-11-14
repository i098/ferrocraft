package net.ferrocraft.host;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
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

    private static final List<String> kept = new ArrayList<>();
    private static final List<String> lost = new ArrayList<>();

    @Override
    public byte[] transform(String name, String transformedName, byte[] bytes) {
        if (bytes == null || !transformedName.equals("net.minecraft.client.Minecraft")) return bytes;
        return shell(bytes);
    }

    /** Handler calls kept in a shell, as {@code Class.method: handler}. */
    public static List<String> kept() {
        synchronized (kept) {
            return new ArrayList<>(kept);
        }
    }

    /** Handler calls that the vanilla body of a shell held and that are gone, as {@code Class.method: handler}. */
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
        Map<LabelNode, Integer> frames = new HashMap<>();
        collect(old, cls, merged, keep, frames);
        List<String> gone = new ArrayList<>();
        List<String> here = new ArrayList<>();
        sortCalls(old, cls, m.name, merged, new HashSet<>(keep), here, gone);
        m.instructions = build(old, keep, frames, cls.name);
        m.tryCatchBlocks.clear();
        m.localVariables = null;
        synchronized (ShellTransformer.kept) {
            ShellTransformer.kept.addAll(here);
            lost.addAll(gone);
        }
        for (String s : here) Host.log("shell: kept " + s);
        for (String s : gone) Host.log("shell: lost, inside the body: " + s);
    }

    /** Splits the handler calls of the vanilla body into those the shell keeps and those it loses. */
    private static void sortCalls(InsnList old, ClassNode cls, String method, Set<String> merged, Set<AbstractInsnNode> keptSet, List<String> here, List<String> gone) {
        for (AbstractInsnNode n = old.getFirst(); n != null; n = n.getNext()) {
            if (!isHandlerCall(n, cls, merged)) continue;
            MethodInsnNode call = (MethodInsnNode) n;
            (keptSet.contains(n) ? here : gone).add(cls.name + "." + method + ": " + call.name + call.desc);
        }
    }

    private static boolean isHandlerCall(AbstractInsnNode n, ClassNode cls, Set<String> merged) {
        if (!(n instanceof MethodInsnNode)) return false;
        MethodInsnNode c = (MethodInsnNode) n;
        return c.owner.equals(cls.name) && merged.contains(c.name + c.desc);
    }

    /** Picks the instructions of the blocks that no branch encloses; the cancel-check frame labels go to {@code frames}. */
    private static void collect(InsnList old, ClassNode cls, Set<String> merged, List<AbstractInsnNode> keep, Map<LabelNode, Integer> frames) {
        for (AbstractInsnNode n = old.getFirst(); n != null; ) {
            Block b = blockAt(n, cls, merged);
            if (b == null) {
                n = n.getNext();
                continue;
            }
            if (!inBranch(old, n, b.end)) keepBlock(n, b, keep, frames);
            n = b.end.getNext();
        }
    }

    private static Block blockAt(AbstractInsnNode n, ClassNode cls, Set<String> merged) {
        boolean callbackNew = n.getOpcode() == Opcodes.NEW && ((TypeInsnNode) n).desc.equals(CALLBACK);
        return callbackNew ? block(n, cls, merged) : nullBlock(n, cls, merged);
    }

    private static void keepBlock(AbstractInsnNode start, Block b, List<AbstractInsnNode> keep, Map<LabelNode, Integer> frames) {
        for (AbstractInsnNode k = start; ; k = k.getNext()) {
            // The frames of the vanilla body do not fit the shell; the cancel check gets its own below.
            if (!(k instanceof FrameNode)) keep.add(k);
            if (k == b.end) break;
        }
        if (b.cancelLabel != null) frames.put(b.cancelLabel, b.ci);
    }

    private static InsnList build(InsnList old, List<AbstractInsnNode> keep, Map<LabelNode, Integer> frames, String owner) {
        InsnList shell = new InsnList();
        for (AbstractInsnNode n : keep) {
            old.remove(n);
            shell.add(n);
            if (n instanceof LabelNode && frames.containsKey(n)) shell.add(frame(owner, frames.get(n)));
        }
        shell.add(new InsnNode(Opcodes.RETURN));
        return shell;
    }

    /**
     * One injector block: from the {@code new CallbackInfo} to the end of the cancel check (or of the handler call), inclusive.
     * {@code ci} is the local of the CallbackInfo; {@code cancelLabel} is the label that ends the cancel check, or null.
     */
    private static final class Block {
        final AbstractInsnNode end;
        final int ci;
        final LabelNode cancelLabel;

        Block(AbstractInsnNode end, int ci, LabelNode cancelLabel) {
            this.end = end;
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

    /** True when a jump from outside the block {@code first..last} passes over it (an if) or back to before it (a loop). */
    private static boolean inBranch(InsnList list, AbstractInsnNode first, AbstractInsnNode last) {
        int from = list.indexOf(first);
        int to = list.indexOf(last);
        for (AbstractInsnNode k = list.getFirst(); k != null; k = k.getNext()) {
            if (k instanceof JumpInsnNode && crosses(list, (JumpInsnNode) k, from, to)) return true;
        }
        return false;
    }

    private static boolean crosses(InsnList list, JumpInsnNode j, int from, int to) {
        int at = list.indexOf(j);
        int target = list.indexOf(j.label);
        return at < from && target > to || at > to && target <= from;
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
        return new Block(call, -1, null);
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
            else if (readsOther(n, ci)) return null;
            if (isHandlerCall(n, cls, merged)) {
                call = (MethodInsnNode) n;
                break;
            }
        }
        if (call == null || ci < 0) return null;
        JumpInsnNode cancel = cancelJump(call, ci);
        if (cancel == null) return new Block(call, ci, null);
        if (nextReal(cancel).getNext() != cancel.label) return null;
        return new Block(cancel.label, ci, cancel.label);
    }

    private static boolean readsOther(AbstractInsnNode n, int ci) {
        return n instanceof VarInsnNode && ((VarInsnNode) n).var != 0 && ((VarInsnNode) n).var != ci;
    }

    /** Mixin's cancel check after the call: {@code aload ci; invokevirtual isCancelled; ifeq L; return; L:}. Its {@code ifeq}, or null. */
    private static JumpInsnNode cancelJump(MethodInsnNode call, int ci) {
        AbstractInsnNode a = nextReal(call);
        AbstractInsnNode b = a == null ? null : nextReal(a);
        AbstractInsnNode c = b == null ? null : nextReal(b);
        AbstractInsnNode d = c == null ? null : nextReal(c);
        boolean load = a != null && a.getOpcode() == Opcodes.ALOAD && ((VarInsnNode) a).var == ci;
        boolean test = b instanceof MethodInsnNode && ((MethodInsnNode) b).owner.equals(CALLBACK) && ((MethodInsnNode) b).name.equals("isCancelled");
        boolean skip = c instanceof JumpInsnNode && c.getOpcode() == Opcodes.IFEQ && d != null && d.getOpcode() == Opcodes.RETURN;
        return load && test && skip ? (JumpInsnNode) c : null;
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
