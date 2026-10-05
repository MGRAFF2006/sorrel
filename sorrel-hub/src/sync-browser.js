import { HttpError } from './http.js';
import { parseJsonObject, refObjectId } from './sync-closure.js';
import { SyncObjectNotFoundError } from './sync-store.js';

const BLOB_PREFIX = Buffer.from('sorrel.blob.v0\n', 'utf8');
const MAX_TEXT_FILE_BYTES = 512 * 1024;

/**
 * Resolve a ref and path to a protocol Tree without exposing raw object bytes.
 *
 * @param {string} repoId
 * @param {string} refName
 * @param {string} path
 * @param {import('./sync-store.js').RepoSyncStore} store
 */
export function browseTree(repoId, refName, path, store) {
  const location = resolveLocation(repoId, refName, path, store);
  const tree = requireObjectKind(store, repoId, location.objectId, 'tree');
  const entries = Array.isArray(tree.entries) ? tree.entries : [];

  return {
    repoId,
    ref: refName,
    path: location.path,
    snapshot: snapshotSummary(location.snapshotId, location.snapshot),
    entries: entries.map(normalizeTreeEntry).sort(compareTreeEntries),
  };
}

/**
 * Resolve a ref and path to a UTF-8 Sorrel blob.
 *
 * @param {string} repoId
 * @param {string} refName
 * @param {string} path
 * @param {import('./sync-store.js').RepoSyncStore} store
 */
export function browseTextFile(repoId, refName, path, store) {
  const segments = normalizeBrowsePath(path);
  if (segments.length === 0) {
    throw new HttpError(400, 'path must identify a file', 'invalid_request');
  }

  const parentPath = segments.slice(0, -1).join('/');
  const fileName = segments.at(-1);
  const location = resolveLocation(repoId, refName, parentPath, store);
  const tree = requireObjectKind(store, repoId, location.objectId, 'tree');
  const entry = findEntry(tree, fileName);

  if (!entry || entry.type === 'directory') {
    throw new HttpError(404, `file ${segments.join('/')} was not found`, 'path_not_found');
  }

  const objectId = requireEntryObjectId(entry);
  const preview = readTextBlob(repoId, objectId, store);
  return {
    repoId,
    ref: refName,
    path: segments.join('/'),
    objectId,
    ...preview,
    snapshot: snapshotSummary(location.snapshotId, location.snapshot),
  };
}

function readTextBlob(repoId, objectId, store) {
  const bytes = getObject(store, repoId, objectId);
  if (!bytes.subarray(0, BLOB_PREFIX.length).equals(BLOB_PREFIX)) {
    throw new HttpError(422, `object ${objectId} is not a Sorrel blob`, 'invalid_sync_object');
  }

  const content = bytes.subarray(BLOB_PREFIX.length);
  if (content.length > MAX_TEXT_FILE_BYTES) {
    throw new HttpError(413, 'file is too large to preview', 'file_too_large');
  }
  if (content.includes(0)) {
    throw new HttpError(415, 'file contains binary data', 'unsupported_file');
  }

  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(content);
  } catch {
    throw new HttpError(415, 'file is not valid UTF-8 text', 'unsupported_file');
  }

  return {
    size: content.length,
    encoding: 'utf-8',
    content: text,
  };
}

/** Read-only comparison of two recorded snapshots, never a merge decision. */
export function browseSnapshotChanges(proposal, store) {
  const { syncRepoId: repoId, sourceSnapshot, targetSnapshot } = proposal;
  if (!repoId || !isObjectId(sourceSnapshot) || !isObjectId(targetSnapshot)) {
    throw new HttpError(409, 'this review needs recorded source and target snapshots to compare', 'comparison_unavailable');
  }
  const before = snapshotFiles(repoId, targetSnapshot, store);
  const after = snapshotFiles(repoId, sourceSnapshot, store);
  const paths = [...new Set([...before.keys(), ...after.keys()])].sort();
  const changed = paths.filter((path) => before.get(path)?.objectId !== after.get(path)?.objectId || before.get(path)?.mode !== after.get(path)?.mode);
  if (changed.length > 500) throw new HttpError(413, 'review exceeds the 500-file preview limit', 'comparison_too_large');
  let remainingBytes = 4 * 1024 * 1024;
  const preview = (entry) => {
    if (!entry) return { content: null };
    try {
      const text = readTextBlob(repoId, entry.objectId, store);
      if (text.size > remainingBytes) return { objectId: entry.objectId, mode: entry.mode, content: null, reason: 'preview_limit' };
      remainingBytes -= text.size;
      return { objectId: entry.objectId, mode: entry.mode, content: text.content };
    } catch (error) {
      if (error instanceof HttpError && [413, 415].includes(error.statusCode)) {
        return { objectId: entry.objectId, mode: entry.mode, content: null, reason: error.code };
      }
      throw error;
    }
  };
  return {
    repoId, sourceSnapshot, targetSnapshot,
    changes: changed.map((path) => ({
      path,
      status: !before.has(path) ? 'added' : !after.has(path) ? 'deleted' : 'modified',
      before: preview(before.get(path)),
      after: preview(after.get(path)),
    })),
  };
}

