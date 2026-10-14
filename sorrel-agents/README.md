# sorrel-agents

Experimental, minimal local agent control plane for Sorrel.

Registers agents against lanes, records leased advisory path claims, and exposes
an "active work" view with overlap warnings. Permissions remain Core's
responsibility — this module only tracks local coordination state.

This alpha is a coordination aid, not an enforcement boundary or a complete
agent orchestration system. Claims are advisory and state is local. Expect the
API and behavior to change before a stable release.

Writers sharing a state directory serialize updates and reload before saving;
`activeWork()` reads the latest atomic state. A failed save leaves the prior
state intact. A busy writer is retried for up to five seconds, then rejected
with the lock path. If a process crashes, check that no writer is running
before removing its `state.lock` file and retrying. The lock contains the
writer's process id; never remove an active writer's lock.

## Usage

```js
import { AgentControlPlane } from '@sorrel/agents';

const plane = new AgentControlPlane({ workspace: process.cwd() });
await plane.registerAgent({ id: 'agent_docs', lane: 'lane_main' });
await plane.claimPath({ agentId: 'agent_docs', path: 'README.md' });
// Renew this agent's still-active claims for another five minutes.
const renewed = await plane.heartbeat({ agentId: 'agent_docs', ttlMs: 5 * 60 * 1000 });
const { agents, claims, warnings } = await plane.activeWork();
console.log({ renewed, agents, claims, warnings });
await plane.releaseClaim({ agentId: 'agent_docs', path: 'README.md' });
```

## Claim lifecycle

- `claimPath({ agentId, path, mode?, ttlMs? })` requires a registered agent and
  returns the claim, including `claimedAt` and `expiresAt` ISO timestamps. New
  claims default to **15 minutes** (`DEFAULT_CLAIM_TTL_MS` is exported). Optional
  `ttlMs` must be a positive safe integer in milliseconds whose resulting expiry
  fits a JavaScript date. Claiming the same owner/path again explicitly replaces
  its record and starts a new lease, including after expiry or release.
- `heartbeat({ agentId, ttlMs? })` returns the registered agent's renewed active
  claims. It preserves `claimedAt`, sets `renewedAt`, and sets `expiresAt` relative
  to the current time using the same default and TTL validation. It never revives
  expired or released claims. Unknown agents are rejected; an agent with no
  active claims receives `[]`.
- `releaseClaim({ agentId, path })` deletes only that owner's named claim and
  returns `true` when removed, or `false` if absent, including repeated calls.
  It can also delete an expired claim or an opaque legacy path recorded verbatim.
- `activeWork()` returns `{ agents, claims, warnings }`. Registrations are
  identities, not evidence of live work: agents remain listed after their claims
  expire. Claims are inactive **at** their expiry timestamp and are omitted from
  both the active view and warnings. Expired records remain on disk until an
  explicit release or replacement; there is no cleanup service.

`warnings` contains `{ type: 'overlap', claims: [first, second] }` for each pair
of active claims from different agents on the same path, or an ancestor and
descendant path. Component boundaries matter: `src/a` overlaps `src/a/file`,
but not `src/ab`. Warnings are returned even for `mode: 'blocking'`: the existing
mode field is metadata only and never rejects another claim or enforces policy.

New paths are relative repository paths. `/` and `\\` are separators; repeated
separators, `.` components and trailing separators are normalized. Empty/root
paths, control characters, absolute/UNC paths, Windows drive forms, and any `..`
component are rejected. Names such as `one:file` remain valid. Matching is
case-sensitive and lexical; no filesystem, symlink, symbol, or permission
resolution is performed. This normalization is not a sandbox.

## Persistence and limitations

Existing persisted claims **without `expiresAt` remain active indefinitely**.
Reading does not rewrite them, infer expiry from `claimedAt`, or destructively
migrate state. Explicit heartbeat adopts a lease for still-active legacy claims;
release removes them. Their recorded paths and timestamps remain readable.
Valid legacy paths are normalized only for overlap comparisons; invalid opaque
legacy paths are compared exactly. Release accepts recorded legacy paths verbatim.

Lease and renewal timestamps survive restart. All mutations, including release
and heartbeat, use the same atomic save and cross-process lock, reloading the
latest state under the lock before updating it. Failed saves roll back cached
mutations. Readers refresh from the atomic state file.

Leases use the caller's wall clock (`now: () => milliseconds` is an optional
constructor clock for deterministic tests). There is no timer, scheduler, clock
synchronization, authenticated ownership, or lease service. The caller supplies
the owner ID; sharing this local state is a coordination agreement. Core remains
the authority for access and writes, regardless of claims or overlap warnings.

## Checks

```sh
npm run check
```
