import { objectId, verifyObjectId } from './blake3.js';

export class SyncObjectIdMismatchError extends Error {
  constructor(message = 'object id does not match content hash') {
    super(message);
    this.name = 'SyncObjectIdMismatchError';
    this.code = 'object_id_mismatch';
  }
}

export class SyncObjectNotFoundError extends Error {
  constructor(message = 'object not found') {
    super(message);
    this.name = 'SyncObjectNotFoundError';
    this.code = 'object_not_found';
  }
}

export class SyncRefCorruptError extends Error {
  constructor() {
    super('persisted ref record is corrupt');
    this.name = 'SyncRefCorruptError';
    this.code = 'corrupt_sync_ref';
  }
}

export class RepoSyncStore {
  constructor() {
    /** @type {Map<string, { objects: Map<string, Buffer>, refs: Map<string, string> }>} */
    this.repos = new Map();
  }

  /** @returns {{ objects: Map<string, Buffer>, refs: Map<string, string> }} */
  #repo(repoId) {
    if (!this.repos.has(repoId)) {
      this.repos.set(repoId, { objects: new Map(), refs: new Map() });
    }
    return this.repos.get(repoId);
  }

  has(repoId, id) {
    return this.repos.get(repoId)?.objects.has(id) ?? false;
  }

  get(repoId, id) {
    const bytes = this.repos.get(repoId)?.objects.get(id);
    if (!bytes) {
      throw new SyncObjectNotFoundError();
    }
    return Buffer.from(bytes);
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
    if (!verifyObjectId(id, bytes)) {
      throw new SyncObjectIdMismatchError();
    }
    this.#repo(repoId).objects.set(id, Buffer.from(bytes));
    return id;
  }

  /** @returns {string[]} */
  listRepos() {
    return [...this.repos.keys()];
  }

  listRefs(repoId) {
    const refs = this.repos.get(repoId)?.refs;
    if (!refs) return [];
    return [...refs.entries()].map(([name, snapshot]) => ({ name, snapshot }));
  }

  getRef(repoId, name) {
    return this.repos.get(repoId)?.refs.get(name);
  }

  setRef(repoId, name, snapshotId) {
    this.#repo(repoId).refs.set(name, snapshotId);
    return snapshotId;
  }
}

export function createRepoSyncStore() {
  return new RepoSyncStore();
}
