# testserver

Local vanilla 1.8.9 server for network and engine tests. It runs in offline mode on `127.0.0.1:25565`. `FC_ONLINE=1` starts it with `online-mode=true` for the login test (`online_mode_login_joins_the_local_server`: set `FC_ACCESS_TOKEN`, `FC_UUID`, `FC_USERNAME`).
Never use it to reach a live server.

Owner: fc-session

1. Run `harness/testserver/run.sh`. It downloads `server.jar` from Mojang, checks the SHA-1, and starts the server.
   Set `FC_JAVA` when `java` is not on the PATH.
2. Run the integration test: `cargo test -p ferrocraft-net-client -- --ignored --nocapture`.
   Set `FC_SERVER` (default `127.0.0.1:25565`) to use another local address.
