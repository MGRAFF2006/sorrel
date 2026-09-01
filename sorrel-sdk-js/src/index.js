/**
 * Minimal Hub HTTP client. Calls a live sorrel-hub — no mocks.
 */

export class HubClient {
  /**
   * @param {{
   *   baseUrl: string,
   *   principal?: { type: string, id: string },
   *   accessToken?: string,
   *   fetch?: typeof globalThis.fetch,
   * }} options
   */
  constructor(options) {
    if (!options?.baseUrl) {
      throw new Error('HubClient requires baseUrl');
    }
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.principal = options.principal ?? { type: 'user', id: 'local' };
    this.accessToken = options.accessToken;
    this.fetch = options.fetch ?? globalThis.fetch;
    if (typeof this.fetch !== 'function') {
      throw new Error('HubClient requires fetch');
    }
  }

  /**
   * @param {string} path
   * @param {RequestInit} [init]
   */
  async request(path, init = {}) {
    const headers = new Headers(init.headers);
    if (!headers.has('accept')) {
      headers.set('accept', 'application/json');
    }
    if (this.accessToken && !headers.has('authorization')) {
      headers.set('authorization', `Bearer ${this.accessToken}`);
    }
    if (init.body && !headers.has('content-type')) {
      headers.set('content-type', 'application/json');
    }
    if (init.method && init.method !== 'GET' && !headers.has('x-sorrel-acting-principal')) {
      headers.set('x-sorrel-acting-principal', JSON.stringify(this.principal));
    }
    const response = await this.fetch(`${this.baseUrl}${path}`, { ...init, headers });
    const text = await response.text();
    let body = null;
    if (text) {
      try {
        body = JSON.parse(text);
      } catch {
        body = text;
      }
    }
    if (!response.ok) {
      const error = new Error(
        `Hub ${init.method ?? 'GET'} ${path} failed: ${response.status}`,
      );
      error.status = response.status;
      error.body = body;
      throw error;
    }
    return body;
  }

  health() {
    return this.request('/healthz');
  }

  capabilities() {
    return this.request('/capabilities');
  }

  session() {
    return this.request('/session', {
      headers: {
        'x-sorrel-acting-principal': JSON.stringify(this.principal),
      },
    });
  }

  listProjects(query = {}) {
    return this.request(withQuery('/projects', query, ['organizationId']));
  }

  getProject(id) {
    return this.request(`/projects/${encodeURIComponent(id)}`);
  }

  createProject(payload) {
    return this.request('/projects', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  listSyncRepos() {
    return this.request('/admin/sync-repos');
  }

  listRefs(repoId) {
    return this.request(`/${encodeURIComponent(repoId)}/refs`);
  }

  listAdminCollection(name) {
    return this.request(`/admin/${encodeURIComponent(name)}`);
  }

  listRepositories(query = {}) {
    return this.request(
      withQuery('/admin/repositories', query, ['organizationId', 'projectId']),
    );
  }

  listProposals(query = {}) {
    return this.request(
      withQuery('/admin/proposals', query, [
        'projectId',
        'repositoryId',
        'syncRepoId',
        'status',
        'sourceLane',
      ]),
    );
  }

  getProposal(id, { includeComments = false } = {}) {
    const query = includeComments ? '?include=comments' : '';
    return this.request(`/admin/proposals/${encodeURIComponent(id)}${query}`);
  }

  createProposal(payload) {
    return this.request('/admin/proposals', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  updateProposal(id, payload) {
    return this.request(`/admin/proposals/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  createReviewComment(payload) {
    return this.request('/admin/review-comments', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  updateReviewComment(id, payload) {
    return this.request(`/admin/review-comments/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  laneSubmit(payload) {
    return this.request('/collaboration/lane-submit', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  proposalSummary(query = {}) {
    return this.request(
      withQuery('/collaboration/proposal-summary', query, ['projectId', 'syncRepoId']),
    );
  }
}

function withQuery(path, query, allowedKeys) {
  const params = new URLSearchParams();
  for (const key of allowedKeys) {
    const value = query[key];
    if (value !== undefined && value !== null && value !== '') {
      params.set(key, String(value));
    }
  }
  const encoded = params.toString();
  return `${path}${encoded ? `?${encoded}` : ''}`;
}
