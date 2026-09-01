## Why

<!-- What problem does this solve, and why is this change needed now? -->

## What changed

<!-- Summarize the implementation. Keep the PR focused on one concern. -->

## User-visible and compatibility effects

<!-- Describe CLI/API/UI/config/storage/security effects. Write "None" when applicable. -->

## Validation

<!-- List the exact checks you ran and their outcomes. Do not claim checks you did not run. -->

- [ ] `npm run check:quick`
- [ ] Focused package checks:
- [ ] `npm test` for cross-package behavior, if applicable
- [ ] `npm run check` before requesting final review

## Impact checklist

- [ ] Tests cover changed behavior, regressions, and relevant failure paths.
- [ ] The PR title clearly describes the user or operator impact so release automation can generate changelogs.
- [ ] User-facing guides, README content, CLI help, examples, and API/SDK docs are updated where behavior changed.
- [ ] Canonical public Markdown and root changelog changes were mirrored with `npm run sync:docs`.
- [ ] `docs/ARCHITECTURE.md`, `docs/STATUS.md`, and `ROADMAP.md` reflect any boundary, shipped-capability, or future-plan changes.
- [ ] Protocol conformance sources and generated consumer fixtures were synchronized where contracts changed.
- [ ] Lockfiles are updated where dependencies changed.
- [ ] No secrets, local state, build output, logs, or `.env*` files are included.

Omitted checks, `skip-changelog` rationale, screenshots, or reviewer notes:

<!-- Explain unchecked items that would normally apply. Add UI screenshots when useful. -->
