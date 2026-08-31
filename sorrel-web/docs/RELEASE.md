<!-- Generated from docs/RELEASE.md by npm run sync:docs. Do not edit. -->

# Release process

Sorrel releases are tagged from this monorepo. `release/manifest.json` lists the
in-tree packages that belong to the release and records the coordinated version
and release date.

## Sources of truth

- GitHub Releases are the immutable public progress record.
- Root `CHANGELOG.md` is the coordinated release summary and source for release
  notes.
- Each `sorrel-*/CHANGELOG.md` records package-specific behavior and limits.
- `docs/STATUS.md` is a current snapshot; `ROADMAP.md` contains future work.

Do not create separate progress dashboards, agent task packs, or feature-audit
ledgers. They duplicate the changelog and become stale.

## Alpha scope

`v0.1.0-alpha.1` is a local-first developer preview. Core, CLI, protocol, and
the Git bridge are the primary supported surface. Hub is localhost/dev-only;
vault, runners, slices, agents, and SDKs are experimental.

## Release candidate checks

From the repo root:

```sh
npm run setup
npm run check
```

CI runs the same consistency, module, lint/format, and E2E components in
separate steps for clearer failure reporting.

Run `cargo bench -p sorrel-core --bench engine` and compare it with
`benchmarks/BASELINE.json` when the release notes claim performance numbers.

## Tagging

After checks pass on `main`:

1. Move every shipped entry from `Unreleased` into a dated version section in
   the root and affected package changelogs.
2. Update `release/manifest.json` and all package versions together.
3. Confirm the release note extraction:

   ```sh
   npm run release:notes -- v0.1.0-alpha.1
   ```

4. Commit and merge the release candidate to `main`.
5. Create an annotated tag and push it:

   ```sh
   git tag -a v0.1.0-alpha.1 -m "Sorrel v0.1.0-alpha.1"
   git push origin v0.1.0-alpha.1
   ```

6. Publish the exact changelog section as a prerelease:

   ```sh
   npm run release:notes -- v0.1.0-alpha.1 > .dev/release-notes.md
   gh release create v0.1.0-alpha.1 \
     --repo MGRAFF2006/sorrel \
     --title "Sorrel v0.1.0-alpha.1" \
     --notes-file .dev/release-notes.md \
     --prerelease --verify-tag
   ```

7. Verify the published tag, release URL, and changelog links.

Tags are release anchors; do not move or recreate them. A correction gets a new
prerelease version.

## After publication

Keep `Unreleased` at the top of each changelog. New user-visible work enters
there as it lands. The next release moves those entries into a new dated
section; do not edit prior release entries except to correct broken links or
factually dangerous errors.

## Rollback

Check out the prior root tag. Back up `.sorrel/` and Hub data directories before
moving between alpha versions; no general storage migration framework exists yet.

## CI

Root Actions checks out this repository directly. No `SUBMODULES_TOKEN` is
required.
