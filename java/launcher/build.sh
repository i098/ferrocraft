#!/bin/sh
# Build the boot jar. Usage: build.sh OUT.jar   (needs a JDK 8+; targets Java 8)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "$1")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
javac -source 8 -target 8 -Xlint:-options -d "$tmp" $(find "$here/src" -name '*.java')
mkdir -p "$(dirname "$out")"
jar cf "$out" -C "$tmp" .
