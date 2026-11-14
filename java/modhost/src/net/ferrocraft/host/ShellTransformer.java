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
import org.objectweb.asm.tree.LookupSwitchInsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.TableSwitchInsnNode;
import org.objectweb.asm.tree.TryCatchBlockNode;
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

    /**
     * Turns the loop methods of the class into shells; with no mod hook there, a shell holds only the return. Each shell
     * {@code name} calls {@code name$head} (the handler blocks before the body) and then {@code name$tail} (the return blocks).
     */
    static byte[] shell(byte[] bytes) {
        ClassNode cls = new ClassNode();
        new ClassReader(bytes).accept(cls, ClassReader.SKIP_DEBUG);
        Set<String> merged = new HashSet<>();
        List<MethodNode> shells = new ArrayList<>();
        for (MethodNode m : methods(cls)) {
            if (isMerged(m)) merged.add(m.name + m.desc);
            if (SHELLS.contains(m.name + m.desc)) shells.add(m);
        }
        for (MethodNode m : shells) methods(cls).addAll(shell(cls, m, merged));
        ClassWriter out = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cls.accept(out);
        return out.toByteArray();
    }

    /** Rewrites {@code m} into a call of its two parts, which it returns. */
    private static List<MethodNode> shell(ClassNode cls, MethodNode m, Set<String> merged) {
        InsnList old = m.instructions;
        List<AbstractInsnNode> head = new ArrayList<>();
        List<AbstractInsnNode> tail = new ArrayList<>();
        Map<LabelNode, Integer> frames = new HashMap<>();
        collect(m, cls, merged, head, tail, frames);
        List<AbstractInsnNode> all = new ArrayList<>(head);
        all.addAll(tail);
        List<String> gone = new ArrayList<>();
        List<String> here = new ArrayList<>();
        sortCalls(old, cls, m.name, merged, new HashSet<>(all), here, gone);
        List<MethodNode> parts = new ArrayList<>();
        parts.add(part(m, "$head", build(old, head, frames, cls.name)));
        parts.add(part(m, "$tail", build(old, tail, frames, cls.name)));
        m.instructions = callParts(cls.name, m.name);
        m.tryCatchBlocks.clear();
        m.localVariables = null;
        synchronized (ShellTransformer.kept) {
            ShellTransformer.kept.addAll(here);
            lost.addAll(gone);
        }
        for (String s : here) Host.log("shell: kept " + s);
        for (String s : gone) Host.log("shell: lost, inside the body: " + s);
        return parts;
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

    /**
     * Picks the instructions of the blocks that no branch encloses. A block the method returns right after is a return block
     * and goes to {@code tail}; the others go to {@code head}. The cancel-check frame labels go to {@code frames}.
     */
    private static void collect(MethodNode m, ClassNode cls, Set<String> merged, List<AbstractInsnNode> head, List<AbstractInsnNode> tail, Map<LabelNode, Integer> frames) {
        for (AbstractInsnNode n = m.instructions.getFirst(); n != null; ) {
            Block b = blockAt(n, cls, merged);
            if (b == null) {
                n = n.getNext();
                continue;
            }
            if (!inBranch(m, n, b.end)) keepBlock(n, b, isReturnBlock(b, cls, merged) ? tail : head, frames);
            n = b.end.getNext();
        }
    }

    /** A return block is followed by nothing but other blocks before the return of the method. */
    private static boolean isReturnBlock(Block b, ClassNode cls, Set<String> merged) {
        AbstractInsnNode next = nextReal(b.end);
        Block more = next == null ? null : blockAt(next, cls, merged);
        while (more != null) {
            next = nextReal(more.end);
            more = next == null ? null : blockAt(next, cls, merged);
        }
        return next != null && next.getOpcode() == Opcodes.RETURN;
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

    /** The shell body: call the head part, then the tail part. */
    private static InsnList callParts(String owner, String name) {
        InsnList list = new InsnList();
        for (String part : new String[] {"$head", "$tail"}) {
            list.add(new VarInsnNode(Opcodes.ALOAD, 0));
            list.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, owner, name + part, "()V", false));
        }
        list.add(new InsnNode(Opcodes.RETURN));
        return list;
    }

    private static MethodNode part(MethodNode m, String suffix, InsnList body) {
        MethodNode p = new MethodNode(m.access, m.name + suffix, m.desc, null, null);
        p.instructions = body;
        return p;
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

    /** True when a branch encloses the block {@code first..last}: a jump or switch passes over it (an if, a case) or back to before it (a loop), or it lies in a catch handler. */
    private static boolean inBranch(MethodNode m, AbstractInsnNode first, AbstractInsnNode last) {
        InsnList list = m.instructions;
        int from = list.indexOf(first);
        int to = list.indexOf(last);
        for (AbstractInsnNode k = list.getFirst(); k != null; k = k.getNext()) {
            if (crosses(list, k, from, to)) return true;
        }
        return inHandler(m, from);
    }

    private static boolean crosses(InsnList list, AbstractInsnNode k, int from, int to) {
        int at = list.indexOf(k);
        for (LabelNode label : targets(k)) {
            int target = list.indexOf(label);
            if (at < from && target > to || at > to && target <= from) return true;
        }
        return false;
    }

    @SuppressWarnings("unchecked")
    private static List<LabelNode> targets(AbstractInsnNode k) {
        List<LabelNode> out = new ArrayList<>();
        if (k instanceof JumpInsnNode) out.add(((JumpInsnNode) k).label);
        if (k instanceof TableSwitchInsnNode) {
            out.add(((TableSwitchInsnNode) k).dflt);
            out.addAll(((TableSwitchInsnNode) k).labels);
        }
        if (k instanceof LookupSwitchInsnNode) {
            out.add(((LookupSwitchInsnNode) k).dflt);
            out.addAll(((LookupSwitchInsnNode) k).labels);
        }
        return out;
    }

    /** A catch handler runs from its label to the target of the goto that ends the try body, or to the end of the method. */
    @SuppressWarnings("unchecked")
    private static boolean inHandler(MethodNode m, int from) {
        InsnList list = m.instructions;
        for (TryCatchBlockNode t : (List<TryCatchBlockNode>) m.tryCatchBlocks) {
            int h = list.indexOf(t.handler);
            AbstractInsnNode prev = t.handler.getPrevious();
            while (prev != null && prev.getOpcode() < 0) prev = prev.getPrevious();
            int end = prev != null && prev.getOpcode() == Opcodes.GOTO ? list.indexOf(((JumpInsnNode) prev).label) : list.size();
            if (from >= h && from < (end > h ? end : list.size())) return true;
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
