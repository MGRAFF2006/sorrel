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

export const DEFAULT_CLAIM_TTL_MS = 15 * 60 * 1000;

function normalizeClaimPath(path) {
  if (typeof path !== 'string' || !path.trim() || /[\x00-\x1f\x7f]/.test(path)) {
    throw new Error('claim path must be a non-empty relative repository path');
  }
  const relative = path.replaceAll('\\', '/');
  if (relative.startsWith('/') || /^[a-z]:/i.test(relative) || relative.split('/').includes('..')) {
    throw new Error('claim path must be relative and cannot contain traversal');
  }
  const normalized = relative.split('/').filter((part) => part && part !== '.').join('/');
  if (!normalized.trim()) throw new Error('claim path must name a repository path');
  if (/^[a-z]:/i.test(normalized)) throw new Error('claim path cannot use a Windows drive form');
  return normalized;
}

function recordedPath(path) {
  try {
    return normalizeClaimPath(path);
  } catch {
    // Opaque legacy paths remain addressable for explicit release.
    return path;
  }
}

function claimTtl(input) {
  const ttl = input.ttlMs === undefined ? DEFAULT_CLAIM_TTL_MS : input.ttlMs;
  if (!Number.isSafeInteger(ttl) || ttl <= 0) {
    throw new Error('ttlMs must be a positive safe integer in milliseconds');
  }
  return ttl;
}

function isActive(claim, now) {
  // Legacy records have no lease; reading them must not silently expire them.
  return claim.expiresAt === undefined || Date.parse(claim.expiresAt) > now;
}

function overlaps(first, second) {
  try {
    const a = normalizeClaimPath(first.path);
    const b = normalizeClaimPath(second.path);
    return a === b || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
  } catch {
    // Keep old opaque paths readable, but only compare invalid legacy paths exactly.
    return first.path === second.path;
  }
}

export class AgentControlPlane {
  /**
   * @param {{ workspace?: string, stateDir?: string, now?: () => number }} options
   */
  constructor(options = {}) {
    this.now = options.now ?? Date.now;
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
    return this.#mutate(() => {
      const agent = {
        id: input.id,
        lane: input.lane ?? 'lane_main',
        displayName: input.displayName ?? input.id,
        registeredAt: new Date(this.now()).toISOString(),
      };
      this.agents.set(agent.id, agent);
      return agent;
    });
  }

  /**
   * @param {{ agentId: string, path: string, mode?: 'advisory' | 'blocking', ttlMs?: number }} input
   */
  async claimPath(input) {
    if (!input?.agentId || !input?.path) {
      throw new Error('claimPath requires agentId and path');
    }
    const path = normalizeClaimPath(input.path);
    const ttl = claimTtl(input);
    return this.#mutate(() => {
      if (!this.agents.has(input.agentId)) {
        throw new Error(`unknown agent ${input.agentId}`);
      }
      const now = this.now();
      const claim = {
        agentId: input.agentId,
        path,
        mode: input.mode ?? 'advisory',
        claimedAt: new Date(now).toISOString(),
        expiresAt: new Date(now + ttl).toISOString(),
      };
      this.#releaseClaims(claim.agentId, path);
      this.claims.set(JSON.stringify([claim.agentId, claim.path]), claim);
      return claim;
    });
  }

  #releaseClaims(agentId, path) {
    let removed = false;
    for (const [key, claim] of this.claims) {
      if (claim.agentId === agentId && recordedPath(claim.path) === path) {
        this.claims.delete(key);
        removed = true;
      }
    }
    return removed;
  }

  /** Release only the named owner's claim; repeated releases return false. */
  async releaseClaim(input) {
    if (!input?.agentId || typeof input.path !== 'string' || !input.path) {
      throw new Error('releaseClaim requires agentId and path');
    }
    const path = recordedPath(input.path);
    return this.#mutate(() => this.#releaseClaims(input.agentId, path));
  }

  /** Renew an existing agent's active claims, never expired or released claims.
   * @param {{ agentId: string, ttlMs?: number }} input
   */
  async heartbeat(input) {
    if (!input?.agentId) throw new Error('heartbeat requires agentId');
    const ttl = claimTtl(input);
    return this.#mutate(() => {
      if (!this.agents.has(input.agentId)) throw new Error(`unknown agent ${input.agentId}`);
      const now = this.now();
      const renewedAt = new Date(now).toISOString();
      const expiresAt = new Date(now + ttl).toISOString();
      const renewed = [];
      for (const [key, claim] of this.claims) {
        if (claim.agentId === input.agentId && isActive(claim, now)) {
          const next = { ...claim, renewedAt, expiresAt };
          this.claims.set(key, next);
          renewed.push(next);
        }
      }
      return renewed;
    });
  }

  async activeWork() {
    if (this.stateDir) this.#load();
    const now = this.now();
    const claims = [...this.claims.values()].filter((claim) => isActive(claim, now));
    const warnings = [];
    // ponytail: pairwise scan; index paths only if coordination volume warrants it.
    for (let index = 0; index < claims.length; index++) {
      for (const other of claims.slice(index + 1)) {
        const claim = claims[index];
        if (claim.agentId !== other.agentId && overlaps(claim, other)) {
          warnings.push({ type: 'overlap', claims: [claim, other] });
        }
      }
    }
    return {
      agents: [...this.agents.values()],
      claims,
      warnings,
    };
  }
}
