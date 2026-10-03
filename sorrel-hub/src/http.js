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

export const MAX_JSON_BODY_BYTES = 16 * 1024 * 1024;

export async function readJsonBody(request) {
  let length = 0;
  if (Number(request.headers['content-length']) > MAX_JSON_BODY_BYTES) {
    throw new HttpError(413, 'request body exceeds 16 MiB', 'body_too_large');
  }
  const chunks = [];

  for await (const chunk of request) {
    length += chunk.length;
    if (length > MAX_JSON_BODY_BYTES) throw new HttpError(413, 'request body exceeds 16 MiB', 'body_too_large');
    chunks.push(chunk);
  }

  if (chunks.length === 0) {
    return {};
  }

  const rawBody = Buffer.concat(chunks).toString('utf8');
  if (rawBody.trim() === '') {
    return {};
  }

  try {
    const value = JSON.parse(rawBody);
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
    }
    return value;
  } catch (error) {
    if (error instanceof HttpError) throw error;
    throw new HttpError(400, 'request body must be valid JSON', 'invalid_json');
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
