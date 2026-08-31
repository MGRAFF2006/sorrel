export type Principal = { type: string; id: string };

export const LOCAL_PRINCIPAL: Principal = { type: 'user', id: 'local' };

export type HubCapabilities = {
  modules: {
    core: boolean;
    actions: boolean;
    agents: boolean;
    secrets: boolean;
    objectStorage: 'fs' | 's3';
  };
  auth: {
    mode: 'dev' | 'workos' | 'oidc';
    /** Present when sessions are established (never WorkOS on hot path). */
    session?: 'cookie' | 'bearer' | 'none';
  };
  convex: {
    enabled: boolean;
    url?: string;
  };
  deploy: 'saas' | 'selfhost' | 'dev';
};

export type HubSessionInfo = {
  auth: {
    mode: 'dev' | 'workos' | 'oidc';
    session: 'cookie' | 'bearer' | 'none';
  };
  session: {
    sessionId: string;
    authMode: 'dev' | 'workos' | 'oidc';
    principal: Principal;
    idpSubject: string | null;
    expiresAt: number | null;
  } | null;
};

const API_BASE = '/api';

/** Optional override for hosts/tests — defaults to session store principal. */
let principalProvider: (() => Principal) | null = null;

export function setPrincipalProvider(provider: (() => Principal) | null) {
  principalProvider = provider;
}

function currentPrincipal(): Principal {
  if (principalProvider) return principalProvider();
  return LOCAL_PRINCIPAL;
}

export async function apiRequest(method: string, path: string, body?: unknown) {
  const headers: Record<string, string> = { accept: 'application/json' };
  const init: RequestInit = { method, headers };
  if (method !== 'GET' && method !== 'HEAD') {
    headers['x-sorrel-acting-principal'] = JSON.stringify(currentPrincipal());
  }
  if (body !== undefined) {
    headers['content-type'] = 'application/json';
    init.body = JSON.stringify(body);
  }
  const response = await fetch(`${API_BASE}${path}`, init);
  const text = await response.text();
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = { raw: text };
  }
  if (!response.ok) {
    const message =
      (payload as { error?: { message?: string } })?.error?.message ??
      `request failed (${response.status})`;
    throw new Error(message);
  }
  return payload;
}

export function apiGet(path: string) {
  return apiRequest('GET', path);
}

export function apiPost(path: string, body: unknown) {
  return apiRequest('POST', path, body);
}

export function apiPatch(path: string, body: unknown) {
  return apiRequest('PATCH', path, body);
}

/** Hub list responses use `{ data: [...] }`. */
export function unwrapList(payload: unknown): unknown[] {
  if (Array.isArray(payload)) return payload;
  if (!payload || typeof payload !== 'object') return [];
  const obj = payload as Record<string, unknown>;
  if (Array.isArray(obj.data)) return obj.data;
  if (Array.isArray(obj.items)) return obj.items;
  if (Array.isArray(obj.projects)) return obj.projects;
  if (Array.isArray(obj.repos)) return obj.repos;
  if (Array.isArray(obj.refs)) return obj.refs;
  return [];
}

export function shortId(value: unknown, length = 12): string {
  if (typeof value !== 'string' || value.length === 0) return '—';
  return value.length > length ? value.slice(0, length) : value;
}

export async function fetchCapabilities(): Promise<HubCapabilities | null> {
  try {
    const payload = (await apiGet('/capabilities')) as { data?: HubCapabilities } & HubCapabilities;
    return payload.data ?? payload;
  } catch {
    return null;
  }
}

export async function fetchSession(): Promise<HubSessionInfo | null> {
  try {
    const payload = (await apiGet('/session')) as { data?: HubSessionInfo };
    return payload.data ?? null;
  } catch {
    return null;
  }
}
