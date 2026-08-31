# sorrel-agents

Experimental, minimal local agent control plane for Sorrel.

Registers agents against lanes, records advisory path claims, and exposes an
"active work" view. Permissions remain Core's responsibility — this module only
tracks coordination state (optionally mirrored to a live Hub project).

This alpha is a coordination aid, not an enforcement boundary or a complete
agent orchestration system. Claims are advisory and state is local unless Hub
mirroring is configured. Expect the API and behavior to change before a stable
release.

## Usage

```js
import { AgentControlPlane } from '@sorrel/agents';

const plane = new AgentControlPlane({ hubUrl: 'http://127.0.0.1:3000' });
await plane.registerAgent({ id: 'agent_docs', lane: 'lane_main' });
await plane.claimPath({ agentId: 'agent_docs', path: 'README.md' });
console.log(await plane.activeWork());
```

## Checks

```sh
npm run check
```
