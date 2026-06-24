# Sorrel authority hardening — submodule agent prompts

Run these **inside each submodule repo**, not from the root monorepo.

## Workflow

1. Open or clone the submodule repo (e.g. `sorrel-protocol`) as its own workspace.
2. Check out that repo's `main` and pull latest.
3. Copy the matching prompt below into a new agent session **in that repo**.
4. Let the agent commit, push to the submodule's `main`, and report test results.
5. After all submodules are done, run **Prompt F** once from the root repo to update submodule pointers only.

Do **not** implement everything from the root repo. The root only coordinates pointers.

---

## Prompt A — `sorrel-protocol` (run first)

```text
You are a Sorrel implementation agent working ONLY in the sorrel-protocol repository.

Goal: Verify and add protocol schemas/examples/tests for decentralized authority and PolicyChange.

Read first:
- This repo's README if present
- Parent repo docs if available: AGENT_NATIVE_VERSION_CONTROL_REPORT.md sections "Layer 2.6" and "Permissions model"

Core security invariant:
A PolicyChange must be evaluated against the PREVIOUS effective policy, not permissions introduced by the change itself.

Required schemas/examples (add if missing):
- AuthorityRoot (threshold, weighted authorities, policy.grant, authority.rotate)
- PolicyChange (previousPolicyRoot, proposedPolicyRoot, signatures, operation)
- Capabilities: policy.grant, policy.delegate, authority.admin, authority.rotate

Required examples:
- Valid authority root
- Denied self-grant (agent grants itself secret.inject + policy.grant → deny)
- Allowed delegated grant (maintainer grants agent path.write within scope)
- Denied scope broadening (delegate tries to grant repo-wide policy.grant)
- Root authority rotation with threshold signatures
- Unsigned policy change marked untrusted/denied

Tasks:
1. Add or update JSON schemas under the existing package structure.
2. Add examples for every case above.
3. Ensure invalid examples fail validation.
4. Add tests; run npm run validate and npm test.

Constraints:
- No production auth or hosted compute.
- Keep changes focused on authority/PolicyChange vocabulary.

Deliverable:
- Commit on a feature branch, open PR to this repo's main (or push main if that is your workflow).
- Report: files changed, commands run, pass/fail counts.
```

---

## Prompt B — `sorrel-core` (after protocol, or in parallel if types are clear)

```text
You are a Sorrel implementation agent working ONLY in the sorrel-core repository.

Goal: Harden the policy evaluator so principals cannot self-grant or apply unsigned policy changes.

Read first:
- sorrel-protocol schemas/examples for AuthorityRoot and PolicyChange (clone or web)
- AGENT_NATIVE_VERSION_CONTROL_REPORT.md "Layer 2.6" if available

Core security invariant:
Never evaluate a permission change using grants created by that same change.
evaluate_policy_change() must receive previous_grants only.

Tasks:
1. Inspect existing permission/authority code.
2. Add or harden types: AuthorityRoot, PolicyChange, AuthoritySignature, ProposedGrant, previousPolicyRoot, proposedPolicyRoot.
3. Implement evaluate_policy_change(change, authority_root, previous_grants, context).
4. Add tests proving:
   - self-grant denied without policy.grant / policy.delegate / authority.admin
   - unsigned change → untrusted/denied
   - forged signature → untrusted/denied
   - valid delegated grant succeeds
   - delegated grant cannot exceed received scope
   - authority.rotate requires configured threshold
   - proposed grants are never used as actor authority

Validation:
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check

Constraints:
- Headless, deterministic, no Hub dependency.
- Prefer focused tests over large abstractions.
- No production crypto required; deterministic test signatures are OK for now.

Deliverable:
- Commit + PR (or push) to this repo's main.
- Report: commits, test counts, any gaps remaining.
```

---

## Prompt C — `sorrel-cli`

```text
You are a Sorrel implementation agent working ONLY in the sorrel-cli repository.

Goal: Expose headless policy surfaces that use sorrel-core decisions (not local grant text alone).

Depends on: sorrel-core with evaluate() and evaluate_policy_change() on its main branch.

Tasks:
1. Add or extend commands:
   - sorrel policy evaluate --principal ... --action ... --resource ...
   - sorrel policy change apply (or similar) for PolicyChange evaluation
2. Wire commands to sorrel-core; do not invent a separate permission model.
3. Add tests/integration tests showing:
   - denied self-grant output (decision: deny, clear reason)
   - allowed delegated grant output
   - unsigned change output (untrusted/denied)

Validation:
cargo test

Constraints:
- Mocked storage is OK if CLI is still mocked, but decisions must come from Core.
- No production auth.

Deliverable:
- Commit + PR to this repo's main.
- Report: commands added, example JSON output for denied self-grant.
```

