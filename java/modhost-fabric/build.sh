#!/bin/sh
# Builds target/modhost-fabric/ferro-host-fabric.jar (the target folder is not tracked): the host classes of java/modhost (without the Forge shadow of
# net.minecraft.client.main.Main, which Knot would read as a second Main) and the agent and the Fabric glue of this folder.
# The jar is the first class path entry and the -javaagent of the Fabric host. FERRO_JAVA_HOME may be any JDK 8 or newer: the
# classes are built for Java 8. FERRO_LIB is the library folder of java/modhost/build.sh; FABRIC_LOADER_JAR is a fabric-loader jar.
set -e
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
F=${FABRIC_LOADER_JAR:?set FABRIC_LOADER_JAR to a fabric-loader jar}
out=$here/../../target/modhost-fabric
rm -rf "$out" && mkdir -p "$out/cls"
asm=$(ls "$L"/asm-all-*.jar)
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options \
    -cp "$L/lw.jar:$asm:$L/forge.jar:$L/guava-17.0.jar:$F" \
    -d "$out/cls" $(find "$here/src" "$here/../modhost/src" -name '*.java' ! -path '*/net/minecraft/*')
printf 'Premain-Class: net.ferrocraft.fabric.Agent\n' > "$out/manifest.mf"
"$J/jar" cfm "$out/ferro-host-fabric.jar" "$out/manifest.mf" -C "$out/cls" net
