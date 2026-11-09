#!/bin/sh
# Static compatibility table for the Polyfrost 1.8.9 PvP mods. Usage: run.sh [CACHE_DIR]   (JDK 8+, curl, unzip)
# Downloads the public mod jars and the public MCP 1.8.9 SRG mapping into CACHE_DIR. Nothing is committed.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cache=${1:-${TMPDIR:-/tmp}/ferro-modhost-accept}
mkdir -p "$cache/mods" "$cache/classes"
[ -f "$cache/joined.srg" ] || {
  curl -fsSL -o "$cache/srg.zip" https://maven.minecraftforge.net/de/oceanlabs/mcp/mcp/1.8.9/mcp-1.8.9-srg.zip
  unzip -o -q "$cache/srg.zip" joined.srg -d "$cache"
}
jars=
while IFS='|' read -r role file url; do
  case $role in '#'*|'') continue;; esac
  [ -f "$cache/mods/$file" ] || curl -fsSL -o "$cache/mods/$file" "$url"
  jars="$jars $cache/mods/$file"
done < "$here/mods.txt"
javac -source 8 -target 8 -Xlint:-options -d "$cache/classes" "$here/Accept.java"
# shellcheck disable=SC2086
java -cp "$cache/classes" Accept "$cache/joined.srg" $jars
