import { evaluateWithTrustedGrants } from '../core-policy.js';
import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { resolveActingPrincipal } from '../policy-guard.js';
import { browseTextFile, browseTree } from '../sync-browser.js';
import {
  isDescendant,
  missingObjects,
  walkClosure,
} from '../sync-closure.js';
import {
  SyncObjectIdMismatchError,
  SyncObjectNotFoundError,
} from '../sync-store.js';

export const POLICY_ACTION_OBJECT_WRITE = 'repo.object.write';
export const POLICY_ACTION_REF_WRITE = 'repo.ref.write';

/** Protocol `RepositoryId` pattern (sorrel-object.schema.json). */
const REPO_ID_PATTERN = /^(repo_[A-Za-z0-9_.:-]+|sorrel:\/\/[A-Za-z0-9_.:/-]+)$/;

/** Ref names: `HEAD`, `main`, `lane/main`, ... — no empty/`.`/`..` segments. */
const REF_SEGMENT_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

/**
 * @param {import('node:http').IncomingMessage} request
 * @param {import('node:http').ServerResponse} response
 * @param {{ url: URL, store: import('../store.js').InMemoryStore, trustedGrantsById?: Record<string, unknown> }} context
 */
export async function handleSyncRoute(request, response, context) {
  const segments = context.url.pathname.split('/').filter(Boolean);
  if (segments.length < 2) {
    throw new HttpError(404, 'sync route not found', 'not_found');
  }

  const repoId = parseRepoId(segments[0]);
  const resource = segments[1];

  if (resource === 'refs') {
    if (segments.length === 2) {
      if (request.method === 'GET') {
        return listRefs(response, context, repoId);
      }
      return sendMethodNotAllowed(response, ['GET']);
    }

    if (request.method === 'POST') {
      const refName = parseRefName(segments.slice(2));
      return await advanceRef(request, response, context, repoId, refName);
    }

    throw new HttpError(404, 'sync route not found', 'not_found');
  }

  if (resource === 'objects') {
    if (segments.length === 2 && request.method === 'POST') {
      return await uploadObjects(request, response, context, repoId);
    }

    if (segments.length === 3 && segments[2] === 'missing' && request.method === 'POST') {
      return await listMissing(request, response, context, repoId);
    }

    if (segments.length === 3 && request.method === 'GET') {
      return getObject(response, context, repoId, segments[2]);
    }

    throw new HttpError(404, 'sync route not found', 'not_found');
  }

  if (resource === 'tree' || resource === 'files') {
    if (segments.length !== 2) {
      throw new HttpError(404, 'sync route not found', 'not_found');
    }
    if (request.method !== 'GET') {
      return sendMethodNotAllowed(response, ['GET']);
    }

    const refName = parseRefName([context.url.searchParams.get('ref') ?? 'main']);
    const path = context.url.searchParams.get('path') ?? '';
    const result = resource === 'tree'
      ? browseTree(repoId, refName, path, context.store.sync)
      : browseTextFile(repoId, refName, path, context.store.sync);
    return sendJson(response, 200, result);
  }

  throw new HttpError(404, 'sync route not found', 'not_found');
}

/** Validates the repo scope segment against the protocol RepositoryId pattern. */
function parseRepoId(rawSegment) {
  const repoId = decodeURIComponent(rawSegment);
  if (!REPO_ID_PATTERN.test(repoId)) {
    throw new HttpError(400, `repo id ${repoId} is not a valid RepositoryId`, 'invalid_request');
  }
  return repoId;
}

/**
 * Joins and decodes remaining path segments into a ref name.
 *
 * Accepts both URL-encoded slashes (`refs/lane%2Fmain`, one segment) and
 * literal slashes (`refs/lane/main`, several segments).
 */
function parseRefName(rawSegments) {
  const name = rawSegments.map((segment) => decodeURIComponent(segment)).join('/');
  const segments = name.split('/');
  const valid =
    segments.length > 0 &&
    segments.every((segment) => REF_SEGMENT_PATTERN.test(segment));
  if (!valid) {
    throw new HttpError(400, `ref name ${name || '(empty)'} is invalid`, 'invalid_request');
  }
  return name;
}

function listRefs(response, context, repoId) {
  sendJson(response, 200, {
    repoId,
    refs: context.store.sync.listRefs(repoId),
  });
}

async function listMissing(request, response, context, repoId) {
  const body = await readJsonBody(request);
  const want = normalizeIdList(body.want, 'want');
  if (want.length === 0) {
    throw new HttpError(400, 'want must contain at least one snapshot id', 'invalid_request');
  }
  const have = normalizeIdList(body.have ?? [], 'have');

  const missing = missingObjects(want, have, repoId, context.store.sync);
  sendJson(response, 200, { missing });
}

