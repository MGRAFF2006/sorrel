import { useParams, useSearchParams } from '@solidjs/router';
import { createEffect, createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, apiPatch, apiPost, unwrapList } from '../api.ts';
import { getActingPrincipal } from '../session.ts';
import {
  EmptyState,
  ErrorText,
  FormStatus,
  Loading,
  PageHeader,
  RefList,
  StatusPill,
} from '../components/ui.tsx';

type AdminItem = Record<string, unknown> & {
  id?: string;
  name?: string;
  title?: string;
  status?: string;
  state?: string;
  projectId?: string;
  proposalId?: string;
  sourceLane?: string;
  syncRepoId?: string;
  authorRef?: string;
  body?: string;
  path?: string;
  description?: string;
  policyRefs?: unknown[];
  grantRefs?: unknown[];
};

const PROPOSAL_TRANSITIONS: Record<string, string[]> = {
  draft: ['open', 'closed'],
  open: ['approved', 'rejected', 'merged', 'closed'],
  approved: ['merged', 'closed'],
  rejected: ['open', 'closed'],
};

const WORKFLOW_NEXT: Record<string, string[]> = {
  queued: ['in_progress', 'failed'],
  in_progress: ['succeeded', 'failed'],
};

type Tab = 'proposals' | 'comments' | 'workflows';

