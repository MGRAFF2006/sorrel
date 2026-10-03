# sorrel-sdk-rust

> **Experimental alpha:** this crate is a thin convenience SDK over
> [`sorrel-core`](../sorrel-core). It is not
> Sorrel's stable or complete Rust embedding surface, and its API may change
> without compatibility guarantees before a stable release.

The current API re-exports a small set of core object and snapshot types and
provides a `Workspace` helper using the CLI's `.sorrel` layout. `Workspace::init`
creates a persistent manifest, active HEAD, and main lane; calling it again with
the same repository id returns the existing HEAD. `Workspace::open` validates
an existing workspace, and `head_snapshot` reads whichever lane is active.
Initialization and reads share the CLI's repository lock.

`snapshot_working_tree` creates a detached immutable snapshot and excludes root
`.sorrel` and `.git` metadata. It does not move HEAD or record a CLI change.
`snapshot_working_tree_filtered` additionally accepts a fallible path filter;
embedding applications should use it to enforce their ignore rules, including
secret and build-output exclusions. The SDK does not automatically discover
`.gitignore` or `.sorrelignore` files.

Old SDK-only stores at `.sorrel/objects/objects` require an explicit migration;
the SDK refuses to reset or silently reinterpret that existing storage. Run the
CLI to recover an interrupted checkout/ref transaction before reopening it
through the SDK. See [CHANGELOG.md](CHANGELOG.md) for the supported
alpha surface and known limitations.

## Checks

```sh
cargo test -p sorrel-sdk
cargo clippy -p sorrel-sdk --all-targets -- -D warnings
cargo fmt --all -- --check
```
