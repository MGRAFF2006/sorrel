import { useParams } from '@solidjs/router';
import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, shortId, unwrapList } from '../api.ts';
import { EmptyState, ErrorText, Loading, PageHeader } from '../components/ui.tsx';

type SyncRepo = { id?: string; refCount?: number };
type SyncRef = { name?: string; snapshot?: string };
type Proposal = { syncRepoId?: string; projectId?: string };
type Repository = { id?: string; name?: string; projectId?: string; provider?: string };

export function SyncView() {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);

  const [selectedRepoId, setSelectedRepoId] = createSignal<string | null>(null);
  const [reloadToken, setReloadToken] = createSignal(0);
  const [query, setQuery] = createSignal('');

  const [allSyncRepos] = createResource(
    () => reloadToken(),
    async () => {
      const data = await apiGet('/admin/sync-repos');
      return unwrapList(data) as SyncRepo[];
    },
  );

  const [proposals] = createResource(
    () => ({ projectId: projectId(), token: reloadToken() }),
    async ({ projectId: id }) => {
      const data = await apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`);
      return unwrapList(data) as Proposal[];
    },
  );

  const [repositories] = createResource(
    () => ({ projectId: projectId(), token: reloadToken() }),
    async ({ projectId: id }) => {
      const data = await apiGet(`/admin/repositories?projectId=${encodeURIComponent(id)}`);
      return unwrapList(data) as Repository[];
    },
  );

  /** Sync repos referenced by this project's proposals, else empty. */
  const projectSyncRepos = createMemo(() => {
    const linked = new Set(
      (proposals() ?? [])
        .map((p) => p.syncRepoId)
        .filter((id): id is string => typeof id === 'string' && id.length > 0),
    );
    const all = allSyncRepos() ?? [];
    if (linked.size === 0) return [] as SyncRepo[];
    return all.filter((r) => r.id && linked.has(r.id));
  });

  const filteredRepos = createMemo(() => {
    const q = query().trim().toLowerCase();
    const list = projectSyncRepos();
    if (!q) return list;
    return list.filter((r) => (r.id ?? '').toLowerCase().includes(q));
  });

  const [refs] = createResource(
    () => selectedRepoId(),
    async (repoId) => {
      if (!repoId) return [] as SyncRef[];
      const data = await apiGet(`/${encodeURIComponent(repoId)}/refs`);
      return unwrapList(data) as SyncRef[];
    },
  );

  const loading = () => allSyncRepos.loading || proposals.loading || repositories.loading;
  const error = () => allSyncRepos.error || proposals.error || repositories.error;

  return (
    <div class="page view-enter">
      <PageHeader
        title="Repositories"
        lede="Local workspaces and content-addressed refs connected to this project."
        actions={
          <button
            type="button"
            class="ghost"
            onClick={() => {
              setSelectedRepoId(null);
              setReloadToken((n) => n + 1);
            }}
          >
            Refresh
          </button>
        }
      />

      <Show when={(repositories() ?? []).length > 0}>
        <div class="surface" style={{ padding: '0.85rem 1rem', 'margin-bottom': '0.75rem' }}>
          <span class="eyebrow">Project sources</span>
          <h2 style={{ margin: '0.45rem 0 0.5rem', 'font-size': '1rem' }}>Connected repositories</h2>
          <ul class="plain-list">
            <For each={repositories()}>
              {(repo) => (
                <li>
                  <span class="mono">{repo.id}</span>
                  <span class="muted">
                    {' '}
                    · {repo.provider}/{repo.name}
                  </span>
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>

      <div class="toolbar">
        <input
          type="search"
          placeholder="Find a repository…"
          autocomplete="off"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
        />
      </div>

      <div class="split sync-split has-detail">
        <div class="surface surface-flush" id="sync-repos" aria-live="polite">
          <Show when={!loading()} fallback={<Loading text="Loading sync data…" />}>
            <Show
              when={!error()}
              fallback={
                <ErrorText
                  text={`Could not load sync data: ${error() instanceof Error ? error()!.message : String(error())}`}
                />
              }
            >
              <Show
                when={filteredRepos().length > 0}
                fallback={
                  <EmptyState
                    title={
                      projectSyncRepos().length === 0
                        ? 'No repositories connected yet'
                        : 'No matches'
                    }
                    body={
                      projectSyncRepos().length === 0
                        ? 'Push a local workspace or submit a lane to connect its repository.'
                        : 'Try a different filter.'
                    }
                  />
                }
              >
                <table class="data-table">
                  <thead>
                    <tr>
                      <th>Repository</th>
                      <th>Refs</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={filteredRepos()}>
                      {(repo) => (
                        <tr
                          class={repo.id && repo.id === selectedRepoId() ? 'selected' : undefined}
                          data-repo-id={repo.id}
                          tabIndex={0}
                          onClick={() => {
                            if (repo.id) setSelectedRepoId(repo.id);
                          }}
                          onKeyDown={(e) => {
                            if ((e.key === 'Enter' || e.key === ' ') && repo.id) {
                              e.preventDefault();
                              setSelectedRepoId(repo.id);
                            }
                          }}
                        >
                          <td class="mono">{repo.id ?? '—'}</td>
                          <td>{String(repo.refCount ?? 0)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>
            </Show>
          </Show>
        </div>

        <aside class="detail-panel sync-refs" aria-live="polite">
          <Show
            when={selectedRepoId()}
            fallback={
              <EmptyState
                title="Select a repository"
                body="Refs and snapshot tips appear in this pane."
              />
            }
          >
            {(repoId) => (
              <>
                <div class="detail-head">
                  <div>
                    <h2>Refs</h2>
                    <p class="row-sub mono">{repoId()}</p>
                  </div>
                  <button type="button" class="ghost" onClick={() => setSelectedRepoId(null)}>
                    Clear
                  </button>
                </div>
                <Show when={!refs.loading} fallback={<Loading text="Loading refs…" />}>
                  <Show
                    when={!refs.error}
                    fallback={
                      <ErrorText
                        text={`Could not load refs: ${refs.error instanceof Error ? refs.error.message : String(refs.error)}`}
                      />
                    }
                  >
                    <Show
                      when={(refs() ?? []).length > 0}
                      fallback={
                        <EmptyState title="No refs" body="This repository has no named refs." />
                      }
                    >
                      <table class="data-table">
                        <thead>
                          <tr>
                            <th>Name</th>
                            <th>Snapshot</th>
                          </tr>
                        </thead>
                        <tbody>
                          <For each={refs()}>
                            {(ref) => {
                              const snapshot = typeof ref.snapshot === 'string' ? ref.snapshot : '';
                              return (
                                <tr style={{ cursor: 'default' }}>
                                  <td class="mono">{ref.name ?? '—'}</td>
                                  <td class="mono" title={snapshot || undefined}>
                                    {shortId(snapshot)}
                                  </td>
                                </tr>
                              );
                            }}
                          </For>
                        </tbody>
                      </table>
                    </Show>
                  </Show>
                </Show>
              </>
            )}
          </Show>
        </aside>
      </div>
    </div>
  );
}
