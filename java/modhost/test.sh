#!/bin/sh
# Runs the host tests after build.sh: boots FML with the host's tweakers and a test class in place of the host's
# entry point. FERRO_JAVA_HOME and FERRO_LIB are as in build.sh; the folder also holds the Minecraft 1.8.9 jar and
# the other Java 8 libraries of the game.
set -e
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
rm -rf "$here/cls/test" && mkdir "$here/cls/test"
others=$(ls "$L"/*.jar | grep -v 'forge.jar\|minecraft-1' | tr '\n' ':')
cp="$here/cls/test:$here/cls:$others$L/forge.jar:$L/minecraft-1.8.9.jar"
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -cp "$cp" -d "$here/cls/test" $(find "$here/test" -name '*.java')
game=$(mktemp -d)
trap 'rm -rf "$game"' EXIT
cd "$game"
"$J/java" -cp "$cp" net.minecraft.launchwrapper.Launch \
    --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker \
    --tweakClass net.ferrocraft.tweak.HostTweaker \
    --gameDir "$game" --assetsDir "$L/minecraft-1.8.9.jar" --version 1.8.9 --accessToken x
