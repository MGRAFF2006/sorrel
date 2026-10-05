import { createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, shortId } from '../api.ts';
import type { ProposalChanges } from '../domain.ts';
import { SourcePreview } from './SourcePreview.tsx';
import { ErrorText, Loading } from './ui.tsx';

export function ReviewChanges(props: { proposalId: string }) {
  const [changes, { refetch }] = createResource(() => props.proposalId, async id =>
    ((await apiGet(`/admin/proposals/${encodeURIComponent(id)}/changes`)) as { data: ProposalChanges }).data,
  );
  return <section class="review-changes">
    <h2>Changes</h2>
    <Show when={!changes.loading} fallback={<Loading text="Comparing snapshots…" />}>
      <Show when={!changes.error} fallback={<>
        <ErrorText text={`Comparison unavailable: ${changes.error instanceof Error ? changes.error.message : String(changes.error)}`} />
        <button type="button" class="ghost" onClick={() => void refetch()}>Retry comparison</button>
      </>}>
        <Show when={changes()}>{data => <>
          <p class="muted mono">{shortId(data().targetSnapshot)} → {shortId(data().sourceSnapshot)}</p>
          <p class="muted">Recorded target and source snapshots. Later pushes do not change this comparison.</p>
          <Show when={data().changes.length > 0} fallback={<p class="muted">These snapshots have no changed files.</p>}>
            <For each={data().changes}>{(change, index) => {
              const [expanded, setExpanded] = createSignal(index() === 0);
              return <details class="changed-file" open={expanded()} onToggle={event => setExpanded(event.currentTarget.open)}>
              <summary><strong>{change.path}</strong><span class="pill">{change.status}</span></summary>
              <Show when={expanded()}><div class="comparison-panes">
                <section><h3>Before<Show when={change.before.mode}> · {change.before.mode}</Show></h3><SourcePreview content={change.before.content} reason={change.before.reason} label={`Before ${change.path}`} /></section>
                <section><h3>After<Show when={change.after.mode}> · {change.after.mode}</Show></h3><SourcePreview content={change.after.content} reason={change.after.reason} label={`After ${change.path}`} /></section>
              </div></Show>
            </details>;
            }}</For>
          </Show>
        </>}</Show>
      </Show>
    </Show>
  </section>;
}
