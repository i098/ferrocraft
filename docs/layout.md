# Workspace layout

Slices never share files; every directory has a README with an Owner line.

![Workspace layout](layout.svg)

## Ownership contract

- The root `Cargo.toml` is a workspace with `members = ["crates/*", "loaders/*", "adapters/*"]`, owned by the scaffold slice only.
- Each slice owns its directories and must not edit root files owned by another slice.
- `java/` and `crates/oracle` are layout nodes in `layout.mmd`; each is owned by one slice, like every other directory.
- Loaders and adapters connect to the engine only through `crates/modhost`, so a universal-client adapter later adds a folder without changing the core.

## Project rules

- Rust 2021, rustfmt and clippy clean, `cargo test` passes.
- Every PR starts as an issue it closes (`Closes #N`).
- PR file cap is 10. Merge commits are used.
- One line in the PR body states the chosen check plan.
