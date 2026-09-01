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
  const bytes = getObject(store, repoId, objectId);
  if (!bytes.subarray(0, BLOB_PREFIX.length).equals(BLOB_PREFIX)) {
    throw new HttpError(422, `object ${objectId} is not a Sorrel blob`, 'invalid_sync_object');
  }

  const content = bytes.subarray(BLOB_PREFIX.length);
  if (content.length > MAX_TEXT_FILE_BYTES) {
    throw new HttpError(413, 'file is too large to preview', 'file_too_large');
  }

  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(content);
  } catch {
    throw new HttpError(415, 'file is not valid UTF-8 text', 'unsupported_file');
  }

  return {
    repoId,
    ref: refName,
    path: segments.join('/'),
    objectId,
    size: content.length,
    encoding: 'utf-8',
    content: text,
    snapshot: snapshotSummary(location.snapshotId, location.snapshot),
  };
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
  if (parsed?.kind?.toLowerCase() !== expectedKind) {
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
