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

export async function readJsonBody(request) {
  const chunks = [];

  for await (const chunk of request) {
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
    return JSON.parse(rawBody);
  } catch {
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
