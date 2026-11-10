#!/bin/sh
# Compiles the host classes into java/modhost/cls. FERRO_LIB is a folder with the Java 8 libraries
# (lw.jar = LaunchWrapper, asm-all, forge.jar, guava); FERRO_JAVA_HOME is a Java 8 JDK.
set -e
here=$(cd "$(dirname "$0")" && pwd)
J=${FERRO_JAVA_HOME:?set FERRO_JAVA_HOME to a Java 8 JDK}/bin
L=${FERRO_LIB:?set FERRO_LIB to the library folder}
rm -rf "$here/cls" && mkdir "$here/cls"
# Unsafe warnings come from R.java, which needs it to allocate stand-in objects without constructors.
"$J/javac" -nowarn -source 8 -target 8 -Xlint:-options \
    -cp "$L/lw.jar:$L/asm-all-5.0.3.jar:$L/forge.jar:$L/guava-17.0.jar" \
    -d "$here/cls" $(find "$here/src" -name '*.java')
