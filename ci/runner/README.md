# ferrocraft CI runner image

`Dockerfile` builds `ferrocraft-ci-runner:1` on top of the shared `crewship-ci-runner:latest`.
The base image is a local image of this host with no registry digest.
It adds Rust 1.100.0 (the pin in `rust-toolchain.toml`) with rustfmt and clippy, and Temurin JDK 8 in the runner tool cache. Vulkan and ALSA packages come from the base image.

Build on the runner host, from this directory:

    docker build -t ferrocraft-ci-runner:1 .

Use it for the `ferrocraft` pool only. In `~/.config/runner-up/i098-ferrocraft.json`, set `"image": "ferrocraft-ci-runner:1"` on each place, then run `systemctl --user restart runner-up@i098-ferrocraft`. Keep a backup of the old file first.

Change the tag when the Rust pin in `rust-toolchain.toml` changes, then rebuild.
