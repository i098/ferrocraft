#!/bin/sh
# Builds the mod host's two jars from the player's own files. Usage, with a Java 8 JDK and the Forge universal jar:
#   FERRO_JAVA_HOME=<jdk8> FERRO_LIB=<folder with forge.jar> java/modhost-prepare/run.sh <client-1.8.9.jar> <out-dir>
# Writes <out-dir>/ferro-mc-1.8.9.jar and <out-dir>/ferro-forge.jar. Nothing is committed.
# "run.sh names <out-dir>" instead packs NameTable (the Fabric name table builder) with mapping-io into <out-dir>/ferro-names.jar.
# It needs curl, and any JDK 8 or newer in FERRO_JAVA_HOME; Config::provision_fabric finds the jar.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = names ]; then
    [ $# -eq 2 ] || { echo "usage: run.sh names <out-dir>" >&2; exit 2; }
    J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a JDK}/bin
    mkdir -p "$2"
    out=$(cd "$2" && pwd)
    mio=0.9.2
    sha=43d03303c07f35fffd938ad14f639ceb69893369
    curl -fsSL -o "$out/mapping-io.jar" "https://repo1.maven.org/maven2/net/fabricmc/mapping-io/$mio/mapping-io-$mio.jar"
    [ "$(sha1sum "$out/mapping-io.jar" | cut -d' ' -f1)" = "$sha" ] || { echo "mapping-io sha1 differs" >&2; exit 1; }
    rm -rf "$out/names-cls" && mkdir "$out/names-cls"
    ( cd "$out/names-cls" && "$J/jar" xf ../mapping-io.jar && rm -rf META-INF )
    "$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -cp "$out/mapping-io.jar" -d "$out/names-cls" "$here/src/net/ferrocraft/prepare/NameTable.java"
    "$J/jar" cf "$out/ferro-names.jar" -C "$out/names-cls" .
    rm -rf "$out/names-cls" "$out/mapping-io.jar"
    exit 0
fi
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
[ $# -eq 2 ] || { echo "usage: run.sh <client-1.8.9.jar> <out-dir>" >&2; exit 2; }
mkdir -p "$2"
# The classes go in the output folder, so the source tree stays clean.
out=$(cd "$2" && pwd)
rm -rf "$out/cls" && mkdir "$out/cls"
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -cp "$L/forge.jar:$L/lzma-0.0.1.jar:$L/lw.jar" \
    -d "$out/cls" $(find "$here/src" -name '*.java' ! -name NameTable.java)
# Forge's patch reader needs its own libraries (log4j, guava, asm, launchwrapper, lzma): the whole FERRO_LIB.
# It logs to ./logs, so run it inside the output folder.
cp="$out/cls:$(ls "$L"/*.jar | tr '\n' ':')"
( cd "$out" && "$J/java" -cp "$cp" net.ferrocraft.prepare.Prepare "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")" "$L/forge.jar" "$out" )
