package net.ferrocraft.boot;

import java.io.File;
import java.lang.reflect.Method;
import java.net.URL;
import java.net.URLClassLoader;
import java.util.ArrayList;
import java.util.List;

/**
 * Main class of the ferrocraft-1.8.9 version. Launchers differ in classpath order (the official
 * launcher puts the vanilla jar last, portablemc first), so this class builds its own loader with
 * our jar first and runs {@link Engine} in it. Stand-in classes then win on every launcher.
 */
public final class Main {
    private Main() {}

    public static void main(String[] args) throws Exception {
        URL self = Main.class.getProtectionDomain().getCodeSource().getLocation();
        List<URL> urls = new ArrayList<URL>();
        urls.add(self);
        for (String entry : System.getProperty("java.class.path").split(File.pathSeparator)) {
            URL u = new File(entry).toURI().toURL();
            if (!u.equals(self)) urls.add(u);
        }
        ClassLoader parent = ClassLoader.getSystemClassLoader().getParent();
        URLClassLoader loader = new URLClassLoader(urls.toArray(new URL[0]), parent);
        Thread.currentThread().setContextClassLoader(loader);
        Method run = loader.loadClass("net.ferrocraft.boot.Engine").getMethod("run", String[].class);
        System.exit((Integer) run.invoke(null, (Object) args));
    }
}
