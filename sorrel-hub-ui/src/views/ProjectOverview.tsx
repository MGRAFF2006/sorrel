import { A, useParams } from '@solidjs/router';
import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, shortId, unwrapList } from '../api.ts';
import { Icon } from '../components/Icon.tsx';
import { MarkdownDocument } from '../components/MarkdownDocument.tsx';
import { EmptyState, ErrorText, Loading, StatusPill } from '../components/ui.tsx';
import type { Project, Proposal, Repository, SyncRef, SyncRepo, TextFileResponse, TreeResponse } from '../domain.ts';
import { metadataString, principalLabel, relativeTime } from '../domain.ts';

type ProjectBundle = {
  project: Project;
  repositories: Repository[];
  proposals: Proposal[];
  syncRepos: SyncRepo[];
};

function joinPath(base: string, name: string) {
  return base ? `${base}/${name}` : name;
}

function formatBytes(value: number | null) {
  if (value === null) return '—';
  if (value < 1024) return `${value} B`;
  return `${(value / 1024).toFixed(value < 10 * 1024 ? 1 : 0)} KB`;
}

export function ProjectOverview() {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);
  const base = () => `/projects/${encodeURIComponent(projectId())}`;
  const [path, setPath] = createSignal('');
  const [selectedRef, setSelectedRef] = createSignal('');

  const [bundle] = createResource(projectId, async (id): Promise<ProjectBundle> => {
    const [projectPayload, repositoriesPayload, proposalsPayload, syncReposPayload] = await Promise.all([
      apiGet(`/projects/${encodeURIComponent(id)}`),
      apiGet(`/admin/repositories?projectId=${encodeURIComponent(id)}`),
      apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`),
      apiGet('/admin/sync-repos'),
    ]);
    return {
      project: ((projectPayload as { data?: Project }).data ?? projectPayload) as Project,
      repositories: unwrapList(repositoriesPayload) as Repository[],
      proposals: unwrapList(proposalsPayload) as Proposal[],
      syncRepos: unwrapList(syncReposPayload) as SyncRepo[],
    };
  });

  const syncRepoId = createMemo(() => {
    const data = bundle();
    if (!data) return undefined;
    const available = new Set(data.syncRepos.map((repo) => repo.id).filter(Boolean));
    const projectMatch = data.project.repositoryIds?.find((id) => available.has(id));
    if (projectMatch) return projectMatch;
    const repositoryMatch = data.repositories.find((repo) => repo.id && available.has(repo.id));
    if (repositoryMatch?.id) return repositoryMatch.id;
    return data.proposals.map((proposal) => proposal.syncRepoId).find((id) => id && available.has(id));
  });
  const repository = createMemo(() => {
    const data = bundle();
    if (!data) return undefined;
    return data.repositories.find((repo) => repo.id === syncRepoId()) ?? data.repositories[0];
  });

  const [refs] = createResource(syncRepoId, async (repoId) => {
    if (!repoId) return [] as SyncRef[];
    return unwrapList(await apiGet(`/${encodeURIComponent(repoId)}/refs`)) as SyncRef[];
  });
  const activeRef = createMemo(() => {
    const requested = selectedRef();
    if (requested) return requested;
    const preferred = repository()?.defaultBranch ?? 'main';
    return refs()?.some((ref) => ref.name === preferred) ? preferred : refs()?.[0]?.name ?? preferred;
  });

  const [tree] = createResource(
    () => {
      const repoId = syncRepoId();
      const ref = activeRef();
      return repoId && ref ? { repoId, ref, path: path() } : null;
    },
    async (source) => {
      if (!source) return null;
      return (await apiGet(
        `/${encodeURIComponent(source.repoId)}/tree?ref=${encodeURIComponent(source.ref)}&path=${encodeURIComponent(source.path)}`,
      )) as TreeResponse;
    },
  );

  const readmeEntry = createMemo(() =>
    tree()?.entries.find((entry) => /^readme(?:\.[a-z0-9]+)?$/i.test(entry.name) && entry.type !== 'directory'),
  );
  const [readme] = createResource(
    () => {
      const repoId = syncRepoId();
      const entry = readmeEntry();
      return repoId && entry ? { repoId, ref: activeRef(), path: joinPath(path(), entry.name) } : null;
    },
    async (source) => {
      if (!source) return null;
      return (await apiGet(
        `/${encodeURIComponent(source.repoId)}/files?ref=${encodeURIComponent(source.ref)}&path=${encodeURIComponent(source.path)}`,
      )) as TextFileResponse;
    },
  );

  const breadcrumbs = createMemo(() => path().split('/').filter(Boolean));
  const activeProposals = createMemo(() =>
    (bundle()?.proposals ?? []).filter((item) => ['draft', 'open', 'approved', 'rejected'].includes(item.status ?? '')),
  );

  return (
    <div class="project-page code-page view-enter">
      <Show when={!bundle.loading} fallback={<Loading text="Loading repository…" />}>
        <Show
          when={!bundle.error && bundle()}
          fallback={<ErrorText text={`Could not load project repository: ${bundle.error instanceof Error ? bundle.error.message : String(bundle.error)}`} />}
        >
          {(data) => (
            <div class="code-layout">
              <div class="code-main">
                <section class="repo-browser surface">
                  <header class="repo-toolbar">
                    <label class="branch-select">
                      <Icon name="branch" />
                      <span class="sr-only">Repository ref</span>
                      <select value={activeRef()} onChange={(event) => { setSelectedRef(event.currentTarget.value); setPath(''); }}>
                        <For each={refs() ?? []}>{(ref) => <option value={ref.name}>{ref.name}</option>}</For>
                        <Show when={(refs() ?? []).length === 0}><option value={activeRef()}>{activeRef()}</option></Show>
                      </select>
                    </label>
                    <span class="repo-stat"><Icon name="branch" />{refs()?.length ?? 0} refs</span>
                    <span class="repo-stat"><Icon name="layers" />{activeProposals().length} active</span>
                    <A href={`${base()}/sync`} class="button secondary compact">Repository details</A>
                  </header>

                  <Show
                    when={syncRepoId()}
                    fallback={
                      <EmptyState
                        title="No synchronized repository yet"
                        body="Connect a local workspace or submit a lane. The Code view will then read the repository's real Sorrel tree and README."
                        action={<A href={`${base()}/sync`} class="button primary">Connect repository</A>}
                      />
                    }
                  >
                    <Show when={!tree.loading} fallback={<Loading text="Reading tree…" />}>
                      <Show
                        when={!tree.error && tree()}
                        fallback={<ErrorText text={`Could not browse ${activeRef()}: ${tree.error instanceof Error ? tree.error.message : String(tree.error)}`} />}
                      >
                        {(currentTree) => (
                          <>
                            <div class="snapshot-line">
                              <span class="avatar tiny">{principalLabel(currentTree().snapshot.author).slice(0, 2).toUpperCase()}</span>
                              <strong>{currentTree().snapshot.message ?? 'Snapshot update'}</strong>
                              <span>{principalLabel(currentTree().snapshot.author)}</span>
                              <time>{relativeTime(currentTree().snapshot.createdAt)}</time>
                              <code>{shortId(currentTree().snapshot.id, 8)}</code>
                            </div>
                            <nav class="breadcrumbs" aria-label="Repository path">
                              <button type="button" onClick={() => setPath('')}>{repository()?.name ?? data().project.name ?? 'repository'}</button>
                              <For each={breadcrumbs()}>{(segment, index) => (
                                <><span>/</span><button type="button" onClick={() => setPath(breadcrumbs().slice(0, index() + 1).join('/'))}>{segment}</button></>
                              )}</For>
                            </nav>
                            <div class="tree-list">
                              <Show when={path()}>
                                <button class="tree-row" type="button" onClick={() => setPath(breadcrumbs().slice(0, -1).join('/'))}>
                                  <Icon name="folder" /><strong>..</strong><span>Parent directory</span><span />
                                </button>
                              </Show>
                              <For each={currentTree().entries}>{(entry) => (
                                <button
                                  class={`tree-row ${entry.type === 'directory' ? 'directory' : 'file'}`}
                                  type="button"
                                  disabled={entry.type !== 'directory'}
                                  onClick={() => entry.type === 'directory' && setPath(joinPath(path(), entry.name))}
                                >
                                  <Icon name={entry.type === 'directory' ? 'folder' : 'code'} />
                                  <strong>{entry.name}</strong>
                                  <span>{entry.type === 'directory' ? 'Directory' : entry.mode ?? 'File'}</span>
                                  <span>{entry.type === 'directory' ? '' : formatBytes(entry.size)}</span>
                                </button>
                              )}</For>
                              <Show when={currentTree().entries.length === 0}><EmptyState title="This directory is empty" /></Show>
                            </div>
                          </>
                        )}
                      </Show>
                    </Show>
                  </Show>
                </section>

                <section class="readme-card surface">
                  <header><Icon name="book" /><strong>{readmeEntry()?.name ?? 'README.md'}</strong><span>{path() || 'Project root'}</span></header>
                  <Show when={!readme.loading} fallback={<Loading text="Reading README…" />}>
                    <Show
                      when={readme()?.content ?? metadataString(data().project.metadata, 'readme')}
                      fallback={<EmptyState title="No README in this directory" body="Add a README to the repository to give this project a narrative front page." />}
                    >
                      {(source) => <MarkdownDocument source={source()} />}
                    </Show>
                  </Show>
                </section>
              </div>

              <aside class="code-sidebar">
                <section>
                  <p class="section-label">About</p>
                  <p class="about-copy">{data().project.description ?? 'No project description yet.'}</p>
                  <dl class="about-list">
                    <div><dt><Icon name="repo" />Repositories</dt><dd>{Math.max(data().repositories.length, data().project.repositoryIds?.length ?? 0)}</dd></div>
                    <div><dt><Icon name="branch" />Open reviews</dt><dd>{activeProposals().length}</dd></div>
                    <div><dt><Icon name="users" />Collaborators</dt><dd>{data().project.principalRefs?.length ?? 0}</dd></div>
                  </dl>
                </section>
                <section>
                  <div class="sidebar-heading"><p class="section-label">Current work</p><A href={`${base()}/work`}>View board</A></div>
                  <Show when={activeProposals().length > 0} fallback={<p class="muted small">No active proposals.</p>}>
                    <div class="compact-list">
                      <For each={activeProposals().slice(0, 5)}>{(proposal) => (
                        <A href={`${base()}/reviews?proposal=${encodeURIComponent(proposal.id ?? '')}`}>
                          <span>{proposal.title ?? proposal.id}</span><StatusPill value={proposal.status} />
                        </A>
                      )}</For>
                    </div>
                  </Show>
                </section>
                <section>
                  <p class="section-label">Repository</p>
                  <p class="mono small">{syncRepoId() ?? 'Not synchronized'}</p>
                  <Show when={repository()?.provider}><p class="muted small">{repository()?.provider} · {repository()?.owner}/{repository()?.name}</p></Show>
                </section>
              </aside>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}
