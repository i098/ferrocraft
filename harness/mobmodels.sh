#!/usr/bin/env bash
# Prints the slime, magma cube, blaze, ghast and bat model trees of the real 1.8.9 client.
# Needs target/oracle/mapped.jar (built by `cargo test -p oracle`) and a JDK 8 or newer.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${FERROCRAFT_ORACLE_DIR:-$root/target/oracle}"
jar="$dir/mapped.jar"
libs="$(printf %s: "$dir"/libs/*.jar)"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
javac -nowarn -source 8 -target 8 -cp "$jar" -d "$out" "$root/java/oracle/MobModels.java" 2>/dev/null || \
  javac --release 8 -cp "$jar" -d "$out" "$root/java/oracle/MobModels.java"
java -cp "$out:$jar:$libs" MobModels
