#!/usr/bin/env bash
# Hot reload proof, see README.md. Right Shift opens OneConfig only when its jar is loaded. Exit 0 when the second host came up in the same engine process.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
L=${FERRO_LIB:?}; J=${FERRO_JRE:?}; jar=${FC_SERVER_JAR:?}
ENGINE=${FC_ENGINE:-${CARGO_TARGET_DIR:-$root/target}/debug/ferrocraft-engine}
BEFORE=${FC_BEFORE_MODS:?}; ADD=${FC_ADD_MODS:?}
OUT=${FC_OUT:-${TMPDIR:-/tmp}/ferrocraft-modreload}; DISP=${FC_DISPLAY:-:137}; PORT=${FC_PORT:-25597}
rm -rf "$OUT"; mkdir -p "$OUT/run/mods" "$OUT/run/versions/1.8.9" "$OUT/srv"; OUT=$(cd "$OUT" && pwd)
pids=(); trap 'for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done' EXIT
cp "$BEFORE"/*.jar "$OUT/run/mods/" 2>/dev/null || true
# Xvfb has no focus: without this the engine opens the pause menu.
printf "pauseOnLostFocus:false\n" > "$OUT/run/options.txt"
ln -sf "$L/minecraft-1.8.9.jar" "$OUT/run/versions/1.8.9/1.8.9.jar"
( cd "$OUT/srv" && echo eula=true > eula.txt && printf 'online-mode=false\nserver-ip=127.0.0.1\nserver-port=%s\nlevel-type=FLAT\ngenerate-structures=false\nspawn-monsters=false\nspawn-animals=false\nview-distance=4\nuse-native-transport=false\n' "$PORT" > server.properties
  exec "$J" -Xmx512M -jar "$jar" nogui >"$OUT/server.log" 2>&1 ) & pids+=($!)
Xvfb "$DISP" -screen 0 1280x720x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 & pids+=($!)
export DISPLAY=$DISP
for _ in $(seq 120); do grep -q 'Done (' "$OUT/server.log" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$OUT/server.log" || { echo "server did not start" >&2; exit 1; }
others=; for j in "$L"/*.jar; do case $j in *forge.jar*|*minecraft-1*) ;; *) others+=$j:;; esac; done
export FERROCRAFT_MODHOST_JAVA=$J FERROCRAFT_MODHOST_GAMEDIR=$OUT/run
export FERROCRAFT_MODHOST_CP="$root/java/modhost/cls:$others$L/forge.jar:$L/minecraft-1.8.9.jar"
( cd "$OUT/run" && exec "$ENGINE" --username Proof --gameDir "$OUT/run" --assetsDir "$L/../assets" --version 1.8.9 \
    --server "127.0.0.1:$PORT" >"$OUT/engine.log" 2>&1 ) & eng=$!; pids+=($eng)
loaded() { grep -c 'ferro-host\] mods loaded' "$OUT/engine.log" || true; }
shot() { ffmpeg -nostdin -loglevel error -y -f x11grab -video_size 1280x720 -i "$DISP" -frames:v 1 "$1"; }
for _ in $(seq 240); do [ "$(loaded)" -ge 1 ] && break; sleep 1; done
[ "$(loaded)" -ge 1 ] || { echo "first host did not load" >&2; exit 1; }
sleep "${FC_SETTLE:-20}"; python3 "$root/harness/modreload/key.py" Shift_R; sleep 3; shot "$OUT/before.png"
jvm1=$(pgrep -f HostTweaker | head -1 || true); [ -n "$jvm1" ] || { echo "no first host JVM" >&2; exit 1; }
cp "$ADD"/*.jar "$OUT/run/mods/"
for _ in $(seq 240); do [ "$(loaded)" -ge 2 ] && break; sleep 1; done
[ "$(loaded)" -ge 2 ] || { echo "no second host (see $OUT/engine.log)" >&2; exit 1; }
sleep "${FC_SETTLE:-20}"; python3 "$root/harness/modreload/key.py" Shift_R; sleep 3; shot "$OUT/after.png"
kill -0 "$eng" || { echo "engine died" >&2; exit 1; }
n=$(pgrep -fc 'HostTweaker' || true)
echo "engine pid $eng alive; host JVMs: $n; first JVM $jvm1 alive: $(kill -0 "$jvm1" 2>/dev/null && echo yes || echo no)"
[ "$n" = 1 ] || { echo "expected one host JVM" >&2; exit 1; }
grep -E 'reloading|mods loaded' "$OUT/engine.log"
