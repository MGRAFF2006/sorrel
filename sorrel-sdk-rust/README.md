# sorrel-sdk-rust

> **Experimental alpha:** this crate is a thin convenience SDK over
> [`sorrel-core`](../sorrel-core). It is not
> Sorrel's stable or complete Rust embedding surface, and its API may change
> without compatibility guarantees before a stable release.

The current API re-exports a small set of core object and snapshot types and
provides a `Workspace` helper for initializing local object storage and
snapshotting a working tree. See [CHANGELOG.md](CHANGELOG.md) for the supported
alpha surface and known limitations.

`Workspace::init` uses the shared `.sorrel/objects` object-store layout. Older SDK
versions wrote to `.sorrel/objects/objects`; those files remain untouched and are
not automatically migrated. To read an older store directly, pass
`.sorrel/objects` to `FileObjectStore::new`.

## Checks

```sh
cargo test -p sorrel-sdk
cargo clippy -p sorrel-sdk --all-targets -- -D warnings
cargo fmt --all -- --check
```
