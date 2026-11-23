#!/bin/sh
# Boots the Fabric host with the mods of the list that FERRO_MODS_LIST names (default java/modhost-accept/mods-fabric.txt) and prints PASS or FAIL for each
# entrypoint, for the hello and for Mixin errors (FabricBootTest). Exit 1 on any FAIL; a JVM that stops before the hello is a FAIL.
# FERRO_JAVA_HOME and FERRO_LIB are as in build.sh (any JDK 8 or newer; it builds the host jar and the test). FABRIC_LOADER_JAR is as
# in build.sh. FERRO_FABRIC_CONFIG lists the Java runtime, game directory, class path, and runtime directory.
# (see README: the ignored Rust test writes it when FERRO_FABRIC_CONFIG_OUT is set). Needs the network for the mod jars.
# FERRO_MODS_LIST is the mod list: default mods-fabric.txt; java/modhost-accept/mods-ornithe.txt for the Ornithe profile (config from Config::provision_ornithe).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a JDK}/bin
cfg=${FERRO_FABRIC_CONFIG:?set FERRO_FABRIC_CONFIG to the file the provision test writes}
"$here/build.sh"
out=$here/../../target/modhost-fabric
java=$(sed -n 1p "$cfg")
main=$(sed -n 2p "$cfg")
classpath=$(sed -n 3p "$cfg")
runtime=$(sed -n 4p "$cfg")
[ -n "$runtime" ] || { echo "No runtime directory: regenerate FERRO_FABRIC_CONFIG" >&2; exit 2; }
cp "$out/ferro-host-fabric.jar" "$runtime/ferro-host-fabric.jar"
tiny=$runtime/names.tiny
[ -f "$tiny" ] || { echo "No name table: $tiny" >&2; exit 2; }
mkdir -p "$main/mods"
ids=
while IFS='|' read -r want file url; do
    case $want in '#'*) continue;; esac
    [ -n "$file" ] || continue
    [ -f "$main/mods/$file" ] || { curl -fsSL -o "$main/mods/$file.part" "$url" && mv "$main/mods/$file.part" "$main/mods/$file"; }
    ids="$ids $(echo "$want" | tr ',' ' ')"
done < "${FERRO_MODS_LIST:-$here/../modhost-accept/mods-fabric.txt}"
mkdir -p "$out/test"
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -d "$out/test" "$here/test/net/ferrocraft/fabric/FabricBootTest.java"
# shellcheck disable=SC2086
"$J/java" -cp "$out/test" net.ferrocraft.fabric.FabricBootTest "$java" "$classpath" "$runtime/ferro-host-fabric.jar" "$tiny" "$main" "$runtime/assets" $ids
