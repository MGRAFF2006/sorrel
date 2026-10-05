# sorrel-agents

Experimental, minimal local agent control plane for Sorrel.

Registers agents against lanes, records advisory path claims, and exposes an
"active work" view. Permissions remain Core's responsibility — this module only
tracks local coordination state.

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
console.log(await plane.activeWork());
```

## Checks

```sh
npm run check
```
