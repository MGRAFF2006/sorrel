// Sorrel Hub web interface — thin companion over the sorrel-hub API.
// Hub owns collaboration state; this UI only GET/POST/PATCH via /api.

const API_BASE = '/api';
export const LOCAL_PRINCIPAL = { type: 'user', id: 'local' };

let selectedProposalId = null;
let adminCache = { proposals: [], comments: [], collection: 'proposals' };

export async function apiRequest(method, path, body) {
  const headers = { accept: 'application/json' };
  const init = { method, headers };
  if (method !== 'GET') {
    headers['x-sorrel-acting-principal'] = JSON.stringify(LOCAL_PRINCIPAL);
  }
  if (body !== undefined) {
    headers['content-type'] = 'application/json';
    init.body = JSON.stringify(body);
  }
  const response = await fetch(`${API_BASE}${path}`, init);
  const text = await response.text();
  let payload;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = { raw: text };
  }
  if (!response.ok) {
    const message = payload?.error?.message ?? `request failed (${response.status})`;
    throw new Error(message);
  }
  return payload;
}

async function apiGet(path) {
  return apiRequest('GET', path);
}

async function apiPost(path, body) {
  return apiRequest('POST', path, body);
}

async function apiPatch(path, body) {
  return apiRequest('PATCH', path, body);
}

/** Hub list responses use `{ data: [...] }`. */
function unwrapList(payload) {
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload?.data)) return payload.data;
  if (Array.isArray(payload?.items)) return payload.items;
  if (Array.isArray(payload?.projects)) return payload.projects;
  if (Array.isArray(payload?.repos)) return payload.repos;
  if (Array.isArray(payload?.refs)) return payload.refs;
  return [];
}

function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null) continue;
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (key === 'hidden') node.hidden = Boolean(value);
    else if (key.startsWith('on') && typeof value === 'function') {
      node.addEventListener(key.slice(2).toLowerCase(), value);
    } else node.setAttribute(key, value);
  }
  for (const child of [].concat(children)) {
    if (child) node.append(child);
  }
  return node;
}

function shortId(value, length = 12) {
  if (typeof value !== 'string' || value.length === 0) return '—';
  return value.length > length ? value.slice(0, length) : value;
}