---

## Prompt D — `sorrel-vault`

```text
You are a Sorrel implementation agent working ONLY in the sorrel-vault repository.

Goal: Ensure secret.read / secret.inject depend on trusted Core policy decisions.

Depends on: sorrel-core permission evaluator on main.

Tasks:
1. Inspect how vault resolves secrets today.
2. Before returning a secret value, require Core evaluate() → Allow for secret.inject (and secret.read if applicable).
3. Reject or return needs_grant when only local grant YAML/text exists without a trusted Core decision.
4. Keep redaction for logs/metadata.
5. Add tests:
   - unauthorized principal → blocked / needs_grant
   - authorized workflow/principal → allowed
   - local grant file alone does not bypass Core

Validation:
npm test

Constraints:
- No raw secrets in Sorrel objects.
- No production external vault providers unless already scaffolded.

Deliverable:
- Commit + PR to this repo's main.
- Report: call sites, test results.
```

---

## Prompt E — `sorrel-runners`

```text
You are a Sorrel implementation agent working ONLY in the sorrel-runners repository.

Goal: Ensure workflow.run, runner.use, and secret refs in JobBundle require trusted Core decisions.

Depends on: sorrel-core permission evaluator on main.

Tasks:
1. Inspect JobBundle and runner gate logic.
2. Before executing a bundle, evaluate via Core:
   - workflow.run on the workflow resource
   - runner.use on the runner resource
   - secret.inject for each secret ref
3. Do not trust runner config or local permissions YAML alone.
4. Add tests for blocked run (no grant) and allowed run (trusted grants).

Validation:
cargo test

Constraints:
- No hosted compute.
- Keep portable JobBundle model.

Deliverable:
- Commit + PR to this repo's main.
- Report: gate logic location, test results.
```

---

## Prompt F — `sorrel-hub`

```text
You are a Sorrel implementation agent working ONLY in the sorrel-hub repository.

Goal: Hub must reference and administer Core policy — not become a separate source of truth.

Depends on: sorrel-core (and ideally sorrel-protocol) on main.

Tasks:
1. Inspect org/repo/proposal/workflow models and routes.
2. Ensure repos carry policyRef / authorityRootRef pointing at Core objects.
3. Admin or privileged actions call Core evaluate() with hydrated trusted grants.
4. Do not store permissions that Core cannot run headlessly.
5. Add tests:
   - repo exposes Core policy refs
   - unauthorized agent denied for policy.grant
   - authorized maintainer allowed via Core grants

Validation:
npm test

Constraints:
- No production auth.
- Skeleton expansion only; no merge queue or marketplace.

Deliverable:
- Commit + PR to this repo's main.
- Report: models/routes changed, test results.
```

---

## Prompt G — root repo pointer update (run last)

```text
You are a Sorrel orchestration agent working in the ROOT sorrel monorepo only.

Goal: Update submodule pointers after authority hardening merged into each submodule's main.

Do NOT re-implement protocol/core/cli/vault/runners/hub here.

Prerequisites — verify each submodule main has merged:
- sorrel-protocol
- sorrel-core
- sorrel-cli
- sorrel-vault
- sorrel-runners
- sorrel-hub

Tasks:
1. For each submodule: git -C <submodule> fetch origin main && confirm merged commit SHA.
2. Update root submodule pointer: git add <submodule>
3. Commit: "Point submodules at authority hardening commits"
4. Open root PR to main.

Validation:
- Each pointer commit must be reachable from that submodule's origin/main.
- Do not point at commits only on feature branches.

Deliverable:
- Root PR link, table of submodule → commit SHA, confirmation no implementation duplicated in root.
```

---

## Quick reference — security invariants (all prompts)

```text
1. PolicyChange is evaluated against PREVIOUS effective policy.
2. No self-grant without policy.grant, policy.delegate, or authority.admin.
3. Delegation cannot broaden scope beyond what was received.
4. authority.rotate requires threshold (or configured root rule).
5. Unsigned/forged changes are untrusted.
6. Vault, runners, Hub, CLI call Core — they do not trust local grant text alone.
```

---

## Suggested order

| Order | Repo | Prompt |
| --- | --- | --- |
| 1 | sorrel-protocol | A |
| 2 | sorrel-core | B |
| 3 | sorrel-cli | C |
| 4 | sorrel-vault | D |
| 5 | sorrel-runners | E |
| 6 | sorrel-hub | F |
| 7 | root sorrel | G |

Prompts C–F can run in parallel after sorrel-core merges.
