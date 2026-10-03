/** Agent coordination only. Core decides permissions. */
import {
  closeSync, existsSync, fsyncSync, linkSync, lstatSync, mkdirSync,
  openSync, readFileSync, readdirSync, renameSync, rmdirSync, unlinkSync,
  writeFileSync,
} from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { dirname, join } from 'node:path';

function agentId(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(value)) {
    throw new Error('agent id must contain 1–64 ASCII letters, digits, underscores or hyphens');
  }
  return value;
}

function claimPath(value) {
  if (typeof value !== 'string' || !value || /[\x00-\x1f\x7f-\x9f]/.test(value)) {
    throw new Error('claim path must be a nonempty relative path');
  }
  const path = value.replaceAll('\\', '/');
  if (path.startsWith('/') || /^[A-Za-z]:/.test(path)) {
    throw new Error('claim path must be relative');
  }
  const parts = [];
  for (const part of path.split('/')) {
    if (!part || part === '.') continue;
    if (part === '..') {
      throw new Error('claim path must not contain parent traversal');
    } else {
      parts.push(part);
    }
  }
  if (!parts.length) throw new Error('claim path must name a file or directory');
  return parts.join('/');
}

function claimId(agent, path) {
  return createHash('sha256').update(JSON.stringify([agent, path])).digest('hex');
}

function stringField(record, key) {
  if (typeof record[key] !== 'string' || !record[key]) {
    throw new Error(`invalid ${key} in agent registry`);
  }
}

function validTimestamp(value) {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/.test(value) && Number.isFinite(Date.parse(value));
}

function validateAgent(record) {
  if (!record || typeof record !== 'object') throw new Error('invalid agent record');
  agentId(record.id);
  for (const key of ['lane', 'displayName', 'registeredAt']) stringField(record, key);
  if (!validTimestamp(record.registeredAt)) throw new Error('invalid registeredAt');
  for (const key of ['workspace', 'task']) {
    if (record[key] !== undefined) stringField(record, key);
  }
  return record;
}

function validateClaim(record, normalize = false) {
  if (!record || typeof record !== 'object') throw new Error('invalid claim record');
  agentId(record.agentId);
  const normalized = claimPath(record.path);
  if (!normalize && normalized !== record.path) throw new Error('noncanonical claim path');
  if (record.mode !== 'advisory') throw new Error('blocking claims are not implemented; use advisory');
  stringField(record, 'claimedAt');
  if (!validTimestamp(record.claimedAt)) throw new Error('invalid claimedAt');
  return { ...record, path: normalized };
}

function syncDirectory(path) {
  // Windows does not expose directory fsync through Node. Atomic replacement still applies.
  if (process.platform === 'win32') return;
  const fd = openSync(path, 'r');
  try { fsyncSync(fd); } finally { closeSync(fd); }
}

// Readers see either a complete old record or a complete new record. No whole-map rewrites.
function writeRecord(path, record, createOnly = false) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  const fd = openSync(temporary, 'wx', 0o600);
  try {
    writeFileSync(fd, `${JSON.stringify(record, null, 2)}\n`);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  try {
    if (createOnly) {
      try {
        linkSync(temporary, path);
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
      }
    } else {
      renameSync(temporary, path);
    }
  } finally {
    if (existsSync(temporary)) unlinkSync(temporary);
  }
  syncDirectory(dirname(path));
}

export class AgentControlPlane {
  /** @param {{workspace?: string, stateDir?: string}} options */
  constructor(options = {}) {
    this.workspace = options.workspace ?? null;
    this.stateDir = options.stateDir ??
      (this.workspace ? join(this.workspace, '.sorrel', 'agents') : null);
    this.agents = new Map();
    this.claims = new Map();
    if (this.stateDir) {
      for (const directory of [this.stateDir, join(this.stateDir, 'agents'), join(this.stateDir, 'claims')]) {
        mkdirSync(directory, { recursive: true });
        if (!lstatSync(directory).isDirectory()) throw new Error(`registry directory must not be a symlink: ${directory}`);
      }
      this.#migrate();
      this.#load();
    }
  }

