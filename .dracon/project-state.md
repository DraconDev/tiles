# Project State

## Current Focus
Adds a `get_run_command` helper that resolves how to execute a file based on shebang, Rust Cargo, or file extension.

## Completed
- [x] Implemented `get_run_command` returning `(PathBuf, program, Vec<String>)` for executable scripts, Rust binaries, and known interpreters.
- [x] Added shebang detection on Unix for executable files.
- [x] Added support for running Rust projects via `cargo run` when a `Cargo.toml` is found.
- [x] Integrated extension‑based interpreter mapping for common script types.
