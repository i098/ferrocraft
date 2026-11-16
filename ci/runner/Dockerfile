# ferrocraft-only job image: the shared crewship-ci-runner plus the toolchain, Java 8 and the cargo components the repo's CI installs on every job.
FROM crewship-ci-runner:latest
USER root
RUN set -eu; \
    d=/opt/hostedtoolcache/Java_Temurin-Jdk_jdk/8.0.492-9.0/x64; mkdir -p $d; \
    curl -fsSL "https://api.adoptium.net/v3/binary/latest/8/ga/linux/x64/jdk/hotspot/normal/eclipse" | tar -xz --strip-components=1 -C $d; \
    v=$($d/bin/java -version 2>&1 | head -1); echo "$v"; \
    d2=/opt/hostedtoolcache/Java_Temurin-Jdk_jdk; ls $d2; \
    touch /opt/hostedtoolcache/Java_Temurin-Jdk_jdk/8.0.492-9.0/x64.complete; \
    chown -R runner:runner /opt/hostedtoolcache
USER runner
ENV CARGO_HOME=/home/runner/.cargo RUSTUP_HOME=/home/runner/.rustup PATH=/home/runner/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.100.0 -c rustfmt -c clippy \
 && rustup toolchain install stable --profile minimal -c rustfmt -c clippy \
 && rustup default 1.100.0 && rustup toolchain list
