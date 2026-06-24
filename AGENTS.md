# Version-Controle (Sorrel)

A Rust workspace for Sorrel, an agent-native version control system. The git
submodules listed in `.gitmodules` (`sorrel-web`, `sorrel-cli`, `sorrel-hub`,
etc.) are currently empty placeholders. The only buildable code lives in the
Cargo workspace member `crates/sorrel-core`, a content-addressed object-store
library (BLAKE3 object IDs, in-memory and filesystem stores).

## Cursor Cloud specific instructions

- This repo requires Rust **1.85+** (it uses `edition2024` / `rust-version = "1.85"`
  and a v4 `Cargo.lock`). The base image's default Rust may be older (1.83), which
  fails with `feature \`edition2024\` is required`. The update script installs and
  defaults to the `stable` toolchain; if you hit that error, run
  `rustup default stable`.
- `sorrel-core` is a **library only** (no binary/service/GUI). There is nothing to
  "serve" or run as a long-lived process. Validate it with the standard Cargo
  commands below.
- Standard commands (run from the repo root):
  - Build: `cargo build`
  - Test: `cargo test`
  - Lint: `cargo clippy --all-targets`
  - Format check: `cargo fmt --all -- --check`
- The `.gitmodules` submodules are unpopulated; do not expect them to build. Do not
  run `git submodule update --init` expecting working code unless the upstream
  submodule repos have been populated.
