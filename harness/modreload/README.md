# modreload

Owner: fc-modreload

Proof for mod hot reload (issue #825): a mod jar copied into `GAME_DIR/mods` while the engine is in a world makes its screen work with no client restart.

`run.sh` starts a local offline 1.8.9 server, Xvfb and the engine with the jars of `FC_BEFORE_MODS` (may be an empty folder), presses Right Shift (OneConfig opens its screen only when its jar is loaded) and saves `before.png`, copies the jars of `FC_ADD_MODS`, waits for the second `mods loaded` line, presses Right Shift again, saves `after.png` and checks that the engine process is the same and one mod host JVM runs. Local server only.

Env: `FERRO_LIB` (Java 8 libraries), `FERRO_JRE` (Java 8 `bin/java`), `FC_SERVER_JAR`, `FC_ENGINE`, `FC_BEFORE_MODS`, `FC_ADD_MODS` (folders of jars), `FC_OUT`, `FC_SETTLE` (seconds to wait before each picture, 20). Build `java/modhost` first (`java/modhost/build.sh`).
