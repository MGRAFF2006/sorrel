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

`v0.1.0-alpha.2` is an installable developer preview. Core, CLI, protocol, and
the Git bridge are the primary supported surface. Hub server images are
hostable, but the service remains development-only without a production
AuthAdapter and network controls; vault, runners, slices, agents, and SDKs are
experimental.

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

Validate the binary distribution plan with the `dist` version pinned in
`dist-workspace.toml`:

```sh
dist plan
```

The dist plan must include CLI archives for Linux x64/ARM64, macOS
Intel/Apple Silicon, and Windows x64/ARM64, plus their SHA-256 files and the
shell and PowerShell installers.

The release workflow also builds Sorrel Hub desktop installers on native
runners for the same six OS/architecture pairs. The Windows ARM64 job runs on
`windows-11-arm`; a release is not published if any required desktop
bundle job fails. `dist-workspace.toml` registers the reusable
`.github/workflows/build-desktop.yml` job, so `dist generate` preserves the
dependency automatically; review both generated and reusable workflows when
the release matrix changes.

Build and exercise the exact server containers that the server release workflow
will publish:

```sh
docker build --tag sorrel-hub:smoke --file sorrel-hub/Dockerfile .
docker build --tag sorrel-hub-web:smoke --file sorrel-hub-web/Dockerfile .
./scripts/smoke-server-images.sh
SORREL_VERSION=0.1.0-alpha.2 \
  docker compose --file deploy/sorrel-server.compose.yml config --quiet
```

## Tagging

### Hosted pilot launch gate

Before external invitations, test the current source-built stack with real
WorkOS application configuration and the intended HTTPS hostname:

- Login/callback uses the exact registered URI. Expiry refreshes a session;
  logout clears local credentials and completes identity-provider sign-out;
  expired or revoked credentials fail closed. Test an IdP outage and retry.
- Two invited teams can see only their exact projects, repository bytes, refs,
  counters, proposals and comments. A forged acting-principal header cannot
  substitute for a hosted identity. Test scoped CLI upload through `/api`.
- A new invitee finds the provisioned project without policy-file editing.
  Verify the empty-access state and clearly document current manual CLI token
  provisioning; automatic CLI login is still an onboarding gap.
- Restart API/web and confirm metadata, objects, refs and valid cookie sessions
  persist. Preserve the cookie password in the operator's secret manager.
- With Hub stopped, back up the entire `hub-data` volume, operator grants and
  configuration to a private location. Restore into a separate disposable
  volume and verify project/ref visibility and a CLI pull before deleting the
  test copy. Keep backup files and credentials out of Git and screenshots.
- Confirm TLS, private API/web ports, expected container users, writeable data
  mounts, startup failures for incomplete hosted config, and actual release
  installer/checksum assets. Run the full release gate in CI.

The supplied container smoke check exercises image startup, non-root users,
read-only roots and proxy health. It does not validate live identity, TLS,
backup restoration or general production readiness. Source deployment guidance
is in [GETTING_STARTED.md](GETTING_STARTED.md#hosted-pilot-from-current-source).

### Pilot outreach

After those gates pass, recruit 3–5 agent-tool teams for a guided trial. Show
one real repository moving from a local lane to recorded review, with explicit
current integration/recovery limits. Measure successful first uploads/reviews,
time and assistance required, and use the next week. Prefer interviews and a
short demo over paid ads until onboarding works without operator help.

An announcement can describe an invite-only Sorrel Hub pilot for reviewing
human and agent changes while retaining local/Git workflows. State that Core
grants control access, integration remains manual, and the preview needs
disposable repositories. Invite concrete workflow feedback through the existing
GitHub issue forms. Do not claim market fit, enforced agent isolation or
production readiness without evidence.

For a later Show HN, provide something people can actually run; a signup-only
landing page is insufficient. Mathis should write and post in his own voice:
HN currently disallows generated/AI-edited posts and automated posting.
Check the [Show HN guidance](https://news.ycombinator.com/showhn.html) and
[posting rules](https://news.ycombinator.com/newsguidelines.html) again at launch.
Other communities need their own current posting rules checked. No outreach is
sent by deployment or release scripts.

### Versioned release

After checks pass on `main`:

1. Run the **Prepare changelogs** workflow with the next version and release
   date. It gathers merged PRs since the previous tag, categorizes their titles
   and labels, maps changed paths to packages, synchronizes the public root
   changelog mirror, and opens a PR. Contributors do not add fragments or run a
   changelog command.
2. Review and merge the generated changelog PR. Edit its prose when a change
   needs extra migration, security, or compatibility context. `skip-changelog`
   omits internal-only PRs; unrecognized title formats fall back to `Changed`.
3. Update `release/manifest.json` and all package versions together.
4. Confirm the release note extraction:

   ```sh
   npm run release:notes -- v0.1.0-alpha.2
   ```

5. Commit and merge the release candidate to `main`.
6. Create an annotated tag and push it:

   ```sh
   git tag -a v0.1.0-alpha.2 -m "Sorrel v0.1.0-alpha.2"
   git push origin v0.1.0-alpha.2
   ```

7. The tag starts `.github/workflows/release.yml` and
   `.github/workflows/server-images.yml` in parallel. The first builds each
   supported CLI binary and desktop app, produces checksums and installers,
   and creates the GitHub Release. The second validates the same tag against
   `release/manifest.json` and publishes attested Linux amd64/arm64
   `sorrel-hub` and `sorrel-hub-web` images. Prerelease-style versions are
   marked as prereleases automatically. Do not create a competing release
   manually while the workflows are running.
8. After the CLI workflow creates the GitHub Release, the server workflow
   attaches the release Compose file, immutable image digests, and their
   checksum file.
9. Verify both workflows, the published tag, CLI and desktop installer assets,
   server assets, public GHCR pulls, release URL, and changelog links. In clean
   temporary environments, run `sorrel --version`, `sorrel init`, and
   `sorrel status`; smoke-test the desktop app against a loopback Hub on every
   OS/architecture family; then start the release Compose stack and probe
   `/healthz` through both the API and `/api` proxy before announcing the
   release.

Tags are release anchors; do not move or recreate them. A correction gets a new
prerelease version.

`release.yml` is generated by the pinned `dist` version. When distribution
configuration changes, regenerate it with `dist generate`, then review both the
workflow and `dist plan` output before committing.

## After publication

Keep `Unreleased` at the top of each changelog. Release preparation regenerates
it from merged PRs and moves those entries into a new dated section. Review the
generated wording in the preparation PR; do not edit prior release entries
except to correct broken links or factually dangerous errors.

## Rollback

Check out the prior root tag. Back up `.sorrel/` and Hub data directories before
moving between alpha versions; no general storage migration framework exists yet.

## CI

Root Actions checks out this repository directly. No `SUBMODULES_TOKEN` is
required.
