import { DEFAULT_LIMITS } from './resource-limits.js';

export class HttpError extends Error {
  /**
   * @param {number} statusCode
   * @param {string} message
   * @param {string} [code]
   * @param {Record<string, unknown>} [details] extra keys merged into the
   *   error envelope's `error` object (e.g. `current`, `missing`).
   */
  constructor(statusCode, message, code = 'http_error', details = undefined) {
    super(message);
    this.name = 'HttpError';
    this.statusCode = statusCode;
    this.code = code;
    this.details = details;
  }
}

export async function readJsonBody(request, maxBytes = DEFAULT_LIMITS.requestBodyBytes) {
  const declaredLength = request.headers?.['content-length'];
  if (declaredLength !== undefined && Number(declaredLength) > maxBytes) {
    throw new HttpError(413, 'request body exceeds configured safety limit', 'request_body_too_large');
  }
  const chunks = [];
  let size = 0;

  for await (const chunk of request.iterator({ destroyOnReturn: false })) {
    size += Buffer.byteLength(chunk);
    if (size > maxBytes) {
      throw new HttpError(413, 'request body exceeds configured safety limit', 'request_body_too_large');
    }
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }

  if (chunks.length === 0) {
    return {};
  }

  const rawBody = Buffer.concat(chunks).toString('utf8');
  if (rawBody.trim() === '') {
    return {};
  }

  let body;
  try {
    body = JSON.parse(rawBody);
  } catch {
    throw new HttpError(400, 'request body must be valid JSON', 'invalid_json');
  }
  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }
  return body;
}

export function decodePathComponent(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    throw new HttpError(400, 'path must use valid percent encoding', 'invalid_request');
  }
}

export function sendJson(response, statusCode, payload, headers = {}) {
  const body = JSON.stringify(payload);

  response.writeHead(statusCode, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(body),
    ...headers,
  });
  response.end(body);
}

export function sendNotFound(response) {
  sendJson(response, 404, {
    error: {
      code: 'not_found',
      message: 'route not found',
    },
  });
}

export function sendMethodNotAllowed(response, allowedMethods) {
  sendJson(
    response,
    405,
    {
      error: {
        code: 'method_not_allowed',
        message: `method must be one of: ${allowedMethods.join(', ')}`,
      },
    },
    {
      allow: allowedMethods.join(', '),
    },
  );
}
