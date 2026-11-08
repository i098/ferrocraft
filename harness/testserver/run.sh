#!/usr/bin/env bash
# Runs a vanilla 1.8.9 server in offline mode on 127.0.0.1:25565 (local tests only).
# Env: FC_JAVA (java binary, default `java`), FC_TESTSERVER_DIR (default: $TMPDIR/ferrocraft-testserver).
set -euo pipefail

URL="https://launcher.mojang.com/v1/objects/b58b2ceb36e01bcd8dbf49c8fb66c55a9f0676cd/server.jar"
SHA1="b58b2ceb36e01bcd8dbf49c8fb66c55a9f0676cd"
DIR="${FC_TESTSERVER_DIR:-${TMPDIR:-/tmp}/ferrocraft-testserver}"
JAVA="${FC_JAVA:-java}"

mkdir -p "$DIR"
cd "$DIR"

sha1_of() { if command -v sha1sum >/dev/null; then sha1sum "$1" | cut -d' ' -f1; else shasum -a 1 "$1" | cut -d' ' -f1; fi; }

if [ ! -f server.jar ] || [ "$(sha1_of server.jar)" != "$SHA1" ]; then
  curl -fsSL -o server.jar "$URL"
  [ "$(sha1_of server.jar)" = "$SHA1" ] || { echo "server.jar sha1 mismatch" >&2; rm -f server.jar; exit 1; }
fi

echo "eula=true" > eula.txt
cat > server.properties <<PROPS
online-mode=false
server-ip=127.0.0.1
server-port=25565
level-type=FLAT
generate-structures=false
spawn-monsters=false
spawn-animals=false
view-distance=4
max-players=4
use-native-transport=false
PROPS

exec "$JAVA" -Xmx512M -jar server.jar nogui