function snapshotFiles(repoId, snapshotId, store) {
  const snapshot = requireObjectKind(store, repoId, snapshotId, 'snapshot');
  const root = refObjectId(snapshot.rootTree ?? snapshot.tree ?? snapshot.root);
  if (!isObjectId(root)) throw new HttpError(422, 'snapshot has no valid root tree', 'invalid_sync_object');
  const files = new Map();
  let entryCount = 0;
  function visit(objectId, prefix, ancestors) {
    if (ancestors.has(objectId) || ancestors.size >= 64) {
      throw new HttpError(422, 'snapshot tree is cyclic or too deep to preview', 'invalid_sync_object');
    }
    const tree = requireObjectKind(store, repoId, objectId, 'tree');
    if (!Array.isArray(tree.entries)) throw new HttpError(422, 'tree entries must be an array', 'invalid_sync_object');
    const nextAncestors = new Set([...ancestors, objectId]);
    const names = new Set();
    for (const raw of tree.entries) {
      if (++entryCount > 10000) throw new HttpError(413, 'snapshot exceeds the 10000-entry preview limit', 'comparison_too_large');
      const entry = normalizeTreeEntry(raw);
      if (normalizeBrowsePath(entry.name).length !== 1 || names.has(entry.name)) {
        throw new HttpError(422, 'tree entry name is invalid or duplicated', 'invalid_sync_object');
      }
      names.add(entry.name);
      const path = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.type === 'directory') visit(entry.objectId, path, nextAncestors);
      else files.set(path, entry);
    }
  }
  visit(root, '', new Set());
  return files;
}

function resolveLocation(repoId, refName, path, store) {
  const segments = normalizeBrowsePath(path);
  const snapshotId = store.getRef(repoId, refName);
  if (!snapshotId) {
    throw new HttpError(404, `ref ${refName} does not exist`, 'unknown_ref');
  }

  const snapshot = requireObjectKind(store, repoId, snapshotId, 'snapshot');
  let objectId = refObjectId(snapshot.rootTree ?? snapshot.tree ?? snapshot.root);
  if (!isObjectId(objectId)) {
    throw new HttpError(422, `snapshot ${snapshotId} has no valid root tree`, 'invalid_sync_object');
  }

  for (const [index, segment] of segments.entries()) {
    const tree = requireObjectKind(store, repoId, objectId, 'tree');
    const entry = findEntry(tree, segment);
    if (!entry || entry.type !== 'directory') {
      const missingPath = segments.slice(0, index + 1).join('/');
      throw new HttpError(404, `directory ${missingPath} was not found`, 'path_not_found');
    }
    objectId = requireEntryObjectId(entry);
  }

  return {
    snapshotId,
    snapshot,
    objectId,
    path: segments.join('/'),
  };
}

function normalizeBrowsePath(path) {
  if (typeof path !== 'string') {
    throw new HttpError(400, 'path must be a string', 'invalid_request');
  }
  if (path === '') {
    return [];
  }
  if (path.startsWith('/') || path.endsWith('/') || path.includes('\\')) {
    throw new HttpError(400, `path ${path} is invalid`, 'invalid_request');
  }

  const segments = path.split('/');
  if (segments.some((segment) => segment === '' || segment === '.' || segment === '..')) {
    throw new HttpError(400, `path ${path} is invalid`, 'invalid_request');
  }
  return segments;
}

function requireObjectKind(store, repoId, objectId, expectedKind) {
  const parsed = parseJsonObject(getObject(store, repoId, objectId));
  if (typeof parsed?.kind !== 'string' || parsed.kind.toLowerCase() !== expectedKind) {
    throw new HttpError(
      422,
      `object ${objectId} is not a ${expectedKind}`,
      'invalid_sync_object',
    );
  }
  return parsed;
}

function getObject(store, repoId, objectId) {
  try {
    return store.get(repoId, objectId);
  } catch (error) {
    if (error instanceof SyncObjectNotFoundError) {
      throw new HttpError(409, `object ${objectId} is missing`, 'closure_incomplete');
    }
    throw error;
  }
}

function findEntry(tree, name) {
  if (!Array.isArray(tree.entries)) {
    return undefined;
  }
  return tree.entries.find((entry) => entry?.name === name);
}

function requireEntryObjectId(entry) {
  const objectId = refObjectId(entry.object ?? entry.id ?? entry.hash);
  if (!isObjectId(objectId)) {
    throw new HttpError(422, 'tree entry has no valid object id', 'invalid_sync_object');
  }
  return objectId;
}

function normalizeTreeEntry(entry) {
  if (!entry || typeof entry !== 'object' || typeof entry.name !== 'string') {
    throw new HttpError(422, 'tree contains an invalid entry', 'invalid_sync_object');
  }

  return {
    name: entry.name,
    path: typeof entry.path === 'string' ? entry.path : entry.name,
    type: typeof entry.type === 'string' ? entry.type : 'file',
    mode: typeof entry.mode === 'string' ? entry.mode : null,
    size: Number.isInteger(entry.size) ? entry.size : null,
    objectId: requireEntryObjectId(entry),
  };
}

function compareTreeEntries(left, right) {
  const leftDirectory = left.type === 'directory' ? 0 : 1;
  const rightDirectory = right.type === 'directory' ? 0 : 1;
  return leftDirectory - rightDirectory || left.name.localeCompare(right.name);
}

function snapshotSummary(id, snapshot) {
  return {
    id,
    message: typeof snapshot.message === 'string' ? snapshot.message : null,
    createdAt: typeof snapshot.createdAt === 'string' ? snapshot.createdAt : null,
    author: snapshot.author ?? null,
    parents: Array.isArray(snapshot.parents)
      ? snapshot.parents.map(refObjectId).filter(isObjectId)
      : [],
  };
}

function isObjectId(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}
