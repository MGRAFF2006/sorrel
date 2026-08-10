# sorrel-sdk-rust

> **Experimental alpha:** this crate is a thin convenience SDK over
> [`sorrel-core`](https://github.com/MGRAFF2006/sorrel-core). It is not
> Sorrel's stable or complete Rust embedding surface, and its API may change
> without compatibility guarantees before a stable release.

The current API re-exports a small set of core object and snapshot types and
provides a `Workspace` helper for initializing local object storage and
snapshotting a working tree. See [CHANGELOG.md](CHANGELOG.md) for the supported
alpha surface and known limitations.

## Checks

```sh
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```
