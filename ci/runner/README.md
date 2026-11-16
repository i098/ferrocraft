# ferrocraft CI runner image

`Dockerfile` builds `ferrocraft-ci-runner:1` on top of the shared `crewship-ci-runner:latest`.
It adds what every `check` job installed at run time: Rust 1.100.0 (the pin in `rust-toolchain.toml`) and stable, both with rustfmt and clippy, and Temurin JDK 8 in the runner tool cache.

Build on the runner host, from this directory:

    docker build -t ferrocraft-ci-runner:1 .

Use it for the `ferrocraft` pool only. In `~/.config/runner-up/i098-ferrocraft.json`, set `"image": "ferrocraft-ci-runner:1"` on each place, then run `systemctl --user restart runner-up@i098-ferrocraft`. Keep a backup of the old file first.

Change the tag when the Rust pin in `rust-toolchain.toml` changes, then rebuild.
