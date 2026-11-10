package net.ferrocraft.host;

import net.minecraft.launchwrapper.IClassTransformer;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.TypeInsnNode;
import org.objectweb.asm.tree.VarInsnNode;

/**
 * Applied after FML's deobfuscation, coremods and Mixin, so class and method names are the SRG
 * names mods see. It does two jobs:
 *
 * <ul>
 *   <li>Replaces the bodies of the OpenGL and window entry points of LWJGL 2 with no-ops or reads of
 *       {@link Input}. No native library or GL context exists in this JVM.
 *   <li>Replaces the bodies of the Gui and FontRenderer draw methods with calls to {@link Sink}.
 * </ul>
 */
public class BridgeTransformer implements IClassTransformer {
    private static final String SINK = "net/ferrocraft/host/Sink";
    private static final String INPUT = "net/ferrocraft/host/Input";

    /**
     * Minecraft's own classes arrive obfuscated until FML's deobfuscation transformer has run, so they need a
     * second instance registered after it; this one handles every other class.
     */
    protected boolean minecraftClasses() {
        return false;
    }

    @Override
    public byte[] transform(String name, String transformedName, byte[] bytes) {
        if (bytes == null) return null;
        String n = transformedName.replace('.', '/');
        if (n.startsWith("net/minecraft/") != minecraftClasses()) return bytes;
        if (n.startsWith("org/lwjgl/opengl/")) {
            return isGl(n) ? patch(bytes, BridgeTransformer::glMethod) : bytes;
        }
        switch (n) {
            case "org/lwjgl/input/Keyboard":
                return patch(bytes, BridgeTransformer::keyboardMethod);
            case "org/lwjgl/input/Mouse":
                return patch(bytes, BridgeTransformer::mouseMethod);
            case "org/lwjgl/opengl/Display":
                return patch(bytes, BridgeTransformer::displayMethod);
            case "net/minecraftforge/fml/client/SplashProgress":
                return patch(bytes, BridgeTransformer::splashMethod);
            case "org/lwjgl/Sys":
                return patch(bytes, BridgeTransformer::sysMethod);
            case "net/minecraft/client/gui/Gui":
                return patch(bytes, BridgeTransformer::guiMethod);
            case "cc/polyfrost/oneconfig/internal/renderer/LwjglManagerImpl":
                return patch(bytes, BridgeTransformer::lwjglManagerMethod);
            case "cc/polyfrost/oneconfig/renderer/NanoVGHelper":
                return withoutLwjgl3Methods(bytes);
            case "cc/polyfrost/oneconfig/platform/impl/GLPlatformImpl":
                return patch(bytes, BridgeTransformer::glPlatformMethod);
            case "net/minecraft/entity/Entity":
                return patch(bytes, BridgeTransformer::entityMethod);
            case "net/minecraft/client/entity/AbstractClientPlayer":
                return patch(bytes, BridgeTransformer::playerMethod);
            case "net/minecraft/client/renderer/EntityRenderer":
                return patch(bytes, BridgeTransformer::entityRendererMethod);
            case "net/minecraft/client/gui/FontRenderer":
                return patch(bytes, BridgeTransformer::fontMethod);
            case "net/minecraft/client/Minecraft":
                return patch(bytes, BridgeTransformer::minecraftMethod);
            default:
                return bytes;
        }
    }

    /** GL11 to GL45 and the ARB, EXT and NV extension classes; not Display, GLContext or the helpers. */
    private static boolean isGl(String n) {
        String s = n.substring("org/lwjgl/opengl/".length());
        return s.matches("GL\\d+") || s.startsWith("ARB") || s.startsWith("EXT") || s.startsWith("NV") || s.startsWith("AMD");
    }

    private interface Rule {
        /** The new body for the method, or null to leave it. */
        InsnList body(MethodNode m);
    }

    private static byte[] patch(byte[] bytes, Rule rule) {
        ClassNode cn = new ClassNode();
        new ClassReader(bytes).accept(cn, 0);
        boolean changed = false;
        for (Object o : cn.methods) {
            MethodNode m = (MethodNode) o;
            if (m.name.equals("<init>")) continue;
            InsnList body = rule.body(m);
            if (body == null) continue;
            m.instructions = body;
            m.tryCatchBlocks.clear();
            m.localVariables = null;
            m.access &= ~Opcodes.ACC_NATIVE;
            changed = true;
        }
        if (!changed) return bytes;
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        cn.accept(cw);
        return cw.toByteArray();
    }

    private static boolean isStatic(MethodNode m) {
        return (m.access & Opcodes.ACC_STATIC) != 0;
    }

