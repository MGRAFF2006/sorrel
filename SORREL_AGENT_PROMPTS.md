# Sorrel Agent Prompt Pack

Use these prompts for the next Sorrel agents. Agents should work inside the
target submodule repository directly, merge submodule work into that submodule's
`main`, then update the root submodule pointer only after the submodule commit is
reachable from the corresponding `main` branch.

## Scope correction

The intended architecture is headless-first:

- Sorrel Core owns portable identity, permission, grant, policy decision,
  redaction, `SecretRef`, and audit semantics.
- Sorrel Hub is a collaboration product and administration surface on top of
  Core. It must not define the only permission model.
- Vaults, runners, CLI commands, slices, lanes, stacks, proposals, and Hub APIs
  should all use the same Core permission vocabulary.
- Raw secret values are never Sorrel objects. Core stores refs, schemas, grants,
  decisions, redaction metadata, and audit events.
- Decentralized does not mean permissionless: policy/grant/authority changes are
  signed `PolicyChange` objects evaluated against the previous effective policy.
  A principal cannot grant itself power unless it already has delegated authority.

## Prompt A: compatibility adaptation across completed foundations

```text
You are a Sorrel implementation agent. Work inside the relevant Sorrel
submodule repos directly, not primarily in the root repo.

Goal:
Adapt the existing completed foundations so they are compatible with the
headless Core permissions architecture. Do not add production auth, hosted
compute, marketplace, merge queue, or sophisticated conflict resolution.

Read first:
- Root `AGENT_NATIVE_VERSION_CONTROL_REPORT.md`
  - "Layer 2.5: Core identity, permissions, and policy spine"
  - "Permissions model"
  - "Secret management"
  - "Workflow permissions"
  - "MVP plan"
- Root `SORREL_PROGRESS.md`
- Target submodule README/AGENTS.md

Architecture invariant:
Sorrel Core defines Principal, Capability, Resource scope, Grant, Policy,
Authority, PolicyChange, Decision, SecretRef, Redaction, and AuditEvent
semantics. Hub, Vault, Runners, CLI, and Slices consume those semantics instead
of inventing their own permission model.

Security invariant:
Never evaluate a permission change using the permissions created by that same
change. Policy/grant/authority updates must be signed and evaluated against the
previous effective policy. Self-grants and scope-broadening delegations must be
denied unless the actor already has authority such as `policy.grant`,
`policy.delegate`, or `authority.admin` for the target resource.

Tasks:
1. In `sorrel-protocol`, add or update schemas/examples/validation for:
   - PrincipalId / PrincipalKind
   - Capability strings
   - ResourceRef / scope descriptors
   - Grant
   - Policy
   - Authority / authority root
   - PolicyChange
   - PolicyDecision
   - AuditEvent
   - SecretRef and redaction markers
2. In `sorrel-core`, add matching Rust model types and serialization tests.
   Include a minimal deterministic policy evaluator that can answer allow,
   deny, redact, needs_grant, and needs_review for in-memory inputs. Include
   explicit denial of self-escalation and unsigned authority changes.
3. In `sorrel-cli`, expose headless inspection/evaluation commands or mocked
   command shapes consistent with the core model. Prefer small commands such as
   `sorrel policy evaluate` and `sorrel grant create`.
4. In `sorrel-vault`, map existing grants/redaction work to Core Grant,
   SecretRef, PolicyDecision, and AuditEvent concepts. Do not store raw secret
   values in Sorrel objects.
5. In `sorrel-runners`, ensure JobBundle/capabilities refer to Core principals,
   required capabilities, runner.use/workflow.run decisions, and secret refs.
6. In `sorrel-slices`, ensure slice manifests preserve/project permissions
   through Core ResourceRef/Grant/Policy semantics, especially when extracting a
   subproject.
7. In `sorrel-hub`, update the skeleton domain model/API assumptions so orgs,
   projects, repos, proposals, review comments, workflow runs, and policies
   reference Core principals/policies. Hub may administer or display policy, but
   should not be the only source of truth.

Validation:
- Run each touched submodule's existing tests.
- Add focused tests for schema validation/model serialization/policy decisions.
- Record commands and results.

Deliverables:
- Submodule commits pushed to each affected submodule main or PRs ready to merge.
- A short report listing changed modules, commits, validation, and any blockers.
- If a root pointer update is needed, update the root submodule pointers only to
  commits reachable from submodule main.
```

## Prompt B: protocol permission schema agent

```text
Work in `sorrel-protocol`.

Implement the first public schema pass for Sorrel's headless Core permission
spine. Add schemas, examples, and validation for Principal, Capability,
ResourceRef, Authority, Grant, Policy, PolicyChange, PolicyDecision, AuditEvent,
SecretRef, and redaction metadata. Keep schemas versioned under the existing
protocol package structure.

Do not model production login/auth providers. This is portable authorization
data, not hosted identity management.

Acceptance:
- Examples cover an agent writing a path, a runner running a workflow, and a
  workflow requesting secret injection.
- Examples cover a denied self-grant and an allowed maintainer/threshold-signed
  grant.
- Invalid examples fail validation.
- README/docs explain that Hub consumes these objects rather than owning them.
- Existing validation commands pass.
```

## Prompt C: core permission engine agent

