# Workspace component links

A Sorrel **Workspace** (`.sorrel/manifest.json`) describes a local repository.
Optional **`componentLinks`** declare how this workspace relates to other
components — the protocol counterpart to git submodules and Cargo dependencies.

## Roles

| `role` | Meaning | Tracking | Typical use |
| --- | --- | --- | --- |
| **`member`** | First-party part of the same product | **`branch`** (e.g. `main`) | Monorepo modules: `sorrel-core`, `sorrel-cli`, … |
| **`dependency`** | External or consumable component | **`revision`** or **`tag`** | Published engine pin, third-party lib |

This mirrors two git workflows:

- **Member** ↔ submodule with `branch = main` + `git submodule update --remote`
- **Dependency** ↔ submodule at a fixed commit, or Cargo `git` + `rev`

The root umbrella repo may still record commit SHAs when snapshotting a release;
members are *defined* to follow a branch. Dependencies are *defined* to follow a
revision.

## `ComponentLink` shape

```json
{
  "name": "sorrel-core",
  "role": "member",
  "location": {
    "type": "git",
    "url": "https://github.com/MGRAFF2006/sorrel-core.git",
    "path": "sorrel-core"
  },
  "tracking": { "mode": "branch", "branch": "main" }
}
```

```json
{
  "name": "sorrel-core",
  "role": "dependency",
  "location": {
    "type": "git",
    "url": "https://github.com/MGRAFF2006/sorrel-core.git"
  },
  "tracking": { "mode": "revision", "revision": "198fe2d31674400f3607b911aad5f7c5dcf9419c" }
}
```

`location.type`:

- **`git`** — remote repository (`url` required; optional checkout `path` in umbrella tree)
- **`path`** — sibling directory in a unified checkout (`url` is relative path, e.g. `../sorrel-core`)

## Workspace example

See [`examples/workspace.json`](../examples/workspace.json).

## Implementation status

| Consumer | Status |
| --- | --- |
| Protocol schema | `Workspace` + `ComponentLink` in `sorrel-object.schema.json` |
| `sorrel-cli` `manifest.json` | Writes core fields today; `componentLinks` optional, not yet emitted |
| Git umbrella (`sorrel/`) | Documents + `.gitmodules` `branch = main`; see root `docs/AGENT_WORKSPACE.md` |
| Hub / sync | Unchanged; sync transport moves objects, not component link metadata |

Tools **SHOULD** reject `member` links with `tracking.mode` other than `branch`, and
`dependency` links without `revision` or `tag`.

## Relation to sync transport

[`sync-transport.md`](sync-transport.md) moves content-addressed objects and refs
between workspaces. **Component links** describe how workspaces are composed in
source control — orthogonal to push/pull.
