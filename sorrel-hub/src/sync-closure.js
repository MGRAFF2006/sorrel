import { HttpError } from './http.js';

const MAX_OBJECTS = 100_000;
const MAX_CLOSURE_BYTES = 256 * 1024 * 1024;
const ID_PATTERN = /^[0-9a-f]{64}$/;

export function parseJsonObject(bytes) {
  try {
    const value = JSON.parse(bytes.toString('utf8'));
    return value && typeof value === 'object' && !Array.isArray(value) ? value : null;
  } catch {
    return null;
  }
}

export function refObjectId(value) {
  const id = typeof value === 'string' ? value : value?.id;
  return typeof id === 'string' ? id.toLowerCase() : undefined;
}

function invalid(message) {
  throw new HttpError(409, message, 'invalid_closure');
}

function checkedRef(value, expectedKind) {
  const id = refObjectId(value);
  if (!id || !ID_PATTERN.test(id)) invalid('closure contains an invalid object reference');
  if (typeof value === 'object' && value?.kind !== expectedKind) {
    invalid(`closure reference must name ${expectedKind}`);
  }
  return id;
}

/** Iterative, bounded graph traversal. Ref publication requires Snapshot roots. */
export function walkClosure(repoId, rootIds, store, { snapshotRoots = false } = {}) {
  const closure = new Set();
  const missing = new Set();
  const seen = new Set();
  const queue = rootIds.map((id) => [id, snapshotRoots ? 'Snapshot' : undefined]);
  let bytesRead = 0;
  for (let index = 0; index < queue.length; index++) {
    const [rawId, expectedKind] = queue[index];
    const id = refObjectId(rawId);
    if (!id || !ID_PATTERN.test(id)) invalid('invalid closure root id');
    const key = `${id}:${expectedKind ?? ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    if (seen.size > MAX_OBJECTS || queue.length > MAX_OBJECTS) invalid('closure exceeds object limit');
    if (!store.has(repoId, id)) {
      missing.add(id);
      continue;
    }
    closure.add(id);
    const bytes = store.get(repoId, id);
    bytesRead += bytes.length;
    if (bytesRead > MAX_CLOSURE_BYTES) invalid('closure exceeds byte limit');
    // Blobs are opaque. JSON-looking file contents must never be followed as links.
    if (expectedKind === 'Blob') continue;
    const parsed = parseJsonObject(bytes);
    const kind = parsed?.kind;
    if (expectedKind && kind !== expectedKind) invalid(`expected ${expectedKind} object ${id}`);
    if (kind !== 'Snapshot' && kind !== 'Tree') continue;
    if (parsed.schemaVersion !== undefined && parsed.schemaVersion !== 'sorrel.protocol.v0') {
      invalid(`unsupported ${kind} schema version`);
    }
    if (kind === 'Snapshot') {
      const tree = parsed.rootTree ?? parsed.tree ?? parsed.root;
      queue.push([checkedRef(tree, 'Tree'), 'Tree']);
      if (!Array.isArray(parsed.parents)) invalid('Snapshot parents must be an array');
      for (const parent of parsed.parents) queue.push([checkedRef(parent, 'Snapshot'), 'Snapshot']);
    } else {
      if (!Array.isArray(parsed.entries)) invalid('Tree entries must be an array');
      const names = new Set();
      for (const entry of parsed.entries) {
        if (!entry || typeof entry !== 'object' || typeof entry.name !== 'string' ||
            !entry.name || /[\\/\0]/.test(entry.name) || ['.', '..', '.sorrel', '.git'].includes(entry.name.toLowerCase()) ||
            names.has(entry.name)) invalid('Tree contains invalid or duplicate entry names');
        names.add(entry.name);
        const ref = entry.object ?? entry.id ?? entry.hash;
        const childKind = typeof ref === 'object' ? ref?.kind : entry.type === 'directory' ? 'Tree' : 'Blob';
        if (!['Tree', 'Blob'].includes(childKind)) invalid('Tree children must be Tree or Blob objects');
        if ((entry.type === 'directory' && childKind !== 'Tree') ||
            (entry.type === 'file' && childKind !== 'Blob')) invalid('Tree entry type does not match reference');
        queue.push([checkedRef(ref, childKind), childKind]);
      }
    }
    if (queue.length > MAX_OBJECTS) invalid('closure exceeds object limit');
  }
  return { closure, incomplete: missing.size > 0, missingIds: [...missing].sort() };
}

export function missingObjects(want, have, repoId, store) {
  const haveSet = new Set(have.map((id) => id.toLowerCase()));
  const { closure, missingIds } = walkClosure(repoId, want, store);
  return [...new Set([...missingIds, ...[...closure].filter((id) => !haveSet.has(id))])].sort();
}

export function isClosureComplete(repoId, rootIds, store) {
  return !walkClosure(repoId, rootIds, store, { snapshotRoots: true }).incomplete;
}

export function isDescendant(repoId, ancestorId, candidateId, store) {
  const ancestor = ancestorId.toLowerCase();
  const visited = new Set();
  const queue = [candidateId.toLowerCase()];
  for (let index = 0; index < queue.length; index++) {
    const id = queue[index];
    if (visited.has(id)) continue;
    visited.add(id);
    if (visited.size > MAX_OBJECTS || queue.length > MAX_OBJECTS) invalid('ancestry exceeds object limit');
    if (id === ancestor) return true;
    if (!store.has(repoId, id)) continue;
    const parsed = parseJsonObject(store.get(repoId, id));
    if (parsed?.kind !== 'Snapshot') invalid('ancestry must contain Snapshot objects');
    if (!Array.isArray(parsed.parents)) invalid('Snapshot parents must be an array');
    for (const parent of parsed.parents) queue.push(checkedRef(parent, 'Snapshot'));
  }
  return false;
}