async function uploadObjects(request, response, context, repoId) {
  const body = await readJsonBody(request);
  assertObjectUploadPolicy(request, body, repoId, context);

  const objects = body.objects;
  if (!Array.isArray(objects) || objects.length === 0) {
    throw new HttpError(400, 'objects must be a non-empty array', 'invalid_request');
  }

  const stored = [];
  const skipped = [];

  for (const [index, entry] of objects.entries()) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new HttpError(400, `objects[${index}] must be an object`, 'invalid_request');
    }

    const id = normalizeObjectId(entry.id, `objects[${index}].id`);
    const bytes = decodeObjectBytes(entry, index);
    if (context.store.sync.has(repoId, id)) {
      // Content-addressed writes are idempotent; verify the claimed id anyway.
      context.store.sync.put(repoId, bytes, id);
      skipped.push(id);
      continue;
    }
    context.store.sync.put(repoId, bytes, id);
    stored.push(id);
  }

  sendJson(response, 200, { stored, skipped });
}

function getObject(response, context, repoId, objectIdValue) {
  const id = normalizeObjectId(objectIdValue, 'object id');

  let bytes;
  try {
    bytes = context.store.sync.get(repoId, id);
  } catch (error) {
    if (error instanceof SyncObjectNotFoundError) {
      throw new HttpError(404, error.message, error.code);
    }
    throw error;
  }

  sendJson(response, 200, {
    id,
    bytes: Buffer.from(bytes).toString('base64'),
  });
}

async function advanceRef(request, response, context, repoId, refName) {
  const body = await readJsonBody(request);
  const actingPrincipal = resolveActingPrincipal(request, context);
  const snapshot = normalizeObjectId(body.snapshot, 'snapshot');
  const expected = body.expected === null || body.expected === undefined
    ? undefined
    : normalizeObjectId(body.expected, 'expected');
  const force = body.force === true;
  const grantRefs = requireGrantRefs(body);

  evaluateWithTrustedGrants(
    actingPrincipal,
    POLICY_ACTION_REF_WRITE,
    { kind: 'repo', id: repoId },
    grantRefs,
    context.trustedGrantsById ?? {},
    {
      policyRefs: body.policyRefs ?? [],
    },
  );

  const current = context.store.sync.getRef(repoId, refName);
  if (expected !== undefined) {
    if (current === undefined) {
      throw new HttpError(404, `ref ${refName} does not exist`, 'unknown_ref');
    }
    if (current !== expected) {
      throw new HttpError(
        400,
        `ref ${refName} expected snapshot does not match current`,
        'invalid_request',
        { current },
      );
    }
  }

  const { incomplete, missingIds } = walkClosure(repoId, [snapshot], context.store.sync);
  if (incomplete) {
    throw new HttpError(
      409,
      `snapshot ${snapshot} closure is missing ${missingIds.length} object(s)`,
      'closure_incomplete',
      { missing: missingIds },
    );
  }

  if (current && !force && !isDescendant(repoId, current, snapshot, context.store.sync)) {
    throw new HttpError(
      409,
      `snapshot ${snapshot} is not a descendant of ref ${refName}`,
      'non_fast_forward',
      { current },
    );
  }

  context.store.sync.setRef(repoId, refName, snapshot);

  sendJson(response, 200, {
    name: refName,
    snapshot,
    previous: current ?? null,
  });
}

function assertObjectUploadPolicy(request, body, repoId, context) {
  const actingPrincipal = resolveActingPrincipal(request, context);
  const grantRefs = requireGrantRefs(body);

  evaluateWithTrustedGrants(
    actingPrincipal,
    POLICY_ACTION_OBJECT_WRITE,
    { kind: 'repo', id: repoId },
    grantRefs,
    context.trustedGrantsById ?? {},
    {
      policyRefs: body.policyRefs ?? [],
    },
  );
}

/** Mutating sync requests MUST carry a `grantRefs` array (sync-transport spec). */
function requireGrantRefs(body) {
  if (!Array.isArray(body.grantRefs)) {
    throw new HttpError(400, 'grantRefs must be an array on mutating sync requests', 'invalid_request');
  }
  return body.grantRefs;
}

function normalizeIdList(value, fieldName) {
  if (!Array.isArray(value)) {
    throw new HttpError(400, `${fieldName} must be an array`, 'invalid_request');
  }

  return value.map((entry, index) => normalizeObjectId(entry, `${fieldName}[${index}]`));
}

function normalizeObjectId(value, fieldName) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new HttpError(400, `${fieldName} must be a non-empty string`, 'invalid_request');
  }

  const id = value.trim();
  if (!/^[0-9a-f]{64}$/.test(id)) {
    throw new HttpError(
      400,
      `${fieldName} must be a 64-character lowercase hex BLAKE3 id`,
      'invalid_request',
    );
  }

  return id;
}

function decodeObjectBytes(entry, index) {
  const data = entry.bytes ?? entry.data ?? entry.content;
  if (typeof data !== 'string' || data.length === 0) {
    throw new HttpError(400, `objects[${index}] requires base64 bytes`, 'invalid_request');
  }

  try {
    const bytes = Buffer.from(data, 'base64');
    if (bytes.length === 0) {
      throw new Error('empty');
    }
    return bytes;
  } catch {
    throw new HttpError(400, `objects[${index}].bytes must be valid base64`, 'invalid_request');
  }
}

export function mapSyncStoreError(error) {
  if (error instanceof SyncObjectIdMismatchError) {
    return new HttpError(400, error.message, error.code);
  }
  if (error instanceof SyncObjectNotFoundError) {
    return new HttpError(404, error.message, error.code);
  }
  return error;
}
