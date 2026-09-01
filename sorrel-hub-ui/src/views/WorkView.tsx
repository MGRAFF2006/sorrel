import { A, useParams } from '@solidjs/router';
import { createMemo, createResource, For, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import { Icon } from '../components/Icon.tsx';
import { EmptyState, ErrorText, Loading, StatusPill } from '../components/ui.tsx';
import type { Proposal } from '../domain.ts';
import { principalLabel, relativeTime } from '../domain.ts';

const columns = [
  { id: 'planned', label: 'Planned', description: 'Draft lanes', statuses: ['draft'] },
  { id: 'active', label: 'Active', description: 'Open or changing', statuses: ['open', 'rejected'] },
  { id: 'review', label: 'Ready', description: 'Approved for integration', statuses: ['approved'] },
  { id: 'integrated', label: 'Integrated', description: 'Merged or closed', statuses: ['merged', 'closed'] },
] as const;

export function WorkView() {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);
  const base = () => `/projects/${encodeURIComponent(projectId())}`;

  const [proposals, { refetch }] = createResource(projectId, async (id) =>
    unwrapList(await apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`)) as Proposal[],
  );
  const work = createMemo(() => proposals() ?? []);

  return (
    <div class="work-page view-enter">
      <header class="workspace-heading">
        <div>
          <p class="section-label">Proposal-backed lanes</p>
          <h1>Work</h1>
          <p>Follow shared lanes from draft through review and integration.</p>
        </div>
        <div class="workspace-actions">
          <button type="button" class="button secondary" onClick={() => void refetch()}><Icon name="refresh" />Refresh</button>
          <A href={`${base()}/reviews`} class="button primary"><Icon name="branch" />Open review</A>
        </div>
      </header>

      <Show when={!proposals.loading} fallback={<Loading text="Loading project work…" />}>
        <Show
          when={!proposals.error}
          fallback={<ErrorText text={`Could not load work: ${proposals.error instanceof Error ? proposals.error.message : String(proposals.error)}`} />}
        >
          <Show
            when={work().length > 0}
            fallback={<EmptyState title="No shared lanes yet" body="Draft or open a review to place a lane on this board." action={<A class="button primary" href={`${base()}/reviews`}>Open the first review</A>} />}
          >
            <div class="kanban-board">
              <For each={columns}>{(column) => {
                const cards = () => work().filter((item) => column.statuses.includes((item.status ?? '') as never));
                return (
                  <section class={`kanban-column ${column.id}`}>
                    <header>
                      <div><span class="column-dot" /><strong>{column.label}</strong><span class="column-count">{cards().length}</span></div>
                      <p>{column.description}</p>
                    </header>
                    <div class="kanban-stack">
                      <For each={cards()}>{(item) => (
                        <A href={`${base()}/reviews?proposal=${encodeURIComponent(item.id ?? '')}`} class="work-card">
                          <div class="work-card-top"><StatusPill value={item.status} /><span>{relativeTime(item.updatedAt ?? item.createdAt)}</span></div>
                          <h2>{item.title ?? item.id ?? 'Untitled proposal'}</h2>
                          <Show when={item.description}><p>{item.description}</p></Show>
                          <footer>
                            <span><Icon name="branch" />{item.sourceLane ?? item.sourceBranch ?? 'No source lane'}</span>
                            <span>{principalLabel(item.authorPrincipal) !== 'Unknown' ? principalLabel(item.authorPrincipal) : item.authorRef ?? ''}</span>
                          </footer>
                        </A>
                      )}</For>
                      <Show when={cards().length === 0}><p class="empty-column">Nothing here</p></Show>
                    </div>
                  </section>
                );
              }}</For>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
