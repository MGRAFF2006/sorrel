/**
 * Agent control plane — coordination only. Core decides permissions.
 */

import { randomUUID } from 'node:crypto';
import {
  closeSync, existsSync, fsyncSync, mkdirSync, openSync,
  readFileSync, renameSync, rmSync, writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

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
      this.agents.clear();
      this.claims.clear();
      return;
    }
    const raw = JSON.parse(readFileSync(path, 'utf8'));
    this.agents = new Map((raw.agents ?? []).map((agent) => [agent.id, agent]));
    this.claims = new Map((raw.claims ?? []).map((claim) => [JSON.stringify([claim.agentId, claim.path]), claim]));
  }

  #persist() {
    const path = this.#statePath();
    if (!path) {
      return;
    }
    const temporary = join(this.stateDir, `.state-${randomUUID()}.tmp`);
    try {
      const file = openSync(temporary, 'wx');
      try {
        writeFileSync(file, `${JSON.stringify({
          agents: [...this.agents.values()],
          claims: [...this.claims.values()],
        }, null, 2)}\n`);
        fsyncSync(file);
      } finally {
        closeSync(file);
      }
      renameSync(temporary, path);
    } finally {
      rmSync(temporary, { force: true });
    }
  }

  async #mutate(update) {
    if (!this.stateDir) return update();
    const lock = join(this.stateDir, 'state.lock');
    const deadline = Date.now() + 5000;
    let descriptor;
    for (;;) {
      try {
        descriptor = openSync(lock, 'wx');
        break;
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        if (Date.now() >= deadline) {
          throw new Error(`Agent state is busy: ${lock}. Retry after the writer finishes; remove the lock only after confirming no writer is running.`);
        }
        await delay(10);
      }
    }
    try {
      writeFileSync(descriptor, `${process.pid}\n`);
      this.#load();
      const agents = new Map(this.agents);
      const claims = new Map(this.claims);
      try {
        const result = update();
        this.#persist();
        return result;
      } catch (error) {
        this.agents = agents;
        this.claims = claims;
        throw error;
      }
    } finally {
      closeSync(descriptor);
      rmSync(lock);
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
    return this.#mutate(() => {
      this.agents.set(agent.id, agent);
      return agent;
    });
  }

  /**
   * @param {{ agentId: string, path: string, mode?: 'advisory' | 'blocking' }} input
   */
  async claimPath(input) {
    if (!input?.agentId || !input?.path) {
      throw new Error('claimPath requires agentId and path');
    }
    const claim = {
      agentId: input.agentId,
      path: input.path,
      mode: input.mode ?? 'advisory',
      claimedAt: new Date().toISOString(),
    };
    return this.#mutate(() => {
      if (!this.agents.has(input.agentId)) {
        throw new Error(`unknown agent ${input.agentId}`);
      }
      this.claims.set(JSON.stringify([claim.agentId, claim.path]), claim);
      return claim;
    });
  }

  async activeWork() {
    if (this.stateDir) this.#load();
    return {
      agents: [...this.agents.values()],
      claims: [...this.claims.values()],
    };
  }
}
