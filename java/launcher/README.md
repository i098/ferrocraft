# java/launcher

Owner: fc-dropin (crate `crates/launcher`).

Java side of the drop-in install.

- `net.ferrocraft.boot.Main` is the `mainClass` of `versions/ferrocraft-1.8.9`. It builds a class loader with our jar first (launchers order the classpath differently) and runs `Engine` in it.
- `net.ferrocraft.boot.Engine` starts the Rust engine binary and waits for it.
- `net.minecraft.client.ClientBrandRetriever` is a stand-in for the vanilla class. It proves our jar comes before the vanilla jar on the classpath.
- `build.sh OUT.jar` builds the boot jar (Java 8 target).
