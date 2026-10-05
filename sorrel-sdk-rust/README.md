# sorrel-sdk-rust

> **Experimental alpha:** this crate is a thin convenience SDK over
> [`sorrel-core`](../sorrel-core). It is not
> Sorrel's stable or complete Rust embedding surface, and its API may change
> without compatibility guarantees before a stable release.

The current API re-exports a small set of core object and snapshot types and
provides a `Workspace` helper for initializing local object storage and
snapshotting a working tree. See [CHANGELOG.md](CHANGELOG.md) for the supported
alpha surface and known limitations.

`Workspace::init` stores objects under `.sorrel/objects`. Working-tree snapshots
use Core's shared CLI/SDK selection: nested `.gitignore`/`.sorrelignore`, ordinary
tracked-file preservation, and protected dotenv files. The parent snapshot is
the tracked baseline. `.env.example` follows ordinary ignore rules. See the
[workspace selection contract](../docs/ARCHITECTURE.md#change-lane-and-merge-flow).
Previously initialized SDK stores under `.sorrel/objects/objects` are not migrated
automatically; keep them backed up when changing to the corrected layout.

## Checks

```sh
cargo test -p sorrel-sdk
cargo clippy -p sorrel-sdk --all-targets -- -D warnings
cargo fmt --all -- --check
```
