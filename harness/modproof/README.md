# modproof

Owner: fc-mod-proof

Live proof sheet for the 22 Forge twins of the OneClient mods (OneConfig, 21 mods; Keystrokes and MixinBooter load as extra jars).

`run.sh` does this, with no input from you:

1. Builds `java/modhost`, starts a local vanilla 1.8.9 server in offline mode (`127.0.0.1:$FC_PORT`), an Xvfb display and the engine with all mod jars.
2. For each row of `steps.txt`: opens OneConfig with Right Shift, types the mod name into the search field, clicks the first card and saves the page as a screenshot.
3. Writes `$FC_OUT/sheet.txt` (one PASS or FAIL line per mod) and `$FC_OUT/shots/NN-<id>.png` (one screenshot per mod; `NN-<id>-grid.png` is the grid before the click).

Run it:

```sh
java/modhost-accept/run.sh /tmp/mods-cache     # downloads the mod jars into /tmp/mods-cache/mods
cargo build -p ferrocraft-engine
FERRO_JAVA_HOME=<JDK 8> FERRO_LIB=<Java 8 libraries> FC_MODS=/tmp/mods-cache/mods harness/modproof/run.sh
```

`FERRO_LIB` is the folder of `java/modhost/build.sh` and must hold `vecmath-1.5.2.jar`; `run.sh` stops at once with a message when it is missing. Other settings are at the top of `run.sh`. The script needs `Xvfb`, `xdotool`, `ffmpeg`, `python3` and `curl`.

## The steps file

One row per mod: `id | search text | note`. The search text is a part of the card title in the OneConfig Mods list, not
the mod id (some cards have a space, as `Chat Block`). A term that finds no card leaves the list empty and the row FAILs.

## What PASS means

A row passes when the card of the mod opens a page that differs from the filtered grid by `FC_DIFF_PX` pixels. This shows that the mod is loaded, is in the OneConfig list and has a config page that the host draws.
It does not show the effect of the mod in the game. The `oneconfig` row passes when Right Shift opens the Mods page.
A mod whose effect is not driven yet has no extra check. A later slice adds its own effect step to `steps.txt` (a key, or a click on a setting) and compares with this sheet.

The script never connects to a live server and never signs in to a Microsoft account. It exits 0 when the sheet has 22 rows, also when a row is FAIL. It exits 1 with no valid sheet when the mod host stops (a `mod host stopped` line in `engine.log`, or the engine process gone) or when the `oneconfig` row FAILs.
