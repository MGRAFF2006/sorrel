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
                  'Project home — jump into reviews or sync for this workspace.'
                }
              />

              <dl class="detail-meta surface" style={{ padding: '1rem' }}>
                <dt>Id</dt>
                <dd class="mono">{p().id ?? '—'}</dd>
                <dt>Organization</dt>
                <dd class="mono">{p().organizationId ?? '—'}</dd>
                <dt>Status</dt>
                <dd>
                  <StatusPill value={p().status} />
                </dd>
              </dl>

              <div class="feature-grid">
                <A href={`${base()}/reviews`} class="feature-tile">
                  <h2>Reviews</h2>
                  <p class="muted">
                    Proposals and comment threads for this project
                    <Show when={!proposals.loading}>
                      {' '}
                      · {openProposals().length} open
                    </Show>
                  </p>
                </A>
                <A href={`${base()}/sync`} class="feature-tile">
                  <h2>Sync</h2>
                  <p class="muted">Object-store repos and refs linked from this project</p>
                </A>
              </div>

              <section class="surface" style={{ 'margin-top': '0.25rem' }}>
                <div class="detail-head">
                  <h2 style={{ margin: 0, 'font-size': '1rem' }}>Recent proposals</h2>
                  <A href={`${base()}/reviews`} class="ghost-link">
                    View all
                  </A>
                </div>
                <Show when={!proposals.loading} fallback={<Loading text="Loading proposals…" />}>
                  <Show
                    when={(proposals() ?? []).length > 0}
                    fallback={
                      <EmptyState
                        title="No proposals yet"
                        body="Open a proposal from Reviews or via lane submit."
                        action={
                          <A href={`${base()}/reviews`} class="button-link">
                            Go to Reviews
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
