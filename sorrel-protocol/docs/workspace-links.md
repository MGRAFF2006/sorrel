# Workspace component links

A Sorrel **Workspace** (`.sorrel/manifest.json`) describes a local repository.
Optional **`componentLinks`** declare how this workspace relates to other
components — the protocol counterpart to package path members and pinned
dependencies.

## Roles

| `role` | Meaning | Tracking | Typical use |
| --- | --- | --- | --- |
| **`member`** | First-party part of the same product | **`branch`** (e.g. `main`) | Monorepo packages: `sorrel-core`, `sorrel-cli`, … |
| **`dependency`** | External or consumable component | **`revision`** or **`tag`** | Published engine pin, third-party lib |

In the current monorepo, first-party packages live as path members under
`MGRAFF2006/sorrel`. `componentLinks` still describe composition for tools that
need an explicit graph; members track a branch conceptually, dependencies a
revision.

## `ComponentLink` shape

```json
{
  "name": "sorrel-core",
  "role": "member",
  "location": {
    "type": "path",
    "url": "sorrel-core"
  },
  "tracking": { "mode": "branch", "branch": "main" }
}
```

```json
{
  "name": "example-engine",
  "role": "dependency",
  "location": {
    "type": "git",
    "url": "https://github.com/example/engine.git"
  },
  "tracking": { "mode": "revision", "revision": "198fe2d31674400f3607b911aad5f7c5dcf9419c" }
}
```

`location.type`:

- **`path`** — in-tree package path in the monorepo (preferred for first-party members)
- **`git`** — remote repository (`url` required; optional checkout `path`)

## Workspace example

See [`examples/workspace.json`](../examples/workspace.json).

## Implementation status

| Consumer | Status |
| --- | --- |
| Protocol schema | `Workspace` + `ComponentLink` in `sorrel-object.schema.json` |
| `sorrel-cli` `manifest.json` | Writes core fields today; `componentLinks` optional, not yet emitted |
| Monorepo packages | Live in-tree; see root `docs/AGENT_WORKSPACE.md` |
| Hub / sync | Unchanged; sync transport moves objects, not component link metadata |

Tools **SHOULD** reject `member` links with `tracking.mode` other than `branch`, and
`dependency` links without `revision` or `tag`.

## Relation to sync transport

[`sync-transport.md`](sync-transport.md) moves content-addressed objects and refs
between workspaces. **Component links** describe how workspaces are composed in
source control — orthogonal to push/pull.
