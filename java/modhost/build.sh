#!/bin/sh
# Builds ferro-host.jar. See java/modhost/README.md for build requirements.
set -e
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
sep=:
if [ "${RUNNER_OS:-}" = Windows ]; then
    L=$(cygpath -m "$L")
    sep=';'
fi
rm -rf "$here/cls" && mkdir "$here/cls"
asm=$(ls "$L"/asm-all-*.jar)
# Unsafe warnings come from R.java, which needs it to allocate stand-in objects without constructors.
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options \
    -cp "$L/lw.jar$sep$asm$sep$L/forge.jar$sep$L/guava-17.0.jar" \
    -d "$here/cls" $(find "$here/src" -name '*.java')
"$J/jar" cf "$here/ferro-host.jar" -C "$here/cls" .