    private static InsnList glMethod(MethodNode m) {
        if (!isStatic(m) || m.name.equals("<clinit>")) return null;
        InsnList l = new InsnList();
        switch (m.name + m.desc) {
            case "glPushMatrix()V":
                call(l, SINK, "push", "()V");
                break;
            case "glPopMatrix()V":
                call(l, SINK, "pop", "()V");
                break;
            case "glLoadIdentity()V":
                call(l, SINK, "identity", "()V");
                break;
            case "glMatrixMode(I)V":
                l.add(new VarInsnNode(Opcodes.ILOAD, 0));
                call(l, SINK, "matrixMode", "(I)V");
                break;
            case "glTranslatef(FFF)V":
                l.add(new VarInsnNode(Opcodes.FLOAD, 0));
                l.add(new VarInsnNode(Opcodes.FLOAD, 1));
                call(l, SINK, "translate", "(FF)V");
                break;
            case "glTranslated(DDD)V":
                l.add(new VarInsnNode(Opcodes.DLOAD, 0));
                l.add(new InsnNode(Opcodes.D2F));
                l.add(new VarInsnNode(Opcodes.DLOAD, 2));
                l.add(new InsnNode(Opcodes.D2F));
                call(l, SINK, "translate", "(FF)V");
                break;
            case "glScalef(FFF)V":
                l.add(new VarInsnNode(Opcodes.FLOAD, 0));
                call(l, SINK, "scale", "(F)V");
                break;
            case "glScaled(DDD)V":
                l.add(new VarInsnNode(Opcodes.DLOAD, 0));
                l.add(new InsnNode(Opcodes.D2F));
                call(l, SINK, "scale", "(F)V");
                break;
            default:
                break;
        }
        returnDefault(l, Type.getReturnType(m.desc));
        return l;
    }

    private static InsnList keyboardMethod(MethodNode m) {
        switch (m.name + m.desc) {
            case "isKeyDown(I)Z":
                return forward(m, INPUT, "isKeyDown", "(I)Z");
            case "isCreated()Z":
                return constant(true);
            case "next()Z":
                return forward(m, INPUT, "nextKey", "()Z");
            case "getEventKey()I":
                return forward(m, INPUT, "eventKey", "()I");
            case "getEventKeyState()Z":
                return forward(m, INPUT, "eventKeyDown", "()Z");
            case "isRepeatEvent()Z":
                return forward(m, INPUT, "eventRepeat", "()Z");
            case "enableRepeatEvents(Z)V":
                return forward(m, INPUT, "repeatEvents", "(Z)V");
            case "areRepeatEventsEnabled()Z":
                return forward(m, INPUT, "repeatEventsEnabled", "()Z");
            case "getEventCharacter()C":
                return constant(0);
            case "getEventNanoseconds()J":
                return defaultBody(m);
            default:
                return null;
        }
    }

    private static InsnList mouseMethod(MethodNode m) {
        switch (m.name + m.desc) {
            case "isButtonDown(I)Z":
                return forward(m, INPUT, "isButtonDown", "(I)Z");
            case "getX()I":
                return forward(m, INPUT, "mouseX", "()I");
            case "getY()I":
                return forward(m, INPUT, "mouseY", "()I");
            case "isCreated()Z":
                return constant(true);
            case "next()Z":
                return forward(m, INPUT, "nextMouse", "()Z");
            case "getEventButton()I":
                return forward(m, INPUT, "eventButton", "()I");
            case "getEventButtonState()Z":
                return forward(m, INPUT, "eventButtonDown", "()Z");
            case "getEventDWheel()I":
                return forward(m, INPUT, "eventDWheel", "()I");
            case "getEventX()I":
                return forward(m, INPUT, "eventX", "()I");
            case "getEventY()I":
                return forward(m, INPUT, "eventY", "()I");
            case "getEventDX()I":
                return forward(m, INPUT, "eventDX", "()I");
            case "getEventDY()I":
                return forward(m, INPUT, "eventDY", "()I");
            case "getEventNanoseconds()J":
                return defaultBody(m);
            case "isGrabbed()Z":
                return constant(false);
            case "getDX()I":
            case "getDY()I":
            case "getDWheel()I":
                return constant(0);
            default:
                return null;
        }
    }

