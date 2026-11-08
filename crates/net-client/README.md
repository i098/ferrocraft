# net-client

Client network session for 1.8.9 (protocol 47). It logs in to an offline-mode server,
answers keepalive on its reader thread, and turns play packets into `NetEvent` values.

Owner: fc-session

Test against `harness/testserver/run.sh`; see `tests/server.rs` (ignored by default).