  #migrate() {
    const path = join(this.stateDir, 'state.json');
    if (!existsSync(path)) return;
    const lock = join(this.stateDir, '.migration.lock');
    const deadline = Date.now() + 2000;
    while (true) {
      try {
        mkdirSync(lock);
        break;
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        if (Date.now() >= deadline) throw new Error('agent registry migration is locked; inspect .migration.lock before retrying');
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
      }
    }
    try {
      if (!existsSync(path)) return;
      const backup = join(this.stateDir, 'state.migrated.json');
      if (existsSync(backup)) throw new Error('legacy migration backup already exists; inspect state.json');
      if (!lstatSync(path).isFile()) throw new Error('legacy registry must be a regular file');
      const raw = JSON.parse(readFileSync(path, 'utf8'));
      if (!Array.isArray(raw.agents) || !Array.isArray(raw.claims)) throw new Error('invalid legacy agent registry');
      const agents = raw.agents.map(validateAgent);
      const claims = raw.claims.map((claim) => validateClaim(claim, true));
      const ids = new Set(agents.map((agent) => agent.id));
      if (claims.some((claim) => !ids.has(claim.agentId))) throw new Error('legacy claim references unknown agent');
      for (const agent of agents) writeRecord(join(this.stateDir, 'agents', `${agent.id}.json`), agent, true);
      for (const claim of claims) writeRecord(join(this.stateDir, 'claims', `${claimId(claim.agentId, claim.path)}.json`), claim, true);
      renameSync(path, backup);
      syncDirectory(this.stateDir);
    } finally {
      rmdirSync(lock);
    }
  }

  #loadRecords(kind, validate, key) {
    const records = new Map();
    const directory = join(this.stateDir, kind);
    for (const name of readdirSync(directory).filter((name) => name.endsWith('.json')).sort()) {
      const path = join(directory, name);
      let record;
      try {
        if (!lstatSync(path).isFile()) throw new Error('record must be a regular file');
        record = validate(JSON.parse(readFileSync(path, 'utf8')));
      } catch (error) {
        // A simultaneous release may remove a listed claim before it is read.
        if (error.code === 'ENOENT') continue;
        throw new Error(`invalid registry record ${path}: ${error.message}`, { cause: error });
      }
      const id = key(record);
      if (name !== `${id}.json`) throw new Error(`registry filename does not match record: ${path}`);
      records.set(id, record);
    }
    return records;
  }

  #load() {
    if (!this.stateDir) return;
    // Claims are published after their agent. Read them first so a concurrent
    // registration cannot make a valid new claim appear orphaned.
    this.claims = this.#loadRecords('claims', validateClaim, (record) => claimId(record.agentId, record.path));
    this.agents = this.#loadRecords('agents', validateAgent, (record) => record.id);
    for (const claim of this.claims.values()) {
      if (!this.agents.has(claim.agentId)) throw new Error(`claim references unknown agent ${claim.agentId}`);
    }
  }

  /** @param {{id: string, lane?: string, displayName?: string, workspace?: string, task?: string}} input */
  async registerAgent(input) {
    const id = agentId(input?.id);
    this.#load();
    const previous = this.agents.get(id);
    const agent = validateAgent({
      ...previous,
      ...input,
      id,
      lane: input.lane ?? previous?.lane ?? 'lane_main',
      displayName: input.displayName ?? previous?.displayName ?? id,
      registeredAt: previous?.registeredAt ?? new Date().toISOString(),
    });
    if (this.stateDir) writeRecord(join(this.stateDir, 'agents', `${id}.json`), agent);
    this.agents.set(id, agent);
    return { ...agent };
  }

  /** @param {{agentId: string, path: string, mode?: 'advisory' | 'blocking'}} input */
  async claimPath(input) {
    const id = agentId(input?.agentId);
    const path = claimPath(input?.path);
    this.#load();
    if (!this.agents.has(id)) throw new Error(`unknown agent ${id}`);
    const claim = validateClaim({ agentId: id, path, mode: input.mode ?? 'advisory', claimedAt: new Date().toISOString() });
    const key = claimId(id, path);
    if (this.stateDir) writeRecord(join(this.stateDir, 'claims', `${key}.json`), claim);
    this.claims.set(key, claim);
    return { ...claim };
  }

  /** @param {{agentId: string, path: string}} input */
  async releasePath(input) {
    const id = agentId(input?.agentId);
    const path = claimPath(input?.path);
    this.#load();
    const key = claimId(id, path);
    if (this.stateDir) {
      try {
        unlinkSync(join(this.stateDir, 'claims', `${key}.json`));
        syncDirectory(join(this.stateDir, 'claims'));
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
        return false;
      }
      this.claims.delete(key);
      return true;
    }
    return this.claims.delete(key);
  }

  async activeWork() {
    this.#load();
    const agents = [...this.agents.values()].map((agent) => ({ ...agent }));
    const claims = [...this.claims.values()].map((claim) => ({ ...claim }));
    const overlapping = new Map();
    // ponytail: pairwise comparison; use a path trie if thousands of active claims need it.
    for (let i = 0; i < claims.length; i++) {
      for (const other of claims.slice(i + 1)) {
        const claim = claims[i];
        if (claim.agentId === other.agentId) continue;
        const path = claim.path === other.path || other.path.startsWith(`${claim.path}/`) ? claim.path :
          claim.path.startsWith(`${other.path}/`) ? other.path : null;
        if (path === null) continue;
        if (!overlapping.has(path)) overlapping.set(path, new Set());
        overlapping.get(path).add(claim.agentId).add(other.agentId);
      }
    }
    const overlaps = [...overlapping].sort(([a], [b]) => a.localeCompare(b))
      .map(([path, ids]) => ({ path, agentIds: [...ids].sort() }));
    return { agents, claims, overlaps };
  }
}
