#!/usr/bin/env bash
# Live proof sheet: starts the local offline server and the engine with the 22 Forge twins of the OneClient mods under
# Xvfb, drives one step per mod (steps.txt) and writes one screenshot and one PASS or FAIL line per mod.
# Local server only. No live server, no Microsoft sign-in.
# Env (required): FERRO_JAVA_HOME (JDK 8), FERRO_LIB (Java 8 libraries, see java/modhost/build.sh), a mod jar folder FC_MODS
#   (default $TMPDIR/ferro-modhost-accept/mods, filled by java/modhost-accept/run.sh) and a built engine FC_ENGINE
#   (default $CARGO_TARGET_DIR or ./target, debug/ferrocraft-engine).
# Env (optional): FC_OUT (output folder, default $TMPDIR/ferrocraft-modproof), FC_DISPLAY (:131), FC_PORT (25591),
#   FC_BOOT_SECS (seconds to wait for the engine window, 90), FC_LOAD_SECS (seconds for the mods to load, 60), XDOTOOL (xdotool command), FC_DIFF_PX (pixels that
#   must change for a step to pass, 3000).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
MODS=${FC_MODS:-${TMPDIR:-/tmp}/ferro-modhost-accept/mods}
ENGINE=${FC_ENGINE:-${CARGO_TARGET_DIR:-$root/target}/debug/ferrocraft-engine}
OUT=${FC_OUT:-${TMPDIR:-/tmp}/ferrocraft-modproof}
DISP=${FC_DISPLAY:-:131}
PORT=${FC_PORT:-25591}
BOOT=${FC_BOOT_SECS:-90}
DIFF_PX=${FC_DIFF_PX:-3000}
XDO=${XDOTOOL:-xdotool}
for t in "$XDO" Xvfb ffmpeg python3 curl; do command -v "$t" >/dev/null || { echo "missing tool: $t" >&2; exit 2; }; done
[ -x "$ENGINE" ] || { echo "engine not built: $ENGINE" >&2; exit 2; }
ls "$MODS"/*.jar >/dev/null 2>&1 || { echo "no mod jars in $MODS (run java/modhost-accept/run.sh)" >&2; exit 2; }

rm -rf "$OUT" && mkdir -p "$OUT/run/mods" "$OUT/run/versions/1.8.9" "$OUT/srv" "$OUT/shots"
OUT=$(cd "$OUT" && pwd)
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

# Host classes, game folder, local server.
FERRO_JAVA_HOME=${FERRO_JAVA_HOME} FERRO_LIB=$L "$root/java/modhost/build.sh" >"$OUT/build.log" 2>&1
cp "$MODS"/*.jar "$OUT/run/mods/"
ln -sf "$L/minecraft-1.8.9.jar" "$OUT/run/versions/1.8.9/1.8.9.jar"
SHA1=b58b2ceb36e01bcd8dbf49c8fb66c55a9f0676cd
jar=${FC_SERVER_JAR:-$OUT/srv/server.jar}
[ -f "$jar" ] || curl -fsSL -o "$jar" "https://launcher.mojang.com/v1/objects/$SHA1/server.jar"
[ "$(sha1sum "$jar" | cut -d' ' -f1)" = "$SHA1" ] || { echo "server.jar sha1 mismatch" >&2; exit 2; }
( cd "$OUT/srv" && echo eula=true > eula.txt && cat > server.properties <<PROPS
online-mode=false
server-ip=127.0.0.1
server-port=$PORT
level-type=FLAT
generate-structures=false
spawn-monsters=false
spawn-animals=false
view-distance=4
max-players=4
use-native-transport=false
PROPS
  exec "$J/java" -Xmx512M -jar "$jar" nogui >"$OUT/server.log" 2>&1 ) &
pids+=($!)
Xvfb "$DISP" -screen 0 1280x720x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
pids+=($!)
export DISPLAY=$DISP
for _ in $(seq 60); do grep -q 'Done (' "$OUT/server.log" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$OUT/server.log" || { echo "local server did not start" >&2; exit 1; }

others=$(ls "$L"/*.jar | grep -v 'forge.jar\|minecraft-1' | tr '\n' ':')
export FERROCRAFT_MODHOST_JAVA=$J/java FERROCRAFT_MODHOST_GAMEDIR=$OUT/run
export FERROCRAFT_MODHOST_CP="$root/java/modhost/cls:$others$L/forge.jar:$L/minecraft-1.8.9.jar"
# The mods write their OneConfig folder into the working directory.
( cd "$OUT/run" && exec "$ENGINE" --username Proof --gameDir "$OUT/run" --assetsDir "$OUT/run/assets" --version 1.8.9 \
    --server "127.0.0.1:$PORT" >"$OUT/engine.log" 2>&1 ) &
pids+=($!)

win=
for _ in $(seq "$BOOT"); do
  win=$("$XDO" search --onlyvisible --name 'Minecraft' 2>/dev/null | head -1 || true)
  [ -n "$win" ] && break; sleep 1
done
[ -n "$win" ] || { echo "engine window did not open (see $OUT/engine.log)" >&2; exit 1; }
geo=$("$XDO" getwindowgeometry "$win" | awk '/Geometry/ {print $2}')
[ "$geo" = 800x600 ] || echo "warning: window is $geo, steps assume 800x600" >&2
# The mods load and the world shows: wait for the OneConfig welcome toast and the chunk load.
sleep "${FC_LOAD_SECS:-60}"
"$XDO" windowfocus "$win"; "$XDO" mousemove 400 300; sleep 1

shot() { ffmpeg -nostdin -loglevel error -y -f x11grab -video_size "$geo" -i "$DISP" -frames:v 1 "$1"; }
# Pixels in the OneConfig content area (x 160-780, y 110-530) that differ by more than 16 in some channel between two
# frames. The HUD text at the top left (coordinates, play time) changes on its own and stays outside.
diffpx() {
  python3 - "$1" "$2" <<'PY'
import subprocess, sys
W = 800
def raw(p):
    return subprocess.run(["ffmpeg", "-loglevel", "error", "-i", p, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                          check=True, stdout=subprocess.PIPE).stdout
a, b = raw(sys.argv[1]), raw(sys.argv[2])
n = 0
for y in range(110, 530):
    for x in range(160, 780):
        i = (y * W + x) * 3
        if max(abs(a[i] - b[i]), abs(a[i + 1] - b[i + 1]), abs(a[i + 2] - b[i + 2])) > 16:
            n += 1
print(n)
PY
}
click() { "$XDO" mousemove "$1" "$2"; sleep 0.2; "$XDO" mousedown 1; sleep 0.15; "$XDO" mouseup 1; }
tap() { "$XDO" keydown "$1"; sleep 0.2; "$XDO" keyup "$1"; }
close_gui() { tap Escape; sleep 1.5; }

shot "$OUT/shots/00-hud.png"
: > "$OUT/sheet.txt"
n=0; fails=0
while IFS='|' read -r id term note; do
  case $id in '#'*|'') continue;; esac
  [ -n "${FC_ONLY:-}" ] && case " $FC_ONLY " in *" $id "*) ;; *) continue;; esac
  n=$((n+1)); nn=$(printf %02d "$n")
  tap Shift_R; sleep 2
  grid=$OUT/shots/$nn-$id-grid.png; page=$OUT/shots/$nn-$id.png
  if [ -z "$term" ]; then
    shot "$page"; cp "$page" "$grid"
    d=$(diffpx "$OUT/shots/00-hud.png" "$page" 2>/dev/null || echo 0)
  else
    click 80 161; sleep 1.5   # the Mods grid: OneConfig reopens on the page that was open last
    shot "$OUT/shots/$nn-$id-nosearch.png"
    for attempt in 1 2; do
      click 672 80; sleep 1   # the field takes the focus about half a second after the click: earlier keys are lost
      "$XDO" key ctrl+a; "$XDO" type --delay 120 "$term"; sleep 1.5
      shot "$grid"
      # The field must take the text: a frame that did not change means the click missed the field.
      [ "$(diffpx "$OUT/shots/$nn-$id-nosearch.png" "$grid")" -ge 500 ] && break
      echo "note: ${id}: search field did not take the text, attempt $attempt again" >&2
      sleep 1
    done
    click 230 180; sleep 2
    shot "$page"
    d=$(diffpx "$grid" "$page")
  fi
  if [ "$d" -ge "$DIFF_PX" ]; then v=PASS; else v=FAIL; fails=$((fails+1)); fi
  printf '%s %-20s %s  changed_px=%s  %s\n' "$nn" "$id" "$v" "$d" "$note" | tee -a "$OUT/sheet.txt"
  close_gui
done < "$here/steps.txt"
echo "rows=$n fail=$fails sheet=$OUT/sheet.txt shots=$OUT/shots"
[ -n "${FC_ONLY:-}" ] || [ "$n" -eq 22 ]
