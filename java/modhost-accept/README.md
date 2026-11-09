# java/modhost-accept

Owner: fc-modhost-pvp

Static acceptance check for the mod host. It does not run any mod.

`run.sh` downloads the public OneClient/Polyfrost 1.8.9 mod jars listed in `mods.txt` and the public MCP 1.8.9 SRG mapping. `Accept.java` then reads each jar's constant strings and checks that every `net/minecraft` class and every `func_`/`field_` member the mod names exists in vanilla 1.8.9. It does the same for the Mixin refmap targets. The names it finds are the minimum shape the stand-in Minecraft jar must provide (see `docs/mod-host.md`).

A PASS means the names exist in vanilla 1.8.9. It does not mean the mod runs on the mod host. `N/A (nothing named)` means the jar names no `net/minecraft` class or member (for example the OneConfig bootstrap loader), so nothing was checked. The tool exits 1 if any row is FAIL. Run `./run.sh` (JDK 8+ needed) to print the table.
