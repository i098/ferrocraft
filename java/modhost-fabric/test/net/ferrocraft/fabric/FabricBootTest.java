package net.ferrocraft.fabric;

import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Boots the Fabric host the way {@code ModHost::start} does (Knot, the host jar as Java agent), with this class as the peer in
 * place of the engine, and checks the log: every expected mod id has its entrypoint called, the JVM sends the hello, and no
 * Mixin error is in the output. A JVM that stops before the hello is a FAIL, never a PASS.
 *
 * <p>Arguments: java, class path, host jar, name table, game dir, assets dir, then the expected mod ids (the ids of the
 * entrypoint providers, one per argument).
 */
public final class FabricBootTest {
    private static final long HELLO_MILLIS = 240_000;
    private static final String[] MIXIN_ERRORS = {"MixinApplyError", "InvalidMixinException", "MixinTransformerError",
            "Mixin apply failed", "Mixin transformation of", "MixinPrepareError", "Critical injection failure"};

    private FabricBootTest() {}

    public static void main(String[] a) throws Exception {
        String java = a[0], cp = a[1], jar = a[2], tiny = a[3], game = a[4], assets = a[5];
        List<String> expect = Arrays.asList(a).subList(6, a.length);
        String token = Long.toHexString(System.nanoTime()) + Long.toHexString(System.currentTimeMillis());
        List<String> log = new CopyOnWriteArrayList<>();
        try (ServerSocket server = new ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"))) {
            server.setSoTimeout(500);
            List<String> cmd = new ArrayList<>(Arrays.asList(java, "-Dferro.bridge=127.0.0.1:" + server.getLocalPort(),
                    "-javaagent:" + jar, "-Dferro.tiny=" + tiny, "-Dferro.gameDir=" + game, "-cp", cp,
                    "net.fabricmc.loader.impl.launch.knot.KnotClient", "--gameDir", game, "--assetsDir", assets,
                    "--version", "1.8.9", "--accessToken", "x"));
            ProcessBuilder pb = new ProcessBuilder(cmd).redirectErrorStream(true);
            pb.environment().put("FERROCRAFT_BRIDGE_TOKEN", token);
            Process p = pb.start();
            Thread pump = pump(p.getInputStream(), log);
            String hello = null;
            long end = System.currentTimeMillis() + HELLO_MILLIS;
            try {
                while (hello == null && System.currentTimeMillis() < end && p.isAlive()) {
                    Socket s;
                    try {
                        s = server.accept();
                    } catch (java.net.SocketTimeoutException e) {
                        continue;
                    }
                    try (Socket c = s) {
                        hello = readHello(c, token);
                    } catch (IOException e) {
                        log.add("hello failed: " + e.getMessage());
                        break;
                    }
                }
            } finally {
                p.destroy();
                p.waitFor();
                pump.join(5000);
            }
            report(log, expect, hello, p.exitValue());
        }
    }

    private static String readHello(Socket s, String token) throws IOException {
        s.setSoTimeout(30_000);
        InputStream in = s.getInputStream();
        if (in.read() != 'H') throw new IOException("the first byte is not the hello tag");
        byte[] t = new byte[token.length()];
        int n = 0;
        while (n < t.length) {
            int r = in.read(t, n, t.length - n);
            if (r < 0) throw new IOException("the connection closed inside the hello");
            n += r;
        }
        if (!new String(t, StandardCharsets.US_ASCII).equals(token)) throw new IOException("the hello token differs");
        return "hello";
    }

    private static Thread pump(InputStream in, List<String> log) {
        Thread t = new Thread(() -> {
            try (BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
                for (String line; (line = r.readLine()) != null; ) {
                    log.add(line);
                    System.out.println("  | " + line);
                }
            } catch (IOException e) {
                log.add("[test] output read failed: " + e);
            }
        });
        t.setDaemon(true);
        t.start();
        return t;
    }

    private static void report(List<String> log, List<String> expect, String hello, int exit) {
        boolean ok = true;
        for (String id : expect) {
            boolean called = log.stream().anyMatch(l -> l.startsWith("[ferro-host] entrypoint ") && l.endsWith(": " + id));
            System.out.println((called ? "PASS" : "FAIL") + " entrypoint " + id);
            ok &= called;
        }
        boolean loaded = log.contains("[ferro-host] mods loaded");
        System.out.println((loaded ? "PASS" : "FAIL") + " mods loaded");
        boolean mixin = log.stream().anyMatch(l -> Arrays.stream(MIXIN_ERRORS).anyMatch(l::contains));
        System.out.println((mixin ? "FAIL" : "PASS") + " no Mixin error in the log");
        String why = log.stream().filter(l -> l.startsWith("hello failed: ")).findFirst().orElse("the JVM stopped before it connected");
        System.out.println((hello != null ? "PASS" : "FAIL") + " hello" + (hello != null ? "" : ": " + why + ", exit status " + exit));
        ok &= loaded && !mixin && hello != null;
        System.out.println(ok ? "FabricBootTest passed" : "FabricBootTest FAILED");
        System.exit(ok ? 0 : 1);
    }
}
