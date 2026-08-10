# Sorrel Agent Task Pack

Last updated: 2026-08-10 UTC

Small, self-contained work orders derived from [`ROADMAP.md`](ROADMAP.md).

> **Status 2026-08-10:** Waves 1–2, the Git mirror, and **monorepo absorption
> (PR #49)** are done. Prefer one PR against root `main`.

## Completed (high signal)

- Merge/conflict model, `merge --continue`, protocol-aligned Conflict/MergeResult
- Hub FS persistence, collaboration write path, bootstrap grants
- Git import / export / colocated `git sync`
- Monorepo absorption (in-tree packages, Cargo workspace, path deps; green CI)

## How to use this pack

- Work in the root checkout; commit once in this repository.
- Run `npm run validate:release`, `npm run test:modules`, and `npm test` for
  cross-package changes.

## Open

### SECRETS-1 — SecretSpec under Sorrel policy

```text
Repository: https://github.com/MGRAFF2006/sorrel (this monorepo)

Context: SecretRef + Core grants stay source of truth. SecretSpec (Apache-2.0,
upstream; do not fork) is the provider/resolver (keyring, dotenv, env first).

Task:
1. Bridge sorrel.secrets.yml / SecretRef names ↔ secretspec.toml (or untyped API).
2. Extend provider enum beyond sorrel-vault to SecretSpec provider URIs.
3. In sorrel-cli: after secret.read / secret.inject allow, ship
   `sorrel secret check|get|set|list` and `sorrel secret run -- <cmd>`.
4. Inject authorized secretRefs into workflow run env; keep log redaction.
5. Deprecate Node vault-cli as primary UX (keep package for schema/tests).

Validate:
  cargo test -p sorrel-cli
  cargo clippy -p sorrel-cli --all-targets -- -D warnings
  npm test (root E2E paths that touch workflow/secrets)
```

### ENV-1 — devenv-first execution + local fallback

`sorrel env init|ensure|shell|info`; prefer `devenv tasks` for workflow/task
run when present; LocalProcessRunner fallback with `--json` `backend:
local-fallback`. Design `RunnerBackend` trait now; remote later.

### LOGS-1 — Structured execution logs

Versioned JSONL under `.sorrel/runs/<id>/`; `sorrel run show|logs`; redaction
markers; backend + principal metadata. Hub streaming later.

### NEXT-1 — Production Hub auth (after alpha)

Hub still trusts acting-principal headers + bootstrap grants. Design and ship a
minimal signed identity path before exposing Hub beyond localhost.

### NEXT-2 — Embedding surface (after alpha)

Stable library boundary for SDKs/apps (C ABI / N-API / WASM / daemon). Do not
start desktop/mobile apps before this.

### DEBT-1 — Collapse CLI forks (after secrets/injection)

`sorrel-cli` still carries in-tree `cli_policy` and `cli_runner` duplicates of
engine/runners packages. **Do not unify yet** — schedule after SecretSpec
injection ships so `--json` shapes and secret env wiring settle first.
