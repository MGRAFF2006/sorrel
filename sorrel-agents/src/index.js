/**
 * Agent control plane — coordination only. Core decides permissions.
 */

import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

export class AgentControlPlane {
  /**
   * @param {{ hubUrl?: string, workspace?: string, stateDir?: string }} options
   */
  constructor(options = {}) {
    this.hubUrl = options.hubUrl ? options.hubUrl.replace(/\/$/, '') : null;
    this.workspace = options.workspace ?? null;
    this.stateDir =
      options.stateDir ??
      (this.workspace ? join(this.workspace, '.sorrel', 'agents') : null);
    /** @type {Map<string, object>} */
    this.agents = new Map();
    /** @type {Map<string, object>} */
    this.claims = new Map();
    if (this.stateDir) {
      mkdirSync(this.stateDir, { recursive: true });
      this.#load();
    }
  }

  #statePath() {
    return this.stateDir ? join(this.stateDir, 'state.json') : null;
  }

  #load() {
    const path = this.#statePath();
    if (!path || !existsSync(path)) {
      return;
    }
    const raw = JSON.parse(readFileSync(path, 'utf8'));
    for (const agent of raw.agents ?? []) {
      this.agents.set(agent.id, agent);
    }
    for (const claim of raw.claims ?? []) {
      this.claims.set(`${claim.agentId}:${claim.path}`, claim);
    }
  }

  #persist() {
    const path = this.#statePath();
    if (!path) {
      return;
    }
    writeFileSync(
      path,
      `${JSON.stringify(
        {
          agents: [...this.agents.values()],
          claims: [...this.claims.values()],
        },
        null,
        2,
      )}\n`,
    );
  }

  /**
   * @param {{ id: string, lane?: string, displayName?: string }} input
   */
  async registerAgent(input) {
    if (!input?.id) {
      throw new Error('registerAgent requires id');
    }
    const agent = {
      id: input.id,
      lane: input.lane ?? 'lane_main',
      displayName: input.displayName ?? input.id,
      registeredAt: new Date().toISOString(),
    };
    this.agents.set(agent.id, agent);
    this.#persist();

    if (this.hubUrl) {
      // Mirror registration as a Hub project note via POST /projects when possible.
      try {
        await fetch(`${this.hubUrl}/projects`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            organizationId: 'org_agents',
            name: `Agent ${agent.id}`,
            description: `lane=${agent.lane}`,
          }),
        });
      } catch {
        // Hub may be unreachable in unit tests without network; local state still holds.
      }
    }
    return agent;
  }

  /**
   * @param {{ agentId: string, path: string, mode?: 'advisory' | 'blocking' }} input
   */
  async claimPath(input) {
    if (!input?.agentId || !input?.path) {
      throw new Error('claimPath requires agentId and path');
    }
    if (!this.agents.has(input.agentId)) {
      throw new Error(`unknown agent ${input.agentId}`);
    }
    const claim = {
      agentId: input.agentId,
      path: input.path,
      mode: input.mode ?? 'advisory',
      claimedAt: new Date().toISOString(),
    };
    this.claims.set(`${claim.agentId}:${claim.path}`, claim);
    this.#persist();
    return claim;
  }

  async activeWork() {
    return {
      agents: [...this.agents.values()],
      claims: [...this.claims.values()],
    };
  }
}
