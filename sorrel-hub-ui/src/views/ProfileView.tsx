import { A } from '@solidjs/router';
import { createMemo, createResource, For, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import { Icon } from '../components/Icon.tsx';
import { EmptyState, ErrorText, Loading, StatusPill } from '../components/ui.tsx';
import type { Project, Proposal } from '../domain.ts';
import { initials, relativeTime } from '../domain.ts';
import { useActingPrincipal } from '../session.ts';

export function ProfileView() {
  const principal = useActingPrincipal();
  const [data] = createResource(async () => {
    const [projectsPayload, proposalsPayload] = await Promise.all([
      apiGet('/projects'),
      apiGet('/admin/proposals'),
    ]);
    return {
      projects: unwrapList(projectsPayload) as Project[],
      proposals: unwrapList(proposalsPayload) as Proposal[],
    };
  });
  const authored = createMemo(() => (data()?.proposals ?? []).filter((proposal) =>
    proposal.authorPrincipal?.type === principal().type && proposal.authorPrincipal?.id === principal().id,
  ));
  const projectMap = createMemo(() => new Map((data()?.projects ?? []).map((project) => [project.id, project])));

  return (
    <div class="identity-page profile-page view-enter">
      <header class="identity-header">
        <span class="identity-mark profile">{initials(principal().id)}</span>
        <div>
          <p class="section-label">Current principal</p>
          <h1>{principal().id}</h1>
          <p>This profile reflects the identity currently acting in Hub.</p>
          <div class="identity-meta"><span class="mono">{principal().type}:{principal().id}</span><span><Icon name="branch" />{authored().length} proposals</span></div>
        </div>
      </header>
      <nav class="identity-tabs"><span class="active">Overview</span><span>Activity <b>{authored().length}</b></span></nav>
      <Show when={!data.loading} fallback={<Loading text="Loading profile activity…" />}>
        <Show when={!data.error} fallback={<ErrorText text="Profile activity could not be loaded." />}>
          <div class="profile-layout">
            <aside class="profile-card surface">
              <div class="profile-cover" />
              <div class="profile-body">
                <span class="profile-avatar">{initials(principal().id)}</span>
                <h2>{principal().id}</h2>
                <p class="mono">{principal().type}</p>
                <p>Hub identity backed by the active development or authenticated session.</p>
                <dl><div><dt>Shared proposals</dt><dd>{authored().length}</dd></div><div><dt>Active</dt><dd>{authored().filter((item) => !['merged', 'closed'].includes(item.status ?? '')).length}</dd></div></dl>
              </div>
            </aside>
            <main class="profile-main">
              <article class="identity-readme surface">
                <header><Icon name="book" /><strong>{principal().id} / README.md</strong></header>
                <EmptyState title="No profile README yet" body="Hub does not invent a biography from activity. A persisted profile model can place a personal Markdown README here later." />
              </article>
              <section class="profile-activity surface">
                <header><Icon name="workflow" /><strong>Recent work</strong></header>
                <Show when={authored().length > 0} fallback={<EmptyState title="No authored proposals" />}>
                  <For each={authored().slice(0, 8)}>{(proposal) => {
                    const project = () => projectMap().get(proposal.projectId);
                    return (
                      <A href={`/projects/${encodeURIComponent(proposal.projectId ?? '')}/reviews?proposal=${encodeURIComponent(proposal.id ?? '')}`}>
                        <div><strong>{proposal.title ?? proposal.id}</strong><p>{project()?.name ?? proposal.projectId} · {relativeTime(proposal.updatedAt)}</p></div><StatusPill value={proposal.status} />
                      </A>
                    );
                  }}</For>
                </Show>
              </section>
            </main>
          </div>
        </Show>
      </Show>
    </div>
  );
}