```text
Work in `sorrel-core`.

Build the minimal Rust model and evaluator for the Sorrel Core permission spine.
Add Principal, Capability, ResourceRef, Authority, Grant, PolicyChange,
PolicyDecision, SecretRef, Redaction, and AuditEvent types with serde support
and tests. Implement a small deterministic evaluator that takes
principal/action/resource/context/grants/policy roots and returns allow, deny,
redact, needs_grant, or needs_review.

Keep this headless and storage-agnostic. It must work in memory and must not
depend on Sorrel Hub.

Acceptance:
- Unit tests cover allow, deny, redaction, expired grant, path-scoped grant,
  workflow.run, runner.use, and secret.inject decisions.
- Unit tests prove self-grant/self-escalation is denied when evaluated against
  the previous effective policy.
- Unit tests prove delegated `policy.grant` cannot broaden beyond its delegated
  resource scope.
- Existing cargo build/test/clippy/fmt checks pass.
- README or module docs show how lanes, workflows, vault, and Hub should call
  the evaluator.
```

## Prompt D: CLI headless policy UX agent

```text
Work in `sorrel-cli` after the protocol/core permission vocabulary exists.

Add the first headless CLI surfaces for inspecting and exercising Core policy:
`sorrel policy evaluate`, `sorrel grant create`, `sorrel grant list`, and
`sorrel secret refs` if that fits the existing CLI structure. It is acceptable
to keep storage mocked if the current CLI is still mocked, but command shapes
must match the Core permission model.

Acceptance:
- CLI tests cover command parsing and output for an agent path.write decision,
  a workflow.run decision, and a secret.inject needs_grant decision.
- README documents local/headless usage without Hub.
```

## Prompt E: vault and runner policy integration agent

```text
Work in `sorrel-vault` and `sorrel-runners`.

Align vault grants/redaction and runner JobBundle execution with the Core
permission spine. JobBundle should declare required capabilities, principal
context, runner requirements, and SecretRef dependencies. Vault resolution should
require Core policy decisions for secret.read/secret.inject and should emit or
model audit events/redaction metadata.

Do not implement hosted compute, production auth, or real external secret
providers unless already scaffolded. Keep the integration local and testable.

Acceptance:
- Tests prove unauthorized runner/secret access is blocked or represented as
  needs_grant.
- Logs redact secret refs/values according to the shared model.
- README explains how Core policy is consulted.
```

## Prompt F: lanes/stacks with permission metadata agent

```text
Work in `sorrel-core` after the permission spine is available.

Implement Lane and Stack objects on top of Change and Snapshot, but include the
permission metadata from the start: owner principal, visibility, policy refs,
grant refs, touched ResourceRefs, and audit hooks. Focus on metadata,
serialization, touched paths/resources, and tests. Do not implement merge logic.

Acceptance:
- Lane/Stack serialization tests include private/redacted/team/review visibility.
- Touched paths/resources are expressed as ResourceRef values.
- Tests show an agent lane can be authorized for path.write through grants.
```

## Prompt G: Hub consumes Core policy agent

```text
Work in `sorrel-hub` only after the Core/protocol permission spine is available.

Expand the Hub skeleton so organizations, projects, repositories, proposals,
review comments, workflow runs, and policies reference Core principals, grants,
policy decisions, and audit events. Hub can expose administration endpoints, but
must not define a separate authorization language.

Do not add production auth, full merge queue, hosted compute, or secret values.

Acceptance:
- API handlers/tests show policy references on projects/proposals/workflow runs.
- Health and existing project APIs keep passing.
- README states Hub is a UI/server over Core policy semantics.
```

## Prompt H: webpage update for Core permissions story

```text
Work in `sorrel-web`.

Goal:
Update the public Sorrel landing page so it clearly communicates that Sorrel
Core is designed to own headless permissions, grants, policy decisions,
SecretRef/redaction semantics, and audit records from the foundation. This
should reassure readers that permissions are not bolted onto Sorrel Hub later.

Important nuance:
Do not claim this is fully implemented yet. Phrase it as "designed around",
"being built into Core", or "Core-native permission spine" unless the
implementation has landed. Also explain the decentralized trust idea in plain
developer language: local clones can edit bytes, but peers/runners/Hub/remotes
accept permission changes only when signed by an already-authorized authority
chain.

Content to add:
- A short section or card: "Core-native permissions"
- Mention principals: humans, agents, runners, workflows, services, and apps.
- Mention scoped grants and policy decisions for paths, lanes, slices, runners,
  workflows, environments, and secrets.
- Mention self-escalation prevention: agents cannot simply edit their own grant
  files to gain power; policy changes are evaluated against prior authority.
- Mention Hub as an administration/collaboration surface over Core policy, not
  the source of truth.
- Keep the page polished, concise, and credible for developer infrastructure.

Constraints:
- Keep the site static HTML/CSS/JS.
- Do not add a backend, package manager, or build step.
- Preserve existing architecture/report/GitHub/progress links.

Validation:
- Serve with `python3 -m http.server 4173 --bind 127.0.0.1`.
- Confirm HTTP 200 and that the new "Core-native permissions" copy appears.
- Run any existing static checks if present.

Deliverables:
- Commit and push to `sorrel-web/main` or open the submodule PR according to the
  current Sorrel operating rule.
- Report commit, validation commands/results, and whether a root pointer update
  is needed.
```
