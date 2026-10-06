/**
 * Walk snapshot closures and compute missing objects for sync transport.
 *
 * Understands both the simplified test shapes (`tree`/`parents` as hex strings)
 * and the protocol/Core shapes (`rootTree: { kind, id }`, `parents: [{ kind, id }]`,
 * tree `entries[].object: { kind, id }`).
 */

import { HttpError } from './http.js';
import { DEFAULT_LIMITS } from './resource-limits.js';

/** One budget per sync request, shared by closure and ancestry validation. */
export function createTraversalBudget(limits = DEFAULT_LIMITS) {
  let links = 0;
  let bytes = 0;
  const objects = new Set();
  const fail = () => { throw new HttpError(413, 'sync traversal exceeds configured safety limits', 'sync_traversal_too_large'); };
  return {
    link() { if (++links > limits.traversalLinks) fail(); },
    object(id) {
      objects.add(id);
      if (objects.size > limits.traversalObjects) fail();
    },
    bytes(size) { bytes += size; if (bytes > limits.traversalBytes) fail(); },
  };
}

const PROTOCOL_VERSION = 'sorrel.protocol.v0';
const TYPED_KINDS = { snapshot: 'Snapshot', tree: 'Tree' };

/**
 * @typedef {import('./sync-store.js').RepoSyncStore} RepoSyncStore
 */

/**
 * @param {Buffer} bytes
 * @returns {{ schemaVersion?: string, kind?: string, tree?: unknown, root?: unknown, rootTree?: unknown, parents?: unknown[], entries?: Array<Record<string, unknown>> } | null}
 */
export function parseJsonObject(bytes) {
  try {
    const value = JSON.parse(bytes.toString('utf8'));
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return null;
    }
    return value;
  } catch {
    return null;
  }
}

/** Match Core's exact persisted kind/version boundary for snapshots and trees. */
export function requireTypedObject(parsed, objectId, expectedKind) {
  if (!Object.hasOwn(TYPED_KINDS, expectedKind) || parsed?.kind !== TYPED_KINDS[expectedKind]) {
    throw new HttpError(422, `object ${objectId} is not a ${expectedKind}`, 'invalid_sync_object');
  }
  if (parsed.schemaVersion !== PROTOCOL_VERSION) {
    throw new HttpError(422, `object ${objectId} has an unsupported schemaVersion`, 'invalid_sync_object');
  }
  return parsed;
}

function normalizeId(value) {
  return typeof value === 'string' ? value.toLowerCase() : undefined;
}

/**
 * Extract a 64-hex object id from a string or `{ id }` / `{ kind, id }` ref.
 *
 * @param {unknown} value
 * @returns {string | undefined}
 */
export function refObjectId(value) {
  if (typeof value === 'string') {
    return normalizeId(value);
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return normalizeId(/** @type {{ id?: unknown }} */ (value).id);
  }
  return undefined;
}

function entryObjectId(entry) {
  if (!entry || typeof entry !== 'object') {
    return undefined;
  }
  return refObjectId(entry.object ?? entry.id ?? entry.hash);
}

function snapshotTreeId(parsed) {
  return (
    refObjectId(parsed.rootTree) ??
    refObjectId(parsed.tree) ??
    refObjectId(parsed.root)
  );
}

/**
 * Collect transitive object ids reachable from roots via snapshot/tree/blob links.
 *
 * @param {string} repoId
 * @param {string[]} rootIds
 * @param {RepoSyncStore} store
 * @param {string} [rootKind] required kind for stored roots (ref updates use snapshot)
 * @returns {{ closure: Set<string>, incomplete: boolean, missingIds: string[] }}
 */
