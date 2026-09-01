import { A, useParams } from '@solidjs/router';
import { createResource, For, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import {
  EmptyState,
  ErrorText,
  Loading,
  PageHeader,
  RefList,
  StatusPill,
} from '../components/ui.tsx';

type Project = {
  id?: string;
  name?: string;
  organizationId?: string;
  description?: string;
  status?: string;
  slug?: string;
  repositoryIds?: string[];
  policyRefs?: unknown[];
  grantRefs?: unknown[];
};

type Proposal = {
  id?: string;
  title?: string;
  status?: string;
  projectId?: string;
};

export function ProjectOverview() {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);
  const base = () => `/projects/${encodeURIComponent(projectId())}`;

  const [project] = createResource(projectId, async (id) => {
    const payload = (await apiGet(`/projects/${encodeURIComponent(id)}`)) as { data?: Project };
    return (payload.data ?? payload) as Project;
  });

  const [proposals] = createResource(projectId, async (id) => {
    const data = await apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`);
    return unwrapList(data) as Proposal[];
  });

  const openProposals = () =>
    (proposals() ?? []).filter((p) => p.status === 'open' || p.status === 'draft');

  return (
    <div class="page view-enter">
      <Show when={!project.loading} fallback={<Loading text="Loading overview…" />}>
        <Show
          when={!project.error && project()}
          fallback={
            <ErrorText
              text={`Could not load project: ${project.error instanceof Error ? project.error.message : String(project.error)}`}
            />
          }
        >
          {(p) => (
            <>
              <PageHeader
                title={p().name ?? p().id ?? 'Project'}
                lede={
                  p().description ??
                  'Review changes and keep local workspaces moving together.'
                }
                actions={<StatusPill value={p().status} />}
              />

              <div class="overview-grid">
                <A href={`${base()}/reviews`} class="metric-card metric-card-primary">
                  <span class="metric-label">Open reviews</span>
                  <strong>{proposals.loading ? '—' : openProposals().length}</strong>
                  <span class="metric-link">Review changes <span aria-hidden="true">→</span></span>
                </A>
                <A href={`${base()}/sync`} class="metric-card">
                  <span class="metric-label">Repositories</span>
                  <strong>{p().repositoryIds?.length ?? 0}</strong>
                  <span class="metric-link">View sync state <span aria-hidden="true">→</span></span>
                </A>
                <div class="metric-card">
                  <span class="metric-label">Organization</span>
                  <strong class="metric-name">{p().organizationId ?? 'Personal'}</strong>
                  <span class="metric-link muted">Project ownership</span>
                </div>
              </div>

              <div class="overview-columns">
              <section class="surface activity-surface">
                <div class="detail-head">
                  <div>
                    <span class="eyebrow">Project activity</span>
                    <h2>Recent reviews</h2>
                  </div>
                  <A href={`${base()}/reviews`} class="ghost-link">
                    View all
                  </A>
                </div>
                <Show when={!proposals.loading} fallback={<Loading text="Loading proposals…" />}>
                  <Show
                    when={(proposals() ?? []).length > 0}
                    fallback={
                      <EmptyState
                        title="No reviews yet"
                        body="Submit a lane from the CLI or open a review here."
                        action={
                          <A href={`${base()}/reviews`} class="button-link">
                            Open Reviews
                          </A>
                        }
                      />
                    }
                  >
                    <div class="list-stack">
                      <For each={(proposals() ?? []).slice(0, 5)}>
                        {(item) => (
                          <A
                            href={`${base()}/reviews?proposal=${encodeURIComponent(item.id ?? '')}`}
                            class="list-row list-row-link"
                          >
                            <div>
                              <div class="row-title">{item.title ?? item.id ?? 'Proposal'}</div>
                              <p class="row-sub mono">{item.id}</p>
                            </div>
                            <StatusPill value={item.status} />
                          </A>
                        )}
                      </For>
                    </div>
                  </Show>
                </Show>
              </section>

              <aside class="surface connect-card">
                <span class="eyebrow">Connect a workspace</span>
                <h2>Work locally. Review here.</h2>
                <p class="muted">Add this Hub as a remote, then push a lane when it is ready to share.</p>
                <div class="command-snippet">
                  <code>sorrel remote add origin &lt;hub-url&gt;</code>
                  <code>sorrel push origin</code>
                </div>
                <A href={`${base()}/sync`} class="ghost-link">Repository details →</A>
              </aside>
              </div>

              <Show when={(p().policyRefs?.length ?? 0) > 0 || (p().grantRefs?.length ?? 0) > 0}>
                <section class="surface" style={{ 'margin-top': '1rem', padding: '1rem' }}>
                  <RefList label="policies" refs={p().policyRefs} />
                  <RefList label="grants" refs={p().grantRefs} />
                </section>
              </Show>
            </>
          )}
        </Show>
      </Show>
    </div>
  );
}
