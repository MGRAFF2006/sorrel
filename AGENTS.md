# Sorrel

Sorrel is an agent-native version-control system split across submodules.

The root repository coordinates architecture and submodule pointers. Most implementation work lives in submodules:

- sorrel-protocol: protocol schemas and examples
- sorrel-core: Rust core object store and future VCS primitives
- sorrel-cli: Rust CLI skeleton
- sorrel-vault: secrets/environment specs and local dev backend
- other sorrel-* repos: future modules

## Rust toolchain

This repo and Rust submodules require Rust 1.85+ because they use edition2024 / modern Cargo metadata. If the base image has an older Rust version, run:

rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
cargo fetch

## Common checks

From Rust repos/workspaces:

cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check

## Submodules

Some submodules may be private. If `git submodule update --init --recursive` fails with "Repository not found", ensure the agent has access to the private Sorrel repos.
