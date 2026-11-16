# ferrocraft CI runner image

`Dockerfile` builds `ferrocraft-ci-runner:1` on top of the shared `crewship-ci-runner:latest`.
The base image is a local image of this host with no registry digest.
It adds Rust 1.100.0 (the pin in `rust-toolchain.toml`) with rustfmt and clippy, and Temurin JDK 8 in the runner tool cache. Vulkan and ALSA packages come from the base image.

Build on the runner host, from this directory:

    docker build -t ferrocraft-ci-runner:1 .

Use it for the `ferrocraft` pool only. In `~/.config/runner-up/i098-ferrocraft.json`, set `"image": "ferrocraft-ci-runner:1"` on each place, then run `systemctl --user restart runner-up@i098-ferrocraft`. Keep a backup of the old file first.

Change the tag when the Rust pin in `rust-toolchain.toml` changes, then rebuild.

## Persistent cargo target dir

Each place keeps its cargo target folder on the host: `/sdc-disk/ci-target/<place id>`, mounted at `/home/runner/ci-target` with `CARGO_TARGET_DIR` set to it. A job reuses the build output of earlier jobs of the same place. No upload or download step runs. One folder for each place, so two jobs never write the same target dir. The shared sccache (host-wide, outside this change) still caches registry crates.

Mounts come from a docker shim, not from the runner config. `docker-shim` here is a wrapper for the ferrocraft service only. It adds the mount and the variable for places `i098-ferrocraft-N`, then runs the host-wide shim. Other pools do not change. The runner config `i098-ferrocraft.json` does not change (backup: `i098-ferrocraft.json.bak` in the task data folder).

Install on the runner host:

    mkdir -p ~/.local/share/runner-up/shim-ferrocraft
    cp docker-shim ~/.local/share/runner-up/shim-ferrocraft/docker
    # drop-in for runner-up@i098-ferrocraft: put that folder first in PATH
    #   ~/.config/systemd/user/runner-up@i098-ferrocraft.service.d/target-dir.conf
    systemctl --user daemon-reload && systemctl --user restart runner-up@i098-ferrocraft

Disk cap: below 20 GB. Before each job starts, the shim deletes the place folder when it holds more than `FERROCRAFT_PLACE_CAP_GB` (default 4). Four places hold at most 16 GB at job start, plus the growth of the running jobs. A deleted folder means one cold build, never a wrong result.

The job container runs as uid 1001 and the shim user is uid 1000, so the shim cannot delete files the container wrote. The shim user needs passwordless sudo (`sudo -n find`) for the delete.

Cargo decides freshness from file times, and a new checkout gives every file a new time. `ci/warm-target.sh start` fixes this: a file whose git blob equals the last fully built tree gets an old time, and a changed file gets the current time. `ci/warm-target.sh done` runs after `cargo test --no-run` succeeds and records the tree. A job that stops before then leaves no record, and the next job rebuilds everything.