export function ReviewsView() {
  const params = useParams<{ projectId: string }>();
  const [search, setSearch] = useSearchParams();
  const projectId = () => decodeURIComponent(params.projectId);

  const [tab, setTab] = createSignal<Tab>('proposals');
  const [selectedProposalId, setSelectedProposalId] = createSignal<string | null>(null);
  const [reloadToken, setReloadToken] = createSignal(0);
  const [formStatus, setFormStatus] = createSignal('');
  const [formError, setFormError] = createSignal(false);
  const [creating, setCreating] = createSignal(false);
  const [filter, setFilter] = createSignal('');

  createEffect(() => {
    const fromQuery = search.proposal;
    if (typeof fromQuery === 'string' && fromQuery.length > 0) {
      setSelectedProposalId(fromQuery);
      setTab('proposals');
    }
  });

  const [proposals] = createResource(
    () => ({ projectId: projectId(), token: reloadToken() }),
    async ({ projectId: id }) => {
      const data = await apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`);
      return unwrapList(data) as AdminItem[];
    },
  );

  const [comments] = createResource(
    () => ({ proposals: proposals(), token: reloadToken() }),
    async ({ proposals: propsList }) => {
      const data = await apiGet('/admin/review-comments');
      const all = unwrapList(data) as AdminItem[];
      const ids = new Set(
        (propsList ?? []).map((p) => p.id).filter((id): id is string => typeof id === 'string'),
      );
      if (ids.size === 0) return [] as AdminItem[];
      return all.filter((c) => typeof c.proposalId === 'string' && ids.has(c.proposalId));
    },
  );

  const [workflows] = createResource(
    () => ({ projectId: projectId(), token: reloadToken() }),
    async ({ projectId: id }) => {
      const data = await apiGet(`/admin/workflow-runs?projectId=${encodeURIComponent(id)}`);
      return unwrapList(data) as AdminItem[];
    },
  );

  const filteredProposals = createMemo(() => {
    const q = filter().trim().toLowerCase();
    const all = proposals() ?? [];
    if (!q) return all;
    return all.filter((item) => {
      const hay = [item.id, item.title, item.status, item.sourceLane, item.syncRepoId]
        .filter(Boolean)
        .join(' ')
        .toLowerCase();
      return hay.includes(q);
    });
  });

  const selectedProposal = () =>
    (proposals() ?? []).find((p) => p.id === selectedProposalId()) ?? null;

  const proposalComments = () =>
    (comments() ?? []).filter((c) => c.proposalId === selectedProposalId());

  async function patchProposalStatus(id: string, status: string) {
    await apiPatch(`/admin/proposals/${encodeURIComponent(id)}`, { status });
    setReloadToken((n) => n + 1);
  }

  async function resolveComment(id: string) {
    await apiPatch(`/admin/review-comments/${encodeURIComponent(id)}`, { state: 'resolved' });
    setReloadToken((n) => n + 1);
  }

  async function patchWorkflow(id: string, status: string) {
    await apiPatch(`/admin/workflow-runs/${encodeURIComponent(id)}`, { status });
    setReloadToken((n) => n + 1);
  }

  async function onOpenProposal(event: Event) {
    event.preventDefault();
    const form = event.currentTarget as HTMLFormElement;
    const data = Object.fromEntries(new FormData(form).entries());
    setFormError(false);
    setFormStatus('Creating…');
    try {
      const created = (await apiPost('/admin/proposals', {
        projectId: projectId(),
        title: data.title,
        syncRepoId: data.syncRepoId || undefined,
        sourceLane: data.sourceLane || undefined,
        description: data.description || undefined,
        authorPrincipal: getActingPrincipal(),
        status: 'open',
      })) as { data: { id: string } };
      setFormStatus(`Created ${created.data.id}`);
      form.reset();
      setCreating(false);
      setSelectedProposalId(created.data.id);
      setSearch({ proposal: created.data.id });
      setReloadToken((n) => n + 1);
    } catch (error) {
      setFormError(true);
      setFormStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function selectProposal(id: string) {
    setSelectedProposalId(id);
    setSearch({ proposal: id });
  }

  return (
    <div class="page review-page view-enter">
      <PageHeader
        title="Review workbench"
        lede="Move from proposal queue to discussion and decision without leaving the project context."
        actions={
          <>
            <button type="button" class="ghost" onClick={() => setReloadToken((n) => n + 1)}>
              Refresh
            </button>
            <Show when={tab() === 'proposals'}>
              <button
                type="button"
                onClick={() => setCreating(true)}
                aria-expanded={creating()}
              >
                Open review <span aria-hidden="true">＋</span>
              </button>
            </Show>
          </>
        }
      />

      <div class="toolbar">
        <div class="segmented" role="tablist" aria-label="Review sections">
          <button
            type="button"
            class={tab() === 'proposals' ? 'active' : undefined}
            role="tab"
            aria-selected={tab() === 'proposals'}
            onClick={() => setTab('proposals')}
          >
            Reviews
          </button>
          <button
            type="button"
            class={tab() === 'comments' ? 'active' : undefined}
            role="tab"
            aria-selected={tab() === 'comments'}
            onClick={() => setTab('comments')}
          >
            Discussion
          </button>
          <button
            type="button"
            class={tab() === 'workflows' ? 'active' : undefined}
            role="tab"
            aria-selected={tab() === 'workflows'}
            onClick={() => setTab('workflows')}
          >
            Checks
          </button>
        </div>
        <Show when={tab() === 'proposals'}>
          <input
            type="search"
            placeholder="Find a review…"
            autocomplete="off"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
          />
        </Show>
      </div>

      <Show when={tab() === 'proposals' && creating()}>
        <div class="dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setCreating(false);
        }}>
          <form class="form-card project-dialog" role="dialog" aria-modal="true" aria-labelledby="open-review-title" onSubmit={onOpenProposal}>
            <div class="dialog-heading">
              <div>
                <span class="eyebrow">02 / Share changes</span>
                <h2 id="open-review-title">Open a review</h2>
                <p class="muted">Bring a lane into the project for discussion and approval.</p>
              </div>
              <button class="dialog-close" type="button" aria-label="Close dialog" onClick={() => setCreating(false)}>×</button>
            </div>
            <label>
              <span>Title</span>
              <input name="title" required placeholder="Land feature lane" autocomplete="off" />
            </label>
            <div class="form-grid two">
              <label>
                <span>Repository <i>optional</i></span>
                <input name="syncRepoId" placeholder="repo_…" autocomplete="off" />
              </label>
              <label>
                <span>Source lane <i>optional</i></span>
                <input name="sourceLane" placeholder="lane_feature" autocomplete="off" />
              </label>
            </div>
            <label>
              <span>Description <i>optional</i></span>
              <textarea name="description" rows={4} placeholder="What changed, and what should reviewers focus on?" />
            </label>
            <div class="dialog-actions">
              <button type="button" class="ghost" onClick={() => setCreating(false)}>Cancel</button>
              <button type="submit">Open review <span aria-hidden="true">→</span></button>
            </div>
            <FormStatus message={formStatus()} error={formError()} />
          </form>
        </div>
      </Show>

      <Show when={tab() === 'proposals'}>
        <div class={`split review-workbench${selectedProposalId() ? ' has-detail' : ''}`}>
          <div class="surface surface-flush" aria-live="polite">
            <Show when={!proposals.loading} fallback={<Loading text="Loading proposals…" />}>
              <Show
                when={!proposals.error}
                fallback={
                  <ErrorText
                    text={`Could not load proposals: ${proposals.error instanceof Error ? proposals.error.message : String(proposals.error)}`}
                  />
                }
              >
                <Show
                  when={filteredProposals().length > 0}
                  fallback={
                    <EmptyState
                      title="No reviews yet"
                      body="Submit a lane from the CLI or open a review here."
                      action={
                        <button type="button" onClick={() => setCreating(true)}>
                          Open the first review
                        </button>
                      }
                    />
                  }
                >
                  <div class="list-stack" role="listbox" aria-label="Proposals">
                    <For each={filteredProposals()}>
                      {(item) => (
                        <div
                          class={`list-row${item.id === selectedProposalId() ? ' selected' : ''}`}
                          role="option"
                          aria-selected={item.id === selectedProposalId()}
                          tabIndex={0}
                          onClick={() => {
                            if (item.id) selectProposal(item.id);
                          }}
                          onKeyDown={(e) => {
                            if ((e.key === 'Enter' || e.key === ' ') && item.id) {
                              e.preventDefault();
                              selectProposal(item.id);
                            }
                          }}
                        >
                          <div>
                            <div class="row-title">{String(item.title ?? item.id ?? 'Proposal')}</div>
                            <p class="row-sub mono">{item.id ?? '—'}</p>
                            <Show when={item.sourceLane}>
                              <p class="row-sub">lane · {String(item.sourceLane)}</p>
                            </Show>
                          </div>
                          <StatusPill value={item.status} />
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>
            </Show>
          </div>

          <Show
            when={selectedProposal()}
            fallback={
              <Show when={filteredProposals().length > 0}>
                <aside class="detail-panel">
                  <EmptyState
                    title="Select a review"
                    body="Discussion, status, and source information open here."
                  />
                </aside>
              </Show>
            }
          >
            {(proposal) => (
              <aside class="detail-panel">
                <div class="detail-head">
                  <div>
                    <h2>{String(proposal().title ?? proposal().id ?? 'Proposal')}</h2>
                    <StatusPill value={proposal().status} />
                  </div>
                  <button
                    type="button"
                    class="ghost"
                    onClick={() => {
                      setSelectedProposalId(null);
                      setSearch({ proposal: undefined });
                    }}
                  >
                    Close
                  </button>
                </div>
                <dl class="detail-meta">
                  <dt>Id</dt>
                  <dd class="mono">{proposal().id ?? '—'}</dd>
                  <dt>Source lane</dt>
                  <dd>{String(proposal().sourceLane ?? '—')}</dd>
                  <dt>Sync repo</dt>
                  <dd class="mono">{String(proposal().syncRepoId ?? '—')}</dd>
                </dl>
                <Show when={proposal().description}>
                  <p>{String(proposal().description)}</p>
                </Show>
                <div class="card-actions">
                  <For each={PROPOSAL_TRANSITIONS[String(proposal().status)] ?? []}>
                    {(status) => (
                      <button
                        type="button"
                        class={
                          status === 'rejected' || status === 'closed'
                            ? 'btn-small danger'
                            : 'btn-small'
                        }
                        onClick={() => void patchProposalStatus(String(proposal().id), status)}
                      >
                        {status}
                      </button>
                    )}
                  </For>
                </div>
                <h2 style={{ 'margin-top': '1.1rem', 'font-size': '1rem' }}>
                  Comments ({proposalComments().length})
                </h2>
                <Show
                  when={proposalComments().length > 0}
                  fallback={<p class="muted">No review comments yet.</p>}
                >
                  <div class="thread">
                    <For each={proposalComments()}>
                      {(c) => (
                        <div class={`thread-item${c.state === 'resolved' ? ' resolved' : ''}`}>
                          <p>{String(c.body ?? '')}</p>
                          <Show when={c.path}>
                            <p class="muted mono">{String(c.path)}</p>
                          </Show>
                          <StatusPill value={c.state} />
                          <Show when={c.state === 'open'}>
                            <div class="card-actions">
                              <button
                                type="button"
                                class="btn-small"
                                onClick={() => void resolveComment(String(c.id))}
                              >
                                resolve
                              </button>
                            </div>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
                <DetailCommentForm
                  proposalId={String(selectedProposalId())}
                  onPosted={() => setReloadToken((n) => n + 1)}
                />
              </aside>
            )}
          </Show>
        </div>
      </Show>

      <Show when={tab() === 'comments'}>
        <ScopedCards
          label="comments"
          items={comments() ?? []}
          loading={comments.loading}
          error={comments.error}
          onResolve={(id) => void resolveComment(id)}
        />
      </Show>

      <Show when={tab() === 'workflows'}>
        <ScopedCards
          label="workflows"
          items={workflows() ?? []}
          loading={workflows.loading}
          error={workflows.error}
          onPatchWorkflow={(id, status) => void patchWorkflow(id, status)}
        />
      </Show>
    </div>
  );
}

function ScopedCards(props: {
  label: string;
  items: AdminItem[];
  loading: boolean;
  error: unknown;
  onResolve?: (id: string) => void;
  onPatchWorkflow?: (id: string, status: string) => void;
}) {
  return (
    <div class="surface surface-flush" aria-live="polite">
      <Show when={!props.loading} fallback={<Loading text={`Loading ${props.label}…`} />}>
        <Show
          when={!props.error}
          fallback={
            <ErrorText
              text={`Could not load ${props.label}: ${props.error instanceof Error ? props.error.message : String(props.error)}`}
            />
          }
        >
          <Show
            when={props.items.length > 0}
            fallback={<EmptyState title={`No ${props.label} for this project`} />}
          >
            <div class="cards" style={{ padding: '0.85rem' }}>
              <For each={props.items}>
                {(item) => (
                  <article class="card">
                    <h3>{String(item.name ?? item.title ?? item.id ?? 'Item')}</h3>
                    <Show when={item.id}>
                      <p class="muted mono">{item.id}</p>
                    </Show>
                    <StatusPill value={item.status} />
                    <StatusPill value={item.state} />
                    <Show when={item.proposalId}>
                      <p class="muted">proposal: {String(item.proposalId)}</p>
                    </Show>
                    <Show when={item.body}>
                      <p>{String(item.body)}</p>
                    </Show>
                    <Show when={item.path}>
                      <p class="muted mono">{String(item.path)}</p>
                    </Show>
                    <RefList label="policies" refs={item.policyRefs} />
                    <Show when={props.onResolve && item.state === 'open'}>
                      <div class="card-actions">
                        <button
                          type="button"
                          class="btn-small"
                          onClick={() => props.onResolve?.(String(item.id))}
                        >
                          resolve
                        </button>
                      </div>
                    </Show>
                    <Show when={props.onPatchWorkflow && item.id}>
                      <div class="card-actions">
                        <For each={WORKFLOW_NEXT[String(item.status)] ?? []}>
                          {(status) => (
                            <button
                              type="button"
                              class={status === 'failed' ? 'btn-small danger' : 'btn-small'}
                              onClick={() => props.onPatchWorkflow?.(String(item.id), status)}
                            >
                              {status}
                            </button>
                          )}
                        </For>
                      </div>
                    </Show>
                  </article>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

function DetailCommentForm(props: { proposalId: string; onPosted: () => void }) {
  const [status, setStatus] = createSignal('');
  const [error, setError] = createSignal(false);

  async function onSubmit(event: Event) {
    event.preventDefault();
    const form = event.currentTarget as HTMLFormElement;
    const data = Object.fromEntries(new FormData(form).entries());
    setError(false);
    setStatus('Posting…');
    try {
      await apiPost('/admin/review-comments', {
        proposalId: props.proposalId,
        body: data.body,
        path: data.path || undefined,
        authorPrincipal: getActingPrincipal(),
      });
      setStatus('Posted');
      form.reset();
      props.onPosted();
    } catch (err) {
      setError(true);
      setStatus(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <form class="form-card" onSubmit={onSubmit} style={{ 'margin-top': '1rem' }}>
      <h2>Reply on this proposal</h2>
      <label>
        Body
        <textarea name="body" rows={3} required placeholder="Review note" />
      </label>
      <label>
        Path (optional)
        <input name="path" type="text" placeholder="src/main.rs" autocomplete="off" />
      </label>
      <button type="submit">Post comment</button>
      <FormStatus message={status()} error={error()} />
    </form>
  );
}
