# sorrel-agents

Minimal local agent coordination for Sorrel. Agents register a lane and optional
workspace/task, claim relative paths, inspect overlapping work, and release
claims. Permissions remain Core's responsibility. Claims are advisory;
`mode: 'blocking'` fails because enforcement is not implemented.

## Usage

```js
import { AgentControlPlane } from '@sorrel/agents';

const plane = new AgentControlPlane({ workspace: process.cwd() });
await plane.registerAgent({
  id: 'agent_docs',
  lane: 'lane_docs',
  task: 'Update examples',
});
await plane.claimPath({ agentId: 'agent_docs', path: 'docs' });
console.log(await plane.activeWork()); // { agents, claims, overlaps }
await plane.releasePath({ agentId: 'agent_docs', path: 'docs' }); // true if removed
```

IDs contain 1–64 ASCII letters, digits, underscores or hyphens. Paths use `/`
separators after normalization (`\\`, repeated separators and `.` are accepted).
Absolute paths, empty paths, control characters and any `..` component are
rejected. A directory claim overlaps claims below that directory; `src/lib`
does not overlap `src/library`. Overlaps have the shape `{ path, agentIds }` and
include only different agents. Registration updates preserve `registeredAt`.

## Persistence and concurrency

The CLI and this package share `.sorrel/agents/`:

- `agents/<id>.json`: `id`, `lane`, `displayName`, `registeredAt`, optional
  `workspace` and `task` strings.
- `claims/<sha256>.json`: `agentId`, normalized `path`, `mode: 'advisory'`,
  `claimedAt`. The filename is the lowercase SHA-256 of UTF-8
  `JSON.stringify([agentId, path])`.

Each write replaces one complete record atomically; operations reload records
so separately running instances cannot overwrite unrelated agents or claims.
Concurrent updates to the same record use the last completed replacement.
Active work reflects live records, but is not a transaction spanning all agents
and claims. Claims are read before agents so concurrent registrations remain
valid. Corrupt or mismatched records fail visibly. Record contents are flushed
before replacement; containing directories are also flushed on Unix. Node does
not expose directory flushing on Windows.

Legacy `state.json` is automatically migrated without overwriting existing
records. The original becomes `state.migrated.json` after migration completes.
An interrupted migration can be retried safely because existing records are
preserved. Migration uses `.migration.lock`; if a crashed process leaves that
directory behind, first verify no migration is still running, then remove the
empty lock directory and retry. Keep the backup until migration is verified;
restoring it as `state.json` can restore previously released legacy claims.

Without `workspace` or `stateDir`, the control plane keeps state in memory only.
The API and record layout remain experimental.

## Checks

```sh
npm run check
```
