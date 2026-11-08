#!/bin/sh
# Build the Fabric mod jar. Usage: FABRIC_LOADER_JAR=path/to/fabric-loader.jar build.sh OUT.jar   (JDK 8+; targets Java 8)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "$1")
: "${FABRIC_LOADER_JAR:?set FABRIC_LOADER_JAR to a fabric-loader jar (compile-time API only)}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
find "$here/src" -name '*.java' -exec javac -source 8 -target 8 -Xlint:-options -cp "$FABRIC_LOADER_JAR" -d "$tmp" {} +
cp "$here/resources/fabric.mod.json" "$tmp/"
mkdir -p "$(dirname "$out")"
jar cf "$out" -C "$tmp" .