export function walkClosure(repoId, rootIds, store, rootKind, budget = createTraversalBudget()) {
  const closure = new Set();
  const expanded = new Map();
  const missing = new Set();
  const pending = [];
  const enqueue = (link) => { budget.link(); pending.push(link); };
  for (const id of rootIds) enqueue({ id, expectedKind: rootKind });
  const verifiedBlobs = new Set();

  while (pending.length > 0) {
    const { id, expectedKind, terminal } = pending.pop();
    const normalized = normalizeId(id);
    if (!normalized || !/^[0-9a-f]{64}$/.test(normalized)) {
      throw new HttpError(422, 'closure contains an invalid object reference', 'invalid_sync_object');
    }
    budget.object(normalized);
    if (terminal && verifiedBlobs.has(normalized)) continue;
    if (!terminal && expanded.has(normalized)) {
      if (expectedKind && expanded.get(normalized) !== expectedKind) {
        throw new HttpError(422, `object ${normalized} is not a ${expectedKind}`, 'invalid_sync_object');
      }
      continue;
    }
    if (!store.has(repoId, normalized)) {
      missing.add(normalized);
      continue;
    }

    closure.add(normalized);
    // Terminal blobs still need the store's digest verification before publication.
    const bytes = store.get(repoId, normalized);
    budget.bytes(bytes.length);
    if (terminal) { verifiedBlobs.add(normalized); continue; }

    const parsed = parseJsonObject(bytes);
    const kind = typeof parsed?.kind === 'string' ? parsed.kind.toLowerCase() : undefined;
    if (expectedKind || kind === 'snapshot' || kind === 'tree') {
      requireTypedObject(parsed, normalized, expectedKind ?? kind);
    }
    expanded.set(normalized, kind);

    if (kind === 'snapshot') {
      const treeId = snapshotTreeId(parsed);
      if (!treeId || !Array.isArray(parsed.parents ?? [])) {
        throw new HttpError(422, `snapshot ${normalized} has invalid links`, 'invalid_sync_object');
      }
      enqueue({ id: treeId, expectedKind: 'tree' });
      for (const parent of parsed.parents ?? []) {
        enqueue({ id: refObjectId(parent), expectedKind: 'snapshot' });
      }
    } else if (kind === 'tree') {
      if (!Array.isArray(parsed.entries)) {
        throw new HttpError(422, `tree ${normalized} has invalid entries`, 'invalid_sync_object');
      }
      for (const entry of parsed.entries) {
        const directory = entry?.type === 'directory' ||
          (typeof entry?.object?.kind === 'string' && entry.object.kind.toLowerCase() === 'tree');
        const terminal = !directory && (entry?.type === 'file' ||
          (typeof entry?.object?.kind === 'string' && entry.object.kind.toLowerCase() === 'blob'));
        enqueue({ id: entryObjectId(entry), expectedKind: directory ? 'tree' : undefined, terminal });
      }
    }
  }

  return { closure, incomplete: missing.size > 0, missingIds: [...missing].sort() };
}

/**
 * Objects the server still needs to satisfy `want`.
 *
 * - Push: ids in `want` that are not yet stored (client should upload them).
 * - Pull: transitive closure of stored `want` roots minus `have` (client should download).
 *
 * @param {string[]} want
 * @param {string[]} have
 * @param {string} repoId
 * @param {RepoSyncStore} store
 * @returns {string[]}
 */
export function missingObjects(want, have, repoId, store, budget = createTraversalBudget()) {
  const haveSet = new Set(have.map((id) => id.toLowerCase()));
  const missing = new Set();

  const { closure, missingIds } = walkClosure(repoId, want, store, undefined, budget);
  for (const id of missingIds) {
    missing.add(id);
  }
  for (const id of closure) {
    if (!haveSet.has(id) && store.has(repoId, id)) {
      missing.add(id);
    }
  }

  return [...missing].sort();
}

/**
 * @param {string} repoId
 * @param {string[]} rootIds
 * @param {RepoSyncStore} store
 * @returns {boolean}
 */
export function isClosureComplete(repoId, rootIds, store) {
  const { incomplete } = walkClosure(repoId, rootIds, store);
  return !incomplete;
}

/**
 * True when `candidate` is the same snapshot as `ancestor` or lists it in parents.
 *
 * @param {string} repoId
 * @param {string} ancestorId
 * @param {string} candidateId
 * @param {RepoSyncStore} store
 * @returns {boolean}
 */
export function isDescendant(repoId, ancestorId, candidateId, store, budget = createTraversalBudget()) {
  const ancestor = ancestorId.toLowerCase();
  const candidate = candidateId.toLowerCase();

  if (ancestor === candidate) {
    return true;
  }

  const visited = new Set();
  const queue = [];
  const enqueue = (id) => { budget.link(); queue.push(id); };
  enqueue(candidate);

  while (queue.length > 0) {
    const current = queue.pop();
    if (!current || visited.has(current)) {
      continue;
    }
    visited.add(current);
    budget.object(current);

    if (!store.has(repoId, current)) {
      continue;
    }

    const bytes = store.get(repoId, current);
    budget.bytes(bytes.length);
    const parsed = parseJsonObject(bytes);
    if (!parsed || typeof parsed.kind !== 'string' || parsed.kind.toLowerCase() !== 'snapshot') {
      continue;
    }

    for (const parent of Array.isArray(parsed.parents) ? parsed.parents : []) {
      const parentId = refObjectId(parent);
      if (!parentId) {
        continue;
      }
      budget.link();
      budget.object(parentId);
      if (parentId === ancestor) {
        return true;
      }
      queue.push(parentId);
    }
  }

  return false;
}
