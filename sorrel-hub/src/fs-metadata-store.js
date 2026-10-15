import fs from 'node:fs';
import path from 'node:path';

import { atomicWrite, decodePathSegment, encodePathSegment, filesystemName, initializeStoreDirectory, PublishedWriteDurabilityError } from './fs-sync-store.js';
import { InMemoryStore } from './store.js';

/**
 * Collection directory names under the metadata root. Keys match the Map
 * property names on InMemoryStore.
 */
const INTEGRATION_COLLECTIONS = ['proposalAuthors', 'proposalBindings', 'proposalApprovals', 'proposalChecks', 'proposalResults', 'integrationOperations'];
const COLLECTIONS = [
  ...INTEGRATION_COLLECTIONS,
  'organizations',
  'projects',
  'repositories',
  'proposals',
  'reviewComments',
  'workflowRuns',
  'policies',
];

/**
 * Filesystem-backed product metadata store.
 *
 * Layout:
 *
 *   <rootDir>/<collection>/<id>.json   one JSON document per record
 *
 * On construction records with matching canonical filenames/IDs load into the
 * same in-memory Maps as InMemoryStore. Each mutation writes the record atomically before
 * publishing it in memory. Corrupt JSON, unreadable files, and invalid record
 * identities are skipped with warnings. Other collection fields are not normalized
 * or repaired during hydration.
 * Post-rename durability failures adopt the published record but still throw.
 *
 * Public methods match InMemoryStore exactly so routes stay unchanged.
 */
export class FsMetadataStore extends InMemoryStore {
  /** @param {string} rootDir */
  constructor(rootDir, options = {}) {
    super(options);
    if (typeof rootDir !== 'string' || rootDir.trim() === '') {
      throw new TypeError('rootDir must be a non-empty string');
    }
    this.rootDir = path.resolve(rootDir);
    initializeStoreDirectory(this.rootDir);
    this.#hydrate();
  }

  storeRecord(collection, record) {
    try {
      this.#persist(collection, record);
    } catch (error) {
      // A post-rename failure must not leave live reads behind the disk state.
      if (error instanceof PublishedWriteDurabilityError) super.storeRecord(collection, record);
      throw error;
    }
    super.storeRecord(collection, record);
  }

  #recordPath(collection, id) {
    return path.join(this.rootDir, collection, filesystemName(`${encodePathSegment(id)}.json`));
  }

  #persist(collection, record) {
    const payload = `${JSON.stringify(record)}\n`;
    atomicWrite(this.#recordPath(collection, record.id), payload, this.rootDir);
  }

  reload() {
    const previous = new Map(COLLECTIONS.map(name => [name, new Map(this[name])]));
    for (const collection of COLLECTIONS) this[collection].clear();
    this.#hydrate();
    // Preserve optional JS fields/references when the authoritative bytes match.
    for (const collection of COLLECTIONS) for (const [id, record] of this[collection]) {
      const old = previous.get(collection).get(id);
      if (old && JSON.stringify(old) === JSON.stringify(record)) this[collection].set(id, old);
    }
  }

  #hydrate() {
    for (const collection of COLLECTIONS) {
      const map = this[collection];
      const dir = path.join(this.rootDir, collection);
      let files;
      try {
        files = fs.readdirSync(dir);
      } catch (error) {
        if (error && error.code === 'ENOENT') {
          continue;
        }
        if (INTEGRATION_COLLECTIONS.includes(collection)) throw new Error('integration evidence collection is unreadable');
        console.warn(`fs-metadata-store: skipping unreadable collection ${collection}: ${error.message}`);
        continue;
      }

      for (const file of files) {
        if (!file.endsWith('.json') || file.startsWith('.')) {
          continue;
        }

        const filePath = path.join(dir, file);
        const record = readRecordFile(filePath);
        if (!record) {
          if (INTEGRATION_COLLECTIONS.includes(collection)) throw new Error('integration evidence is corrupt; operator repair required');
          continue;
        }

        map.set(record.id, record);
      }
    }
  }
}

/**
 * @param {string} filePath
 * @returns {object | undefined}
 */
function readRecordFile(filePath) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    console.warn(`fs-metadata-store: skipping unreadable file ${filePath}: ${error.message}`);
    return undefined;
  }

  try {
    const value = JSON.parse(raw);
    if (!value || typeof value !== 'object' || Array.isArray(value) ||
        typeof value.id !== 'string' || !value.id.trim() || value.id !== value.id.trim() ||
        value.id === '.' || value.id === '..' || !value.id.isWellFormed()) {
      console.warn(`fs-metadata-store: skipping invalid record file ${filePath}: invalid record identity`);
      return undefined;
    }
    const filename = path.basename(filePath);
    if (filename !== `${encodePathSegment(value.id)}.json` ||
        decodePathSegment(filename.slice(0, -5)) !== value.id) {
      console.warn(`fs-metadata-store: skipping invalid record file ${filePath}: filename/identity mismatch`);
      return undefined;
    }
    return value;
  } catch {
    console.warn(`fs-metadata-store: skipping corrupt record file ${filePath}: invalid record encoding`);
  }
  return undefined;
}

/**
 * @param {string} rootDir
 * @param {object} [options]
 * @returns {FsMetadataStore}
 */
export function createFsMetadataStore(rootDir, options = {}) {
  return new FsMetadataStore(rootDir, options);
}
