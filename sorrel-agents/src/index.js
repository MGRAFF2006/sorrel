/**
 * Agent control plane — coordination only. Core decides permissions.
 */

import { mkdirSync, readFileSync, writeFileSync, existsSync, renameSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';

export class AgentControlPlane {
  /**
   * @param {{ workspace?: string, stateDir?: string }} options
   */
  constructor(options = {}) {
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
      this.claims.set(JSON.stringify([claim.agentId, claim.path]), claim);
    }
  }

  #persist() {
    const path = this.#statePath();
    if (!path) {
      return;
    }
    const temporary = join(this.stateDir, `.state-${randomUUID()}.tmp`);
    try {
      writeFileSync(
        temporary,
        `${JSON.stringify(
          {
            agents: [...this.agents.values()],
            claims: [...this.claims.values()],
          },
          null,
          2,
        )}\n`,
      );
      renameSync(temporary, path);
    } finally {
      rmSync(temporary, { force: true });
    }
  }

  #setAndPersist(map, key, value) {
    const previous = map.get(key);
    map.set(key, value);
    try {
      this.#persist();
    } catch (error) {
      if (previous === undefined) map.delete(key);
      else map.set(key, previous);
      throw error;
    }
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
    this.#setAndPersist(this.agents, agent.id, agent);
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
    this.#setAndPersist(this.claims, JSON.stringify([claim.agentId, claim.path]), claim);
    return claim;
  }

  async activeWork() {
    return {
      agents: [...this.agents.values()],
      claims: [...this.claims.values()],
    };
  }
}
