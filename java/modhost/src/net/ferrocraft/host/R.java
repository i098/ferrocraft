package net.ferrocraft.host;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import sun.misc.Unsafe;

/** Reflection over Minecraft and Forge classes by SRG name; the host has no compile-time link to them. */
final class R {
    private static final Unsafe UNSAFE;

    static {
        try {
            Field f = Unsafe.class.getDeclaredField("theUnsafe");
            f.setAccessible(true);
            UNSAFE = (Unsafe) f.get(null);
        } catch (ReflectiveOperationException e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    private R() {}

    static Class<?> cls(String name) {
        try {
            return Class.forName(name, true, Thread.currentThread().getContextClassLoader());
        } catch (ClassNotFoundException e) {
            throw new IllegalStateException(e);
        }
    }

    /** An instance without running a constructor. */
    static Object alloc(String cls) {
        try {
            return UNSAFE.allocateInstance(cls(cls));
        } catch (InstantiationException e) {
            throw new IllegalStateException(e);
        }
    }

    static Object neu(String cls, Object... args) {
        Class<?> c = cls(cls);
        for (Constructor<?> k : c.getDeclaredConstructors()) {
            if (fits(k.getParameterTypes(), args)) {
                try {
                    k.setAccessible(true);
                    return k.newInstance(args);
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(cls + " constructor failed", e);
                }
            }
        }
        throw new IllegalStateException("no constructor on " + cls + " for " + java.util.Arrays.toString(args) + "; has " + java.util.Arrays.toString(c.getDeclaredConstructors()));
    }

    static Object call(Object target, String name, Object... args) {
        Class<?> c = target instanceof Class ? (Class<?>) target : target.getClass();
        Object self = target instanceof Class ? null : target;
        for (Class<?> k = c; k != null; k = k.getSuperclass()) {
            for (Method m : k.getDeclaredMethods()) {
                if (m.getName().equals(name) && fits(m.getParameterTypes(), args)) {
                    try {
                        m.setAccessible(true);
                        return m.invoke(self, args);
                    } catch (ReflectiveOperationException e) {
                        throw new IllegalStateException(name + " failed", e.getCause() != null ? e.getCause() : e);
                    }
                }
            }
        }
        throw new IllegalStateException("no method " + name + " on " + c.getName());
    }

    static Object get(Object target, String name) {
        Field f = field(target, name);
        try {
            return f.get(target instanceof Class ? null : target);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    static void set(Object target, String name, Object value) {
        Field f = field(target, name);
        try {
            f.set(target instanceof Class ? null : target, value);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static Field field(Object target, String name) {
        Class<?> c = target instanceof Class ? (Class<?>) target : target.getClass();
        for (Class<?> k = c; k != null; k = k.getSuperclass()) {
            try {
                Field f = k.getDeclaredField(name);
                f.setAccessible(true);
                return f;
            } catch (NoSuchFieldException ignored) {
                // try the superclass
            }
        }
        throw new IllegalStateException("no field " + name + " on " + c.getName());
    }

    private static boolean fits(Class<?>[] types, Object[] args) {
        if (types.length != args.length) return false;
        for (int i = 0; i < types.length; i++) {
            if (args[i] == null) {
                if (types[i].isPrimitive()) return false;
            } else if (!box(types[i]).isInstance(args[i])) {
                return false;
            }
        }
        return true;
    }

    private static Class<?> box(Class<?> t) {
        if (!t.isPrimitive()) return t;
        if (t == int.class) return Integer.class;
        if (t == boolean.class) return Boolean.class;
        if (t == float.class) return Float.class;
        if (t == long.class) return Long.class;
        if (t == double.class) return Double.class;
        if (t == short.class) return Short.class;
        if (t == byte.class) return Byte.class;
        return Character.class;
    }
}
