#!/bin/sh
# End-to-end proof. Needs: JDK 8 (JAVA_HOME), cargo, portablemc >= 5, xvfb-run, uv.
# Usage: smoke/run.sh   (downloads vanilla 1.8.9 into a temp dir; offline account, no sign-in)
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
: "${JAVA_HOME:?set JAVA_HOME to a JDK 8}"
export PATH="$JAVA_HOME/bin:$PATH"

cargo build --manifest-path "$root/crates/launcher/Cargo.toml"
tgt=$(cargo metadata --manifest-path "$root/crates/launcher/Cargo.toml" --format-version 1 --no-deps | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
"$root/java/launcher/build.sh" "$work/boot.jar"
"$tgt/debug/ferrocraft-launcher" install --minecraft-dir "$work/mc" --boot-jar "$work/boot.jar" --engine "$tgt/debug/ferrocraft-engine-window"

# Polls X until a top-level window with the given title exists.
finder='import sys, time
from Xlib import display
title, end = sys.argv[1], time.time() + float(sys.argv[2])
d = display.Display()
while time.time() < end:
    for w in d.screen().root.query_tree().children:
        if w.get_wm_name() == title:
            g = w.get_geometry()
            print("X window found: %r %dx%d" % (title, g.width, g.height))
            sys.exit(0)
    time.sleep(0.5)
sys.exit("X window not found")'

# Headless launcher-equivalent: portablemc runs versions/ferrocraft-1.8.9 (inherits 1.8.9), offline.
FERROCRAFT_WINDOW_SECS=20 xvfb-run -a -s "-screen 0 1280x720x24" sh -c '
  portablemc --output machine start --main-dir "$1" --mc-dir "$1" --jvm "$JAVA_HOME/bin/java" \
    --fetch-exclude ferrocraft-1.8.9 --username Steve ferrocraft-1.8.9 > "$2/game.log" 2>&1 &
  pid=$!
  uv run --quiet --with python-xlib python3 -c "$3" "Ferrocraft 1.8.9" 90
  wait $pid
' sh "$work/mc" "$work" "$finder" || { cat "$work/game.log"; exit 1; }
grep -E "^log_raw|^terminated|^launched" "$work/game.log"
grep -q 'ClientBrandRetriever=ferrocraft from file:.*boot-0.1.0.jar' "$work/game.log"
grep -q 'ferrocraft-engine: window' "$work/game.log"

# Official launcher order: libraries first, vanilla client jar last (portablemc above puts it first).
libs=$(find "$work/mc/libraries" -name '*.jar' ! -name 'boot-*' | tr '\n' ':')
out=$(FERROCRAFT_WINDOW_SECS=1 xvfb-run -a java -cp "$work/mc/libraries/net/ferrocraft/boot/0.1.0/boot-0.1.0.jar:${libs}$work/mc/versions/1.8.9/1.8.9.jar" \
  net.ferrocraft.boot.Main --gameDir "$work/profile")
echo "$out"
echo "$out" | grep -q 'ClientBrandRetriever=ferrocraft from file:.*boot-0.1.0.jar'
echo "$out" | grep -q 'ferrocraft-engine: window'
echo "SMOKE OK"
