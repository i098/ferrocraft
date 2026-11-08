# testserver

Local vanilla 1.8.9 server for network and engine tests. It runs in offline mode on `127.0.0.1:25565`.
Never use it to reach a live server.

Owner: fc-session

1. Run `harness/testserver/run.sh`. It downloads `server.jar` from Mojang, checks the SHA-1, and starts the server.
   Set `FC_JAVA` when `java` is not on the PATH.
2. Run the integration test: `cargo test -p ferrocraft-net-client -- --ignored --nocapture`.
   Set `FC_SERVER` (default `127.0.0.1:25565`) to use another local address.
