import fs from 'node:fs';
import path from 'node:path';
import { randomBytes } from 'node:crypto';

import { objectId, verifyObjectId } from './blake3.js';
import {
  SyncObjectIdMismatchError,
  SyncObjectNotFoundError,
} from './sync-store.js';

const OBJECT_ID_PATTERN = /^[0-9a-f]{64}$/;

/**
 * Encode an arbitrary identifier (repo id, ref name) into a filesystem-safe
 * single path segment. Alphanumerics, `-` and `_` pass through; every other
 * byte (including `.` and `/`) is percent-encoded, so distinct ids never
 * collide and no separator or traversal sequence can reach the disk.
 *
 * @param {string} value
 * @returns {string}
 */
export function encodePathSegment(value) {
  let out = '';
  for (const byte of Buffer.from(value, 'utf8')) {
    const char = String.fromCharCode(byte);
    if (/[A-Za-z0-9_-]/.test(char)) {
      out += char;
    } else {
      out += `%${byte.toString(16).padStart(2, '0')}`;
    }
  }
  return out === '' ? '%' : out;
}

/**
 * Inverse of {@link encodePathSegment}. Percent-encoded bytes are decoded as
 * UTF-8; the empty-string sentinel (`%`) round-trips back to `""`.
 *
 * @param {string} value
 * @returns {string}
 */
export function decodePathSegment(value) {
  if (typeof value !== 'string') {
    throw new TypeError('value must be a string');
  }
  if (value === '%') {
    return '';
  }

  const bytes = [];
  for (let i = 0; i < value.length; ) {
    if (value[i] === '%') {
      if (i + 2 >= value.length) {
        throw new Error('invalid percent-encoded path segment');
      }
      const hex = value.slice(i + 1, i + 3);
      if (!/^[0-9a-fA-F]{2}$/.test(hex)) {
        throw new Error('invalid percent-encoded path segment');
      }
      bytes.push(Number.parseInt(hex, 16));
      i += 3;
    } else {
      bytes.push(value.charCodeAt(i));
      i += 1;
    }
  }
  return Buffer.from(bytes).toString('utf8');
}

/**
 * Filesystem-backed drop-in replacement for the in-memory `RepoSyncStore`.
 *
 * Layout, mirroring sorrel-core's `FileObjectStore` semantics:
 *
 *   <rootDir>/<repo>/objects/<id[0..2]>/<id>   content-addressed bytes
 *   <rootDir>/<repo>/refs/<ref>                one JSON document per ref
 *
 * Writes are atomic (temp file + rename in the same directory) and object
 * reads are digest-verified, so a corrupted or tampered object surfaces as
 * an id mismatch instead of silently propagating bad bytes.
 *
 * The API is synchronous on purpose: it matches the in-memory store so the
 * routes and closure walker work with either implementation unchanged.
 */
export class FsRepoSyncStore {
  /** @param {string} rootDir */
  constructor(rootDir) {
    if (typeof rootDir !== 'string' || rootDir.trim() === '') {
      throw new TypeError('rootDir must be a non-empty string');
    }
    this.rootDir = path.resolve(rootDir);
    initializeStoreDirectory(this.rootDir);
  }

