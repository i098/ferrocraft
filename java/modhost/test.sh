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
# FERRO_PREPARE_OUT, from java/modhost-prepare/run.sh, boots on the generated jars instead of the player's two jars.
fj=$L/forge.jar
mcj=$L/minecraft-1.8.9.jar
if [ -n "${FERRO_PREPARE_OUT:-}" ]; then
    fj=$FERRO_PREPARE_OUT/ferro-forge.jar
    mcj=$FERRO_PREPARE_OUT/ferro-mc-1.8.9.jar
fi
cp="$here/cls/test:$here/cls:$others$fj:$mcj"
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options -cp "$cp" -d "$here/cls/test" $(find "$here/test" -name '*.java')
game=$(mktemp -d)
trap 'rm -rf "$game"' EXIT
cd "$game"
# The 22 Forge twins of the OneClient mods (mod ids, not file names), plus Keystrokes. oneconfig is the loader the
# bootstrap downloads at first boot.
twins=oneconfig,evergreenhud,polysprint,vanillahud,polycrosshair,redaction,chatting,chatblock,polytime,polyweather,freelook,behindyouv3,betternightvision,damagetint,polyhitbox,polynametag,countdown,glintcolorizer,overflowanimations,overflowparticles,polyblur,colorsaturation,polyfrostkeystrokes
boot() {
    # FML refuses a Minecraft jar whose classes are not signed, and the generated jar is a patched copy: ignore that.
    # The mod boot needs network on a cold cache: OneConfig's bootstrap downloads its loader, and Cloudflare drops
    # this JDK's TLS 1.3 offer, so TLS 1.2 is pinned.
    "$J/java" -Dferro.test="$1" -Dferro.mods=$twins -Dfml.ignoreInvalidMinecraftCertificates=true -Djdk.tls.client.protocols=TLSv1.2 -cp "$cp" net.minecraft.launchwrapper.Launch \
        --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker \
        --tweakClass net.ferrocraft.tweak.HostTweaker \
        --gameDir "$game" --assetsDir "$L/minecraft-1.8.9.jar" --version 1.8.9 --accessToken x
}
boot PlayerTest
# Optional: FERRO_MODS is a folder with the public mod jars listed in java/modhost-accept/mods.txt.
if [ -n "${FERRO_MODS:-}" ]; then
    mkdir -p "$game/mods" && cp "$FERRO_MODS"/*.jar "$game/mods/"
    boot ModBootTest
fi
