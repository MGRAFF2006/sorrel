import { A } from '@solidjs/router';
import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import { Icon } from '../components/Icon.tsx';
import { EmptyState, ErrorText, Loading, StatusPill } from '../components/ui.tsx';
import type { Project, Proposal, ReviewComment, WorkflowRun } from '../domain.ts';
import { principalLabel, relativeTime } from '../domain.ts';

type InboxItem = {
  id: string;
  kind: 'review' | 'comment' | 'workflow';
  label: string;
  title: string;
  summary: string;
  status?: string;
  projectId?: string;
  projectName: string;
  proposalId?: string;
  actor?: string;
  updatedAt?: string;
  needsAction: boolean;
};

export function InboxView() {
  const [filter, setFilter] = createSignal<'needs' | 'all'>('needs');
  const [selectedId, setSelectedId] = createSignal<string | null>(null);

  const [data] = createResource(async () => {
    const [projectsPayload, proposalsPayload, commentsPayload, workflowsPayload] = await Promise.all([
      apiGet('/projects'),
      apiGet('/admin/proposals'),
      apiGet('/admin/review-comments'),
      apiGet('/admin/workflow-runs'),
    ]);
    return {
      projects: unwrapList(projectsPayload) as Project[],
      proposals: unwrapList(proposalsPayload) as Proposal[],
      comments: unwrapList(commentsPayload) as ReviewComment[],
      workflows: unwrapList(workflowsPayload) as WorkflowRun[],
    };
  });

  const items = createMemo<InboxItem[]>(() => {
    const value = data();
    if (!value) return [];
    const projectNames = new Map(value.projects.map((project) => [project.id, project.name ?? project.id ?? 'Project']));
    const proposalById = new Map(value.proposals.map((proposal) => [proposal.id, proposal]));
    const result: InboxItem[] = [];

    for (const proposal of value.proposals) {
      if (!proposal.id || ['merged', 'closed'].includes(proposal.status ?? '')) continue;
      const label = proposal.status === 'approved'
        ? 'Ready to integrate'
        : proposal.status === 'rejected'
          ? 'Changes requested'
          : proposal.status === 'draft'
            ? 'Draft review'
            : 'Review open';
      result.push({
        id: `proposal:${proposal.id}`,
        kind: 'review',
        label,
        title: proposal.title ?? proposal.id,
        summary: proposal.description ?? `Review the work shared from ${proposal.sourceLane ?? proposal.sourceBranch ?? 'a local lane'}.`,
        status: proposal.status,
        projectId: proposal.projectId,
        projectName: projectNames.get(proposal.projectId) ?? proposal.projectId ?? 'Project',
        proposalId: proposal.id,
        actor: principalLabel(proposal.authorPrincipal) !== 'Unknown' ? principalLabel(proposal.authorPrincipal) : proposal.authorRef,
        updatedAt: proposal.updatedAt ?? proposal.createdAt,
        needsAction: ['open', 'approved', 'rejected'].includes(proposal.status ?? ''),
      });
    }

    for (const comment of value.comments) {
      if (!comment.id || comment.state !== 'open') continue;
      const proposal = proposalById.get(comment.proposalId);
      result.push({
        id: `comment:${comment.id}`,
        kind: 'comment',
        label: 'Open discussion',
        title: proposal?.title ?? 'Review comment',
        summary: comment.body ?? 'An unresolved review thread needs attention.',
        status: comment.state,
        projectId: proposal?.projectId,
        projectName: projectNames.get(proposal?.projectId) ?? proposal?.projectId ?? 'Project',
        proposalId: comment.proposalId,
        actor: principalLabel(comment.authorPrincipal) !== 'Unknown' ? principalLabel(comment.authorPrincipal) : comment.authorRef,
        updatedAt: comment.updatedAt ?? comment.createdAt,
        needsAction: true,
      });
    }

    for (const workflow of value.workflows) {
      if (!workflow.id || !['failed', 'cancelled'].includes(workflow.status ?? '')) continue;
      result.push({
        id: `workflow:${workflow.id}`,
        kind: 'workflow',
        label: workflow.status === 'failed' ? 'Check failed' : 'Check cancelled',
        title: workflow.name ?? workflow.id,
        summary: `Workflow ${workflow.status}. Open the associated review to inspect the run context.`,
        status: workflow.status,
        projectId: workflow.projectId,
        projectName: projectNames.get(workflow.projectId) ?? workflow.projectId ?? 'Project',
        proposalId: workflow.proposalId,
        updatedAt: workflow.updatedAt ?? workflow.createdAt,
        needsAction: workflow.status === 'failed',
      });
    }

    return result.sort((left, right) => Date.parse(right.updatedAt ?? '') - Date.parse(left.updatedAt ?? ''));
  });
  const visibleItems = createMemo(() => filter() === 'needs' ? items().filter((item) => item.needsAction) : items());
  const selected = createMemo(() => visibleItems().find((item) => item.id === selectedId()) ?? visibleItems()[0]);

  return (
    <div class="inbox-view view-enter">
      <aside class="inbox-nav">
        <h1>Inbox</h1>
        <p>Cross-project decisions and failed work.</p>
        <button class={filter() === 'needs' ? 'active' : ''} type="button" onClick={() => setFilter('needs')}>
          <Icon name="inbox" /><span>Needs you</span><strong>{items().filter((item) => item.needsAction).length}</strong>
        </button>
        <button class={filter() === 'all' ? 'active' : ''} type="button" onClick={() => setFilter('all')}>
          <Icon name="archive" /><span>All current</span><strong>{items().length}</strong>
        </button>
        <div class="inbox-note">
          <p class="section-label">Global, not home</p>
          <p>Projects remain the entry point. Inbox is the place to make decisions across them.</p>
        </div>
      </aside>

      <section class="inbox-queue">
        <header><div><p class="section-label">Decision queue</p><h2>{filter() === 'needs' ? 'Needs you' : 'All current activity'}</h2></div><span>{visibleItems().length}</span></header>
        <Show when={!data.loading} fallback={<Loading text="Loading inbox…" />}>
          <Show when={!data.error} fallback={<ErrorText text="The inbox could not be loaded." />}>
            <Show when={visibleItems().length > 0} fallback={<EmptyState title="Nothing needs your attention" body="Open reviews, unresolved comments, and failed workflows will appear here." />}>
              <For each={visibleItems()}>{(item) => (
                <button class={`inbox-item ${selected()?.id === item.id ? 'active' : ''}`} type="button" onClick={() => setSelectedId(item.id)}>
                  <span class={`inbox-kind ${item.kind}`}><Icon name={item.kind === 'review' ? 'branch' : item.kind === 'comment' ? 'inbox' : 'workflow'} /></span>
                  <span class="inbox-item-copy">
                    <span class="inbox-item-top"><strong>{item.label}</strong><time>{relativeTime(item.updatedAt)}</time></span>
                    <span class="inbox-item-title">{item.title}</span>
                    <span class="inbox-item-summary">{item.summary}</span>
                    <span class="inbox-tags"><StatusPill value={item.status} /><span>{item.projectName}</span></span>
                  </span>
                </button>
              )}</For>
            </Show>
          </Show>
        </Show>
      </section>

      <section class="inbox-detail">
        <Show when={selected()} fallback={<EmptyState title="Select an item" body="Its context and next step appear here." />}>
          {(item) => (
            <article>
              <p class="detail-crumb">{item().projectName} / {item().label}</p>
              <StatusPill value={item().status} />
              <h2>{item().title}</h2>
              <p class="detail-lede">{item().summary}</p>
              <div class="decision-strip">
                <span class={`inbox-kind ${item().kind}`}><Icon name={item().kind === 'review' ? 'branch' : item().kind === 'comment' ? 'inbox' : 'workflow'} /></span>
                <div>
                  <strong>{item().label}</strong>
                  <p>{item().actor ? `${item().actor} · ` : ''}{relativeTime(item().updatedAt)}</p>
                </div>
                <Show when={item().projectId}>
                  <A class="button primary" href={item().proposalId
                    ? `/projects/${encodeURIComponent(item().projectId!)}/reviews?proposal=${encodeURIComponent(item().proposalId!)}`
                    : `/projects/${encodeURIComponent(item().projectId!)}`}>Open context</A>
                </Show>
              </div>
              <section class="detail-explainer">
                <h3>Why this is here</h3>
                <p>{item().kind === 'comment'
                  ? 'This discussion remains unresolved on an active proposal.'
                  : item().kind === 'workflow'
                    ? 'The Hub recorded a workflow outcome that needs inspection.'
                    : 'This proposal is still in a state that may require a review or integration decision.'}</p>
              </section>
            </article>
          )}
        </Show>
      </section>
    </div>
  );
}
