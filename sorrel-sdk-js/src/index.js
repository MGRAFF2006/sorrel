/**
 * Minimal Hub HTTP client. Calls a live sorrel-hub — no mocks.
 */

export class HubClient {
  /**
   * @param {{ baseUrl: string, principal?: { type: string, id: string } }} options
   */
  constructor(options) {
    if (!options?.baseUrl) {
      throw new Error('HubClient requires baseUrl');
    }
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.principal = options.principal ?? { type: 'user', id: 'local' };
  }

  /**
   * @param {string} path
   * @param {RequestInit} [init]
   */
  async request(path, init = {}) {
    const headers = {
      accept: 'application/json',
      ...(init.headers ?? {}),
    };
    if (init.body && !headers['content-type']) {
      headers['content-type'] = 'application/json';
    }
    if (init.method && init.method !== 'GET') {
      headers['x-sorrel-acting-principal'] = JSON.stringify(this.principal);
    }
    const response = await fetch(`${this.baseUrl}${path}`, { ...init, headers });
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

  listProjects() {
    return this.request('/projects');
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
    const params = new URLSearchParams();
    if (query.projectId) params.set('projectId', query.projectId);
    if (query.syncRepoId) params.set('syncRepoId', query.syncRepoId);
    const qs = params.toString();
    return this.request(`/collaboration/proposal-summary${qs ? `?${qs}` : ''}`);
  }
}
