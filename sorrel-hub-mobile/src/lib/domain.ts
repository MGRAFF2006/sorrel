export const PROPOSAL_TRANSITIONS: Readonly<Record<string, readonly string[]>> = {
  draft: ['open', 'closed'],
  open: ['approved', 'rejected', 'merged', 'closed', 'draft'],
  approved: ['merged', 'open', 'closed'],
  rejected: ['open', 'closed'],
  merged: ['closed'],
  closed: ['draft', 'open'],
};

export function normalizeBaseUrl(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) throw new Error('Enter a Hub URL.');

  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error('Enter a complete URL beginning with http:// or https://.');
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('Hub URLs must use HTTP or HTTPS.');
  }
  if (url.username || url.password) {
    throw new Error('Do not put credentials in the Hub URL.');
  }
  if (url.pathname !== '/' || url.search || url.hash) {
    throw new Error('Use the Hub base URL without a path, query, or fragment.');
  }
  return url.origin;
}

export function isInsecureConnection(baseUrl: string): boolean {
  return new URL(baseUrl).protocol === 'http:';
}

export function unwrapList<T>(payload: unknown): T[] {
  if (Array.isArray(payload)) return payload as T[];
  if (!payload || typeof payload !== 'object') return [];
  const record = payload as Record<string, unknown>;
  for (const key of ['data', 'items', 'projects', 'repos', 'refs']) {
    if (Array.isArray(record[key])) return record[key] as T[];
  }
  return [];
}

export function shortId(value: unknown, length = 12): string {
  if (typeof value !== 'string' || !value) return '—';
  return value.length > length ? value.slice(0, length) : value;
}

export function formatError(error: unknown): string {
  if (error && typeof error === 'object') {
    const body = (error as { body?: unknown }).body;
    if (body && typeof body === 'object') {
      const message = (body as { error?: { message?: unknown } }).error?.message;
      if (typeof message === 'string') return message;
    }
  }
  return error instanceof Error ? error.message : String(error);
}

export function tabletColumns(width: number): 1 | 2 {
  return width >= 720 ? 2 : 1;
}
