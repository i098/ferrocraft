#!/bin/sh
# Build the boot jar. Usage: build.sh OUT.jar   (needs a JDK 8+; targets Java 8)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
case $1 in /*) out=$1;; *) out=$PWD/$1;; esac
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
find "$here/src" -name '*.java' -exec javac -source 8 -target 8 -Xlint:-options -d "$tmp" {} +
mkdir -p "$(dirname "$out")"
jar cf "$out" -C "$tmp" .
