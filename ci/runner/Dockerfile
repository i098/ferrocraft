# ferrocraft-only job image: the shared crewship-ci-runner plus the toolchain, Java 8 and the cargo components the repo's CI installs on every job.
FROM crewship-ci-runner:latest
USER root
RUN set -eu; \
    d=/opt/hostedtoolcache/Java_Temurin-Jdk_jdk/8.0.492-9.0/x64; mkdir -p $d; \
    curl -fsSL "https://api.adoptium.net/v3/binary/version/jdk8u492-b09/linux/x64/jdk/hotspot/normal/eclipse" | tar -xz --strip-components=1 -C $d; \
    touch /opt/hostedtoolcache/Java_Temurin-Jdk_jdk/8.0.492-9.0/x64.complete; \
    chown -R runner:runner /opt/hostedtoolcache
USER runner
ENV CARGO_HOME=/home/runner/.cargo RUSTUP_HOME=/home/runner/.rustup PATH=/home/runner/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.100.0 -c rustfmt -c clippy \
 && rustup default 1.100.0 && rustup toolchain list