  #repoDir(repoId) {
    return path.join(this.rootDir, encodePathSegment(repoId));
  }

  #objectPath(repoId, id) {
    return path.join(this.#repoDir(repoId), 'objects', id.slice(0, 2), id);
  }

  #refPath(repoId, name) {
    return path.join(this.#repoDir(repoId), 'refs', encodePathSegment(name));
  }

  has(repoId, id) {
    return fs.existsSync(this.#objectPath(repoId, normalizeObjectId(id)));
  }

  get(repoId, id) {
    const normalized = normalizeObjectId(id);
    let bytes;
    try {
      bytes = fs.readFileSync(this.#objectPath(repoId, normalized));
    } catch (error) {
      if (error && error.code === 'ENOENT') {
        throw new SyncObjectNotFoundError();
      }
      throw error;
    }

    if (!verifyObjectId(normalized, bytes)) {
      throw new SyncObjectIdMismatchError('stored object bytes do not match id');
    }
    return bytes;
  }

  /**
   * Store content-addressed bytes. When `expectedId` is set, reject on mismatch.
   *
   * @param {string} repoId
   * @param {Buffer} bytes
   * @param {string | undefined} expectedId
   * @returns {string}
   */
  put(repoId, bytes, expectedId) {
    const id = objectId(bytes);
    if (expectedId !== undefined && id !== expectedId.toLowerCase()) {
      throw new SyncObjectIdMismatchError();
    }

    const target = this.#objectPath(repoId, id);
    if (!fs.existsSync(target) || !verifyObjectId(id, fs.readFileSync(target))) {
      atomicWrite(target, bytes, this.rootDir);
    } else {
      syncStoredFile(target, this.rootDir);
    }
    return id;
  }

  /** @returns {string[]} */
  listRepos() {
    let entries;
    try {
      entries = fs.readdirSync(this.rootDir, { withFileTypes: true });
    } catch (error) {
      if (error && error.code === 'ENOENT') {
        return [];
      }
      throw error;
    }

    const repos = [];
    for (const entry of entries) {
      if (entry.isDirectory()) {
        repos.push(decodePathSegment(entry.name));
      }
    }
    return repos;
  }

  listRefs(repoId) {
    const refsDir = path.join(this.#repoDir(repoId), 'refs');
    let files;
    try {
      files = fs.readdirSync(refsDir);
    } catch (error) {
      if (error && error.code === 'ENOENT') {
        return [];
      }
      throw error;
    }

    const refs = [];
    for (const file of files.sort()) {
      const parsed = readRefFile(path.join(refsDir, file));
      if (parsed) {
        refs.push(parsed);
      }
    }
    return refs;
  }

  getRef(repoId, name) {
    const parsed = readRefFile(this.#refPath(repoId, name));
    return parsed ? parsed.snapshot : undefined;
  }

  setRef(repoId, name, snapshotId, objectIds = []) {
    // HTTP passes its already validated closure, including terminal blobs.
    for (const id of objectIds) syncStoredFile(this.#objectPath(repoId, normalizeObjectId(id)), this.rootDir);
    const payload = JSON.stringify({ name, snapshot: snapshotId });
    atomicWrite(this.#refPath(repoId, name), Buffer.from(`${payload}\n`, 'utf8'), this.rootDir);
    return snapshotId;
  }
}

function normalizeObjectId(id) {
  const normalized = String(id).toLowerCase();
  if (!OBJECT_ID_PATTERN.test(normalized)) {
    throw new SyncObjectNotFoundError('object id is not a 64-character hex id');
  }
  return normalized;
}

function readRefFile(filePath) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    if (error && error.code === 'ENOENT') {
      return undefined;
    }
    throw error;
  }

  try {
    const value = JSON.parse(raw);
    if (
      value &&
      typeof value === 'object' &&
      typeof value.name === 'string' &&
      typeof value.snapshot === 'string'
    ) {
      return { name: value.name, snapshot: value.snapshot };
    }
  } catch {
    // fall through: a torn/corrupt ref file reads as absent rather than crashing
  }
  return undefined;
}

/**
 * Atomically write bytes to `target` via a same-directory temp file + rename.
 *
 * @param {string} target
 * @param {Buffer | string} bytes
 * @param {string} [rootDir] Directory boundary whose entries must be flushed.
 */
export function atomicWrite(target, bytes, rootDir = path.dirname(target)) {
  const dir = path.dirname(target);
  fs.mkdirSync(dir, { recursive: true });
  const tmp = path.join(dir, `.tmp-${process.pid}-${randomBytes(6).toString('hex')}`);
  let created = false;
  let published = false;
  try {
    const fd = fs.openSync(tmp, 'wx');
    created = true;
    try {
      fs.writeFileSync(fd, bytes);
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    fs.renameSync(tmp, target);
    published = true;
    syncDirectoryChain(dir, rootDir);
  } catch (error) {
    if (created && !published) {
      try {
        fs.rmSync(tmp, { force: true });
      } catch {
        // best effort cleanup of our unpublished temporary
      }
    }
    if (published) throw new PublishedWriteDurabilityError(error);
    throw error;
  }
}

/** A rename completed, but durability could not be established. */
export class PublishedWriteDurabilityError extends Error {
  constructor(cause) {
    super('published write durability could not be established', { cause });
    this.name = 'PublishedWriteDurabilityError';
  }
}

/** Reconcile root creation from earlier failed writes, including after restart. */
export function initializeStoreDirectory(rootDir) {
  const root = path.resolve(rootDir);
  fs.mkdirSync(root, { recursive: true });
  syncDirectoryChain(root, path.parse(root).root);
}

function syncDirectoryChain(directory, rootDir) {
  if (process.platform === 'win32') return;
  const root = path.resolve(rootDir);
  let current = path.resolve(directory);
  const relative = path.relative(root, current);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new TypeError('write directory must be inside the store root');
  }
  for (;;) {
    const fd = fs.openSync(current, 'r');
    try {
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    if (current === root) break;
    current = path.dirname(current);
  }
}

function syncStoredFile(target, rootDir) {
  if (process.platform === 'win32') return;
  const fd = fs.openSync(target, 'r');
  try {
    fs.fsyncSync(fd);
  } finally {
    fs.closeSync(fd);
  }
  syncDirectoryChain(path.dirname(target), rootDir);
}

/**
 * @param {string} rootDir
 * @returns {FsRepoSyncStore}
 */
export function createFsRepoSyncStore(rootDir) {
  return new FsRepoSyncStore(rootDir);
}
