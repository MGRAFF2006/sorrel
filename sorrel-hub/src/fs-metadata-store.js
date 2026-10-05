import fs from 'node:fs';
import path from 'node:path';

import { atomicWrite, encodePathSegment } from './fs-sync-store.js';
import { InMemoryStore } from './store.js';

/**
 * Collection directory names under the metadata root. Keys match the Map
 * property names on InMemoryStore.
 */
const COLLECTIONS = [
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
 * On construction every readable record is loaded into the same in-memory
 * Maps as InMemoryStore. Each mutation writes the record atomically before
 * publishing it in memory. Corrupt or unreadable files are skipped
 * with a warning so a bad document never takes the process down.
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
    fs.mkdirSync(this.rootDir, { recursive: true });
    this.#hydrate();
  }

  storeRecord(collection, record) {
    this.#persist(collection, record);
    super.storeRecord(collection, record);
  }

  #recordPath(collection, id) {
    return path.join(this.rootDir, collection, `${encodePathSegment(id)}.json`);
  }

  #persist(collection, record) {
    const payload = `${JSON.stringify(record)}\n`;
    atomicWrite(this.#recordPath(collection, record.id), payload);
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
    if (value && typeof value === 'object' && !Array.isArray(value) && typeof value.id === 'string') {
      return value;
    }
    console.warn(`fs-metadata-store: skipping invalid record file ${filePath}: missing string id`);
  } catch (error) {
    console.warn(`fs-metadata-store: skipping corrupt record file ${filePath}: ${error.message}`);
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