function statusPill(value) {
  if (!value) return null;
  const cls = `pill pill-${String(value).toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
  return el('span', { class: cls, text: value });
}

function renderRefList(label, refs) {
  if (!Array.isArray(refs) || refs.length === 0) return null;
  const items = refs.map((ref) =>
    el('li', { text: typeof ref === 'string' ? ref : `${ref.kind ?? '?'}:${ref.id ?? '?'}` }),
  );
  return el('div', { class: 'refs' }, [
    el('span', { class: 'refs-label', text: label }),
    el('ul', {}, items),
  ]);
}

function projectCard(project) {
  return el('article', { class: 'card' }, [
    el('h3', { text: project.name ?? project.id ?? 'Untitled project' }),
    project.id ? el('p', { class: 'muted mono', text: project.id }) : null,
    project.organizationId
      ? el('p', { class: 'muted', text: `org: ${project.organizationId}` })
      : null,
    project.description ? el('p', { text: project.description }) : null,
    statusPill(project.status),
    renderRefList('policies', project.policyRefs),
    renderRefList('grants', project.grantRefs),
  ]);
}

async function loadProjects() {
  const container = document.getElementById('projects');
  const org = document.getElementById('org-filter').value.trim();
  container.replaceChildren(el('p', { class: 'muted', text: 'Loading projects…' }));
  try {
    const query = org ? `?organizationId=${encodeURIComponent(org)}` : '';
    const data = await apiGet(`/projects${query}`);
    const projects = unwrapList(data);
    if (projects.length === 0) {
      container.replaceChildren(el('p', { class: 'muted', text: 'No projects yet.' }));
      return;
    }
    container.replaceChildren(...projects.map(projectCard));
  } catch (error) {
    container.replaceChildren(
      el('p', { class: 'error', text: `Could not load projects: ${error.message}` }),
    );
  }
}

function proposalActions(item, { compact = false } = {}) {
  if (!item?.id || !item.status) return null;
  const actions = [];
  const transitions = {
    draft: ['open', 'closed'],
    open: ['approved', 'rejected', 'merged', 'closed'],
    approved: ['merged', 'closed'],
    rejected: ['open', 'closed'],
  };
  for (const status of transitions[item.status] ?? []) {
    const danger = status === 'rejected' || status === 'closed';
    actions.push(
      el('button', {
        type: 'button',
        class: danger ? 'btn-small danger' : 'btn-small',
        text: status,
        onClick: async (event) => {
          event.stopPropagation();
          try {
            await apiPatch(`/admin/proposals/${encodeURIComponent(item.id)}`, { status });
            await loadAdmin('proposals');
            if (selectedProposalId === item.id) {
              await showProposalDetail(item.id);
            }
          } catch (error) {
            alert(`Status update failed: ${error.message}`);
          }
        },
      }),
    );
  }
  if (actions.length === 0) return null;
  return el('div', { class: compact ? 'card-actions' : 'card-actions' }, actions);
}

function commentActions(item) {
  if (!item?.id || item.state !== 'open') return null;
  return el('div', { class: 'card-actions' }, [
    el('button', {
      type: 'button',
      class: 'btn-small',
      text: 'resolve',
      onClick: async (event) => {
        event.stopPropagation();
        try {
          await apiPatch(`/admin/review-comments/${encodeURIComponent(item.id)}`, {
            state: 'resolved',
          });
          await loadAdmin(adminCache.collection);
          if (selectedProposalId) await showProposalDetail(selectedProposalId);
        } catch (error) {
          alert(`Resolve failed: ${error.message}`);
        }
      },
    }),
  ]);
}

function workflowActions(item) {
  if (!item?.id) return null;
  const next = {
    queued: ['running', 'failed'],
    running: ['succeeded', 'failed'],
  };
  const actions = (next[item.status] ?? []).map((status) =>
    el('button', {
      type: 'button',
      class: status === 'failed' ? 'btn-small danger' : 'btn-small',
      text: status,
      onClick: async (event) => {
        event.stopPropagation();
        try {
          await apiPatch(`/admin/workflow-runs/${encodeURIComponent(item.id)}`, { status });
          await loadAdmin('workflow-runs');
        } catch (error) {
          alert(`Workflow update failed: ${error.message}`);
        }
      },
    }),
  );
  if (actions.length === 0) return null;
  return el('div', { class: 'card-actions' }, actions);
}

function adminCard(item, collection) {
  const isProposal = collection === 'proposals';
  const selected = isProposal && item.id && item.id === selectedProposalId;
  const children = [
    el('h3', { text: item.name ?? item.title ?? item.id ?? 'Item' }),
    item.id ? el('p', { class: 'muted mono', text: item.id }) : null,
    statusPill(item.status),
    statusPill(item.state),
    item.projectId ? el('p', { class: 'muted', text: `project: ${item.projectId}` }) : null,
    item.proposalId ? el('p', { class: 'muted', text: `proposal: ${item.proposalId}` }) : null,
    item.sourceLane ? el('p', { class: 'muted', text: `lane: ${item.sourceLane}` }) : null,
    item.syncRepoId ? el('p', { class: 'muted mono', text: `sync: ${item.syncRepoId}` }) : null,
    item.authorRef ? el('p', { class: 'muted', text: `author: ${item.authorRef}` }) : null,
    item.body ? el('p', { text: item.body }) : null,
    item.path ? el('p', { class: 'muted mono', text: item.path }) : null,
    renderRefList('policies', item.policyRefs),
    renderRefList('grants', item.grantRefs),
  ];
  if (collection === 'proposals') children.push(proposalActions(item, { compact: true }));
  if (collection === 'review-comments') children.push(commentActions(item));
  if (collection === 'workflow-runs') children.push(workflowActions(item));

  return el(
    'article',
    {
      class: `card${isProposal ? ' selectable' : ''}${selected ? ' selected' : ''}`,
      onClick: isProposal
        ? () => {
            selectedProposalId = item.id;
            showProposalDetail(item.id);
            loadAdmin('proposals');
          }
        : undefined,
    },
    children,
  );
}

async function showProposalDetail(proposalId) {
  const detail = document.getElementById('admin-detail');
  const layout = document.getElementById('admin-layout');
  detail.hidden = false;
  layout.classList.add('has-detail');
  detail.replaceChildren(el('p', { class: 'muted', text: 'Loading proposal…' }));

  try {
    let proposal = adminCache.proposals.find((p) => p.id === proposalId);
    if (!proposal) {
      const payload = await apiGet(`/admin/proposals/${encodeURIComponent(proposalId)}`);
      proposal = payload?.data ?? payload;
    }

    let comments = adminCache.comments.filter((c) => c.proposalId === proposalId);
    if (comments.length === 0) {
      const payload = await apiGet('/admin/review-comments');
      adminCache.comments = unwrapList(payload);
      comments = adminCache.comments.filter((c) => c.proposalId === proposalId);
    }

    const commentForm = el('form', { class: 'form-card', id: 'detail-comment-form' }, [
      el('h2', { text: 'Reply on this proposal' }),
      el('label', {}, [
        'Body',
        el('textarea', { name: 'body', rows: '3', required: true, placeholder: 'Review note' }),
      ]),
      el('label', {}, [
        'Path (optional)',
        el('input', { name: 'path', type: 'text', placeholder: 'src/main.rs', autocomplete: 'off' }),
      ]),
      el('button', { type: 'submit', text: 'Post comment' }),
      el('p', { class: 'muted', id: 'detail-comment-status', 'aria-live': 'polite' }),
    ]);
    commentForm.addEventListener('submit', async (event) => {
      event.preventDefault();
      const status = commentForm.querySelector('#detail-comment-status');
      const data = Object.fromEntries(new FormData(commentForm).entries());
      status.className = 'muted';
      status.textContent = 'Posting…';
      try {
        await apiPost('/admin/review-comments', {
          proposalId,
          body: data.body,
          path: data.path || undefined,
          authorPrincipal: LOCAL_PRINCIPAL,
        });
        status.textContent = 'Posted';
        commentForm.reset();
        const payload = await apiGet('/admin/review-comments');
        adminCache.comments = unwrapList(payload);
        await showProposalDetail(proposalId);
        if (adminCache.collection === 'review-comments') await loadAdmin('review-comments');
      } catch (error) {
        status.textContent = error.message;
        status.className = 'error';
      }
    });

    detail.replaceChildren(
      el('div', { class: 'panel-head' }, [
        el('div', {}, [
          el('h2', { text: proposal.title ?? proposal.id ?? 'Proposal' }),
          statusPill(proposal.status),
        ]),
        el('button', {
          type: 'button',
          class: 'ghost',
          text: 'Close',
          onClick: () => {
            selectedProposalId = null;
            detail.hidden = true;
            layout.classList.remove('has-detail');
            loadAdmin('proposals');
          },
        }),
      ]),
      el('dl', { class: 'detail-meta' }, [
        el('dt', { text: 'Id' }),
        el('dd', { class: 'mono', text: proposal.id ?? '—' }),
        el('dt', { text: 'Project' }),
        el('dd', { class: 'mono', text: proposal.projectId ?? '—' }),
        el('dt', { text: 'Source lane' }),
        el('dd', { text: proposal.sourceLane ?? '—' }),
        el('dt', { text: 'Sync repo' }),
        el('dd', { class: 'mono', text: proposal.syncRepoId ?? '—' }),
      ]),
      proposal.description ? el('p', { text: proposal.description }) : null,
      proposalActions(proposal),
      el('h2', { text: `Comments (${comments.length})` }),
      comments.length === 0
        ? el('p', { class: 'muted', text: 'No review comments yet.' })
        : el(
            'div',
            { class: 'thread' },
            comments.map((c) =>
              el('div', { class: `thread-item${c.state === 'resolved' ? ' resolved' : ''}` }, [
                el('p', { text: c.body ?? '' }),
                c.path ? el('p', { class: 'muted mono', text: c.path }) : null,
                statusPill(c.state),
                commentActions(c),
              ]),
            ),
          ),
      commentForm,
    );
  } catch (error) {
    detail.replaceChildren(
      el('p', { class: 'error', text: `Could not load proposal: ${error.message}` }),
    );
  }
}

function showAdminForms(collection) {
  document.getElementById('proposal-form').hidden = collection !== 'proposals';
  document.getElementById('comment-form').hidden = collection !== 'review-comments';
  document.getElementById('workflow-form').hidden = collection !== 'workflow-runs';
  const detail = document.getElementById('admin-detail');
  const layout = document.getElementById('admin-layout');
  if (collection === 'proposals') {
    if (selectedProposalId) {
      detail.hidden = false;
      layout.classList.add('has-detail');
    }
  } else {
    detail.hidden = true;
    layout.classList.remove('has-detail');
  }
}

async function loadAdmin(collection) {
  const container = document.getElementById('admin-list');
  adminCache.collection = collection;
  showAdminForms(collection);
  container.replaceChildren(el('p', { class: 'muted', text: `Loading ${collection}…` }));
  try {
    const data = await apiGet(`/admin/${collection}`);
    const items = unwrapList(data);
    if (collection === 'proposals') adminCache.proposals = items;
    if (collection === 'review-comments') adminCache.comments = items;
    if (items.length === 0) {
      container.replaceChildren(el('p', { class: 'muted', text: `No ${collection} yet.` }));
      return;
    }
    container.replaceChildren(...items.map((item) => adminCard(item, collection)));
    if (collection === 'proposals' && selectedProposalId) {
      await showProposalDetail(selectedProposalId);
    }
  } catch (error) {
    container.replaceChildren(
      el('p', { class: 'error', text: `Could not load ${collection}: ${error.message}` }),
    );
  }
}

let selectedSyncRepoId = null;

function syncRepoRow(repo) {
  const id = repo.id ?? '';
  const row = el('tr', { 'data-repo-id': id }, [
    el('td', { class: 'mono', text: id }),
    el('td', { text: String(repo.refCount ?? 0) }),
  ]);
  if (id && id === selectedSyncRepoId) {
    row.classList.add('selected');
  }
  return row;
}

async function loadSyncRefs(repoId) {
  const container = document.getElementById('sync-refs');
  selectedSyncRepoId = repoId;
  container.replaceChildren(
    el('h2', { text: `Refs · ${repoId}` }),
    el('p', { class: 'muted', text: 'Loading refs…' }),
  );

  const table = document.querySelector('#sync-repos table');
  if (table) {
    for (const row of table.querySelectorAll('tbody tr')) {
      row.classList.toggle('selected', row.dataset.repoId === repoId);
    }
  }

  try {
    const data = await apiGet(`/${encodeURIComponent(repoId)}/refs`);
    const refs = unwrapList(data);
    if (refs.length === 0) {
      container.replaceChildren(
        el('h2', { text: `Refs · ${repoId}` }),
        el('p', { class: 'muted', text: 'No refs' }),
      );
      return;
    }

    const body = el(
      'tbody',
      {},
      refs.map((ref) => {
        const snapshot = typeof ref.snapshot === 'string' ? ref.snapshot : '';
        return el('tr', {}, [
          el('td', { class: 'mono', text: ref.name ?? '—' }),
          el('td', {
            class: 'mono',
            text: shortId(snapshot),
            title: snapshot || undefined,
          }),
        ]);
      }),
    );

    container.replaceChildren(
      el('h2', { text: `Refs · ${repoId}` }),
      el('table', { class: 'data-table' }, [
        el('thead', {}, [
          el('tr', {}, [el('th', { text: 'Name' }), el('th', { text: 'Snapshot' })]),
        ]),
        body,
      ]),
    );
  } catch (error) {
    container.replaceChildren(
      el('h2', { text: `Refs · ${repoId}` }),
      el('p', { class: 'error', text: `Could not load refs: ${error.message}` }),
    );
  }
}

async function loadSyncRepos() {
  const container = document.getElementById('sync-repos');
  const refsContainer = document.getElementById('sync-refs');
  container.replaceChildren(el('p', { class: 'muted', text: 'Loading synced repositories…' }));
  refsContainer.replaceChildren();
  selectedSyncRepoId = null;

  try {
    const data = await apiGet('/admin/sync-repos');
    const repos = unwrapList(data);
    if (repos.length === 0) {
      container.replaceChildren(
        el('p', { class: 'muted', text: 'No repositories have been synced yet' }),
      );
      return;
    }

    const body = el('tbody', {}, repos.map(syncRepoRow));
    body.addEventListener('click', (event) => {
      const row = event.target.closest('tr[data-repo-id]');
      if (!row?.dataset.repoId) return;
      loadSyncRefs(row.dataset.repoId);
    });

    container.replaceChildren(
      el('table', { class: 'data-table' }, [
        el('thead', {}, [
          el('tr', {}, [el('th', { text: 'Repository' }), el('th', { text: 'Refs' })]),
        ]),
        body,
      ]),
    );
  } catch (error) {
    container.replaceChildren(
      el('p', {
        class: 'error',
        text: `Could not load synced repositories: ${error.message}`,
      }),
    );
  }
}

async function refreshStatus() {
  const badge = document.getElementById('api-status');
  try {
    const health = await apiGet('/healthz');
    badge.textContent = `API: ${health.status ?? 'ok'}`;
    badge.className = 'status status-ok';
  } catch {
    badge.textContent = 'API: unreachable';
    badge.className = 'status status-down';
  }
}

function wireAdminTabs() {
  const tabs = document.getElementById('admin-tabs');
  tabs.addEventListener('click', (event) => {
    const button = event.target.closest('.tab');
    if (!button) return;
    for (const tab of tabs.querySelectorAll('.tab')) tab.classList.remove('active');
    button.classList.add('active');
    loadAdmin(button.dataset.collection);
  });
}

function showView(viewName) {
  const nav = document.getElementById('app-nav');
  for (const button of nav.querySelectorAll('.nav-item')) {
    button.classList.toggle('active', button.dataset.view === viewName);
  }
  for (const panel of document.querySelectorAll('.view')) {
    panel.hidden = panel.dataset.view !== viewName;
  }
  if (viewName === 'sync') {
    loadSyncRepos();
  }
  if (viewName === 'admin') {
    loadAdmin(adminCache.collection || 'proposals');
  }
}

function wireNav() {
  const nav = document.getElementById('app-nav');
  nav.addEventListener('click', (event) => {
    const button = event.target.closest('.nav-item');
    if (!button) return;
    showView(button.dataset.view);
  });
}

function wireForms() {
  document.getElementById('project-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.target;
    const status = document.getElementById('project-form-status');
    const data = Object.fromEntries(new FormData(form).entries());
    status.className = 'muted';
    status.textContent = 'Creating…';
    try {
      const created = await apiPost('/projects', {
        organizationId: data.organizationId,
        name: data.name,
        description: data.description || undefined,
      });
      status.textContent = `Created ${created.data.id}`;
      form.reset();
      await loadProjects();
    } catch (error) {
      status.textContent = error.message;
      status.className = 'error';
    }
  });

  document.getElementById('proposal-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.target;
    const status = document.getElementById('proposal-form-status');
    const data = Object.fromEntries(new FormData(form).entries());
    status.className = 'muted';
    status.textContent = 'Creating…';
    try {
      const created = await apiPost('/admin/proposals', {
        projectId: data.projectId,
        title: data.title,
        syncRepoId: data.syncRepoId || undefined,
        sourceLane: data.sourceLane || undefined,
        description: data.description || undefined,
        authorPrincipal: LOCAL_PRINCIPAL,
        status: 'open',
      });
      status.textContent = `Created ${created.data.id}`;
      form.reset();
      selectedProposalId = created.data.id;
      await loadAdmin('proposals');
    } catch (error) {
      status.textContent = error.message;
      status.className = 'error';
    }
  });

  document.getElementById('comment-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.target;
    const status = document.getElementById('comment-form-status');
    const data = Object.fromEntries(new FormData(form).entries());
    status.className = 'muted';
    status.textContent = 'Posting…';
    try {
      const created = await apiPost('/admin/review-comments', {
        proposalId: data.proposalId,
        body: data.body,
        path: data.path || undefined,
        authorPrincipal: LOCAL_PRINCIPAL,
      });
      status.textContent = `Created ${created.data.id}`;
      form.reset();
      await loadAdmin('review-comments');
    } catch (error) {
      status.textContent = error.message;
      status.className = 'error';
    }
  });

  document.getElementById('workflow-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.target;
    const status = document.getElementById('workflow-form-status');
    const data = Object.fromEntries(new FormData(form).entries());
    status.className = 'muted';
    status.textContent = 'Updating…';
    try {
      await apiPatch(`/admin/workflow-runs/${encodeURIComponent(data.runId)}`, {
        status: data.status,
      });
      status.textContent = `Updated ${data.runId} → ${data.status}`;
      await loadAdmin('workflow-runs');
    } catch (error) {
      status.textContent = error.message;
      status.className = 'error';
    }
  });
}

function init() {
  document.getElementById('refresh-projects').addEventListener('click', loadProjects);
  document.getElementById('refresh-sync').addEventListener('click', loadSyncRepos);
  document.getElementById('org-filter').addEventListener('keydown', (event) => {
    if (event.key === 'Enter') loadProjects();
  });
  wireNav();
  wireAdminTabs();
  wireForms();
  refreshStatus();
  loadProjects();
  loadAdmin('proposals');
}

if (typeof document !== 'undefined') {
  document.addEventListener('DOMContentLoaded', init);
}
