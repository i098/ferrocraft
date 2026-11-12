#!/bin/sh
# Builds the mod host's two jars from the player's own files. Usage, with a Java 8 JDK and the Forge universal jar:
#   FERRO_JAVA_HOME=<jdk8> FERRO_LIB=<folder with forge.jar> java/modhost-prepare/run.sh <client-1.8.9.jar> <out-dir>
# Writes <out-dir>/ferro-mc-1.8.9.jar and <out-dir>/ferro-forge.jar. Nothing is committed.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
[ $# -eq 2 ] || { echo "usage: run.sh <client-1.8.9.jar> <out-dir>" >&2; exit 2; }
mkdir -p "$2"
# The classes go in the output folder, so the source tree stays clean.
out=$(cd "$2" && pwd)
rm -rf "$out/cls" && mkdir "$out/cls"
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -cp "$L/forge.jar:$L/lzma-0.0.1.jar:$L/lw.jar" \
    -d "$out/cls" $(find "$here/src" -name '*.java')
# Forge's patch reader needs its own libraries (log4j, guava, asm, launchwrapper, lzma): the whole FERRO_LIB.
# It logs to ./logs, so run it inside the output folder.
cp="$out/cls:$(ls "$L"/*.jar | tr '\n' ':')"
( cd "$out" && "$J/java" -cp "$cp" net.ferrocraft.prepare.Prepare "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")" "$L/forge.jar" "$out" )