    /** LWJGL's Sys loads the native library when the class initialises; nothing here needs it. */
    private static InsnList sysMethod(MethodNode m) {
        InsnList l = new InsnList();
        switch (m.name + m.desc) {
            case "<clinit>()V":
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            case "getTime()J":
                l.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "java/lang/System", "currentTimeMillis", "()J", false));
                l.add(new InsnNode(Opcodes.LRETURN));
                return l;
            case "getTimerResolution()J":
                l.add(new org.objectweb.asm.tree.LdcInsnNode(Long.valueOf(1000L)));
                l.add(new InsnNode(Opcodes.LRETURN));
                return l;
            default:
                return isStatic(m) ? defaultBody(m) : null;
        }
    }

    /** Forge's splash screen runs its own GL thread; Rust draws the screen instead. */
    private static InsnList splashMethod(MethodNode m) {
        boolean entry = isStatic(m) && m.desc.equals("()V") && (m.name.equals("start") || m.name.equals("pause") || m.name.equals("resume") || m.name.equals("finish"));
        return entry ? defaultBody(m) : null;
    }

    private static InsnList defaultBody(MethodNode m) {
        InsnList l = new InsnList();
        returnDefault(l, Type.getReturnType(m.desc));
        return l;
    }

    private static InsnList displayMethod(MethodNode m) {
        switch (m.name + m.desc) {
            case "<clinit>()V": {
                InsnList l = new InsnList();
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            }
            case "getWidth()I":
                return forward(m, INPUT, "width", "()I");
            case "getHeight()I":
                return forward(m, INPUT, "height", "()I");
            case "isCreated()Z":
            case "isActive()Z":
            case "isVisible()Z":
                return constant(true);
            case "isCloseRequested()Z":
                return constant(false);
            default:
                return null;
        }
    }

    private static InsnList guiMethod(MethodNode m) {
        InsnList l = new InsnList();
        switch (m.name + m.desc) {
            case "func_73734_a(IIIII)V": // Gui.drawRect
                loads(l, m, 0);
                call(l, SINK, "rect", "(IIIII)V");
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            case "func_73733_a(IIIIII)V": // Gui.drawGradientRect
                loads(l, m, 1);
                call(l, SINK, "gradient", "(IIIIII)V");
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            default:
                return null;
        }
    }

    /**
     * Minecraft.displayGuiScreen needs the window, the sound handler and the mouse grab, so {@link Host#displayScreen}
     * does what mods read. setIngameFocus, which GuiScreen.keyTyped calls after Escape, needs the mouse helper.
     */
    private static InsnList minecraftMethod(MethodNode m) {
        InsnList l = new InsnList();
        switch (m.name + m.desc) {
            case "func_147108_a(Lnet/minecraft/client/gui/GuiScreen;)V": // displayGuiScreen
                l.add(new VarInsnNode(Opcodes.ALOAD, 1));
                call(l, "net/ferrocraft/host/Host", "displayScreen", "(Ljava/lang/Object;)V");
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            case "func_71381_h()V": // setIngameFocus
                l.add(new InsnNode(Opcodes.RETURN));
                return l;
            default:
                return null;
        }
    }

    /** FontRenderer.drawString(String, float, float, int, boolean): every draw method ends here. */
    private static InsnList fontMethod(MethodNode m) {
        if (!m.name.equals("func_175065_a") || !m.desc.equals("(Ljava/lang/String;FFIZ)I")) return null;
        InsnList l = new InsnList();
        l.add(new VarInsnNode(Opcodes.ALOAD, 1));
        l.add(new VarInsnNode(Opcodes.FLOAD, 2));
        l.add(new VarInsnNode(Opcodes.FLOAD, 3));
        l.add(new VarInsnNode(Opcodes.ILOAD, 4));
        l.add(new VarInsnNode(Opcodes.ILOAD, 5));
        call(l, SINK, "text", "(Ljava/lang/String;FFIZ)V");
        l.add(new VarInsnNode(Opcodes.FLOAD, 2));
        l.add(new InsnNode(Opcodes.F2I));
        l.add(new VarInsnNode(Opcodes.ALOAD, 0));
        l.add(new VarInsnNode(Opcodes.ALOAD, 1));
        // func_78256_a is FontRenderer.getStringWidth, which returns 0 for null.
        l.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, "net/minecraft/client/gui/FontRenderer", "func_78256_a", "(Ljava/lang/String;)I", false));
        l.add(new InsnNode(Opcodes.IADD));
        l.add(new InsnNode(Opcodes.IRETURN));
        return l;
    }

    /** Loads every argument of the method onto the stack, starting at local slot `first`. */
    private static void loads(InsnList l, MethodNode m, int first) {
        Type[] args = Type.getArgumentTypes(m.desc);
        int slot = isStatic(m) ? 0 : 1;
        if (first > slot) slot = first;
        for (Type a : args) {
            l.add(new VarInsnNode(a.getOpcode(Opcodes.ILOAD), slot));
            slot += a.getSize();
        }
    }

    private static InsnList forward(MethodNode m, String owner, String name, String desc) {
        InsnList l = new InsnList();
        loads(l, m, 0);
        call(l, owner, name, desc);
        l.add(new InsnNode(Type.getReturnType(m.desc).getOpcode(Opcodes.IRETURN)));
        return l;
    }

    /** Entity.getBrightness reads the world, which the stand-in player does not have; full light. */
    private static InsnList entityMethod(MethodNode m) {
        if (!(m.name + m.desc).equals("func_70013_c(F)F")) return null;
        InsnList l = new InsnList();
        l.add(new InsnNode(Opcodes.FCONST_1));
        l.add(new InsnNode(Opcodes.FRETURN));
        return l;
    }

    /** AbstractClientPlayer.isSpectator reads the network player info, which does not exist here. */
    private static InsnList playerMethod(MethodNode m) {
        return (m.name + m.desc).equals("func_175149_v()Z") ? constant(false) : null;
    }

    /**
     * EntityRenderer.loadShader builds a ShaderGroup from the resource manager and a framebuffer, which feed
     * the GPU post pass (OneConfig's blur). Rust draws, so there is no pass: no group loads, and mods read
     * getShaderGroup as null, the state of a client without shader support.
     */
    private static InsnList entityRendererMethod(MethodNode m) {
        if (!(m.name + m.desc).equals("func_175069_a(Lnet/minecraft/util/ResourceLocation;)V")) return null;
        InsnList l = new InsnList();
        l.add(new InsnNode(Opcodes.RETURN));
        return l;
    }

    /**
     * NanoVGHelper declares two methods on NVGColor, a class of OneConfig's isolated LWJGL 3, which the game
     * class loader cannot load next to LWJGL 2: a proxy of the interface fails on them. Nothing here uses them.
     */
    private static byte[] withoutLwjgl3Methods(byte[] bytes) {
        ClassNode cn = new ClassNode();
        new ClassReader(bytes).accept(cn, 0);
        if (!cn.methods.removeIf(m -> ((MethodNode) m).desc.contains("Lorg/lwjgl/"))) return bytes;
        ClassWriter cw = new ClassWriter(0);
        cn.accept(cw);
        return cw.toByteArray();
    }

    /**
     * OneConfig defines its renderer implementations in its own class loader, out of reach of transformers, and
     * that loader's LWJGL 3 cannot start here: the manager's accessors return {@link Nvg#stand} proxies instead.
     */
    private static InsnList lwjglManagerMethod(MethodNode m) {
        Type ret = Type.getReturnType(m.desc);
        boolean accessor = m.desc.startsWith("()") && ret.getSort() == Type.OBJECT
                && ret.getInternalName().startsWith("cc/polyfrost/oneconfig/renderer/")
                && m.name.matches("get(NanoVGHelper|ScissorHelper|AssetHelper|FontHelper|TinyFD)");
        if (!accessor) return null;
        InsnList l = new InsnList();
        l.add(new org.objectweb.asm.tree.LdcInsnNode(ret.getClassName()));
        call(l, "net/ferrocraft/host/Nvg", "stand", "(Ljava/lang/String;)Ljava/lang/Object;");
        l.add(new TypeInsnNode(Opcodes.CHECKCAST, ret.getInternalName()));
        l.add(new InsnNode(Opcodes.ARETURN));
        return l;
    }

    /** OneConfig's GL platform: its rect goes to {@link Nvg#glRect}. */
    private static InsnList glPlatformMethod(MethodNode m) {
        return (m.name + m.desc).equals("drawRect(FFFFI)V") ? forward(m, "net/ferrocraft/host/Nvg", "glRect", m.desc) : null;
    }

    private static InsnList constant(Object v) {
        InsnList l = new InsnList();
        l.add(new InsnNode(v instanceof Boolean && (Boolean) v ? Opcodes.ICONST_1 : Opcodes.ICONST_0));
        l.add(new InsnNode(Opcodes.IRETURN));
        return l;
    }

    private static void call(InsnList l, String owner, String name, String desc) {
        l.add(new MethodInsnNode(Opcodes.INVOKESTATIC, owner, name, desc, false));
    }

    private static void returnDefault(InsnList l, Type t) {
        switch (t.getSort()) {
            case Type.VOID:
                l.add(new InsnNode(Opcodes.RETURN));
                break;
            case Type.LONG:
                l.add(new InsnNode(Opcodes.LCONST_0));
                l.add(new InsnNode(Opcodes.LRETURN));
                break;
            case Type.FLOAT:
                l.add(new InsnNode(Opcodes.FCONST_0));
                l.add(new InsnNode(Opcodes.FRETURN));
                break;
            case Type.DOUBLE:
                l.add(new InsnNode(Opcodes.DCONST_0));
                l.add(new InsnNode(Opcodes.DRETURN));
                break;
            case Type.OBJECT:
            case Type.ARRAY:
                l.add(new InsnNode(Opcodes.ACONST_NULL));
                l.add(new InsnNode(Opcodes.ARETURN));
                break;
            default:
                l.add(new InsnNode(Opcodes.ICONST_0));
                l.add(new InsnNode(Opcodes.IRETURN));
                break;
        }
    }
}
