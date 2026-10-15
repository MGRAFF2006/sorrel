import { createEffect, createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, apiPost } from '../api.ts';
import { ReviewChanges } from './ReviewChanges.tsx';
import { ErrorText, Loading } from './ui.tsx';

type Binding = {
  id: string;
  candidateSnapshot: string | null;
  requirementsDigest: string;
  inputs: { sourceRef: string; targetRef: string; sourceSnapshot: string; targetSnapshot: string };
  conflicts: { id?: string; path?: string; type?: string; conflictType?: string }[];
};
type Readiness = {
  state: string;
  binding?: Binding;
  blockers?: string[];
  ready?: boolean;
  canApprove?: boolean;
  canIntegrate?: boolean;
  approved?: boolean;
  requiredChecksConfigured?: boolean;
  requiredChecks?: { id: string; outcome: string; trusted: boolean; attempt: number | null; runId: string | null }[];
  receipt?: { operationId: string; candidateSnapshot: string; targetRef: string; publishedAt: string };
};
export function ProposalIntegration(props: { proposalId: string; available: boolean; reason?: string | null }) {
  const [revision, setRevision] = createSignal(0);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  // Preserve this exact payload after ambiguous network failures; retry never generates a new operation.
  const [attempt, setAttempt] = createSignal<{ bindingId: string; expectedTargetSnapshot: string; idempotencyKey: string }>();
  createEffect(() => { props.proposalId; setAttempt(undefined); setError(''); });
  const endpoint = () => `/admin/proposals/${encodeURIComponent(props.proposalId)}`;
  const [state, { refetch }] = createResource(() => props.available && { id: props.proposalId, revision: revision() },
    async () => (await apiGet(`${endpoint()}/integration`) as { data: Readiness }).data);
  async function act(kind: 'prepare' | 'approve' | 'integrate') {
    setPending(true); setError('');
    try {
      const binding = state()?.binding;
      if (kind === 'prepare') {
        await apiPost(`${endpoint()}/integration`, {});
        setAttempt(undefined);
      } else if (kind === 'approve' && binding) {
        await apiPost(`${endpoint()}/approvals`, { bindingId: binding.id, candidateSnapshot: binding.candidateSnapshot });
      } else if (kind === 'integrate') {
        const payload = attempt() ?? (binding ? { bindingId: binding.id, expectedTargetSnapshot: binding.inputs.targetSnapshot, idempotencyKey: crypto.randomUUID() } : undefined);
        if (!payload) throw new Error('Prepare a candidate first');
        setAttempt(payload);
        const result = (await apiPost(`${endpoint()}/integrate`, payload) as { data: { state: string; reason?: string } }).data;
        if (result.state !== 'integrated') throw new Error(result.reason ?? 'Publication was not completed');
        setAttempt(undefined);
      }
      setRevision(n => n + 1);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      // A receipt may have committed even when the response was lost.
      void refetch();
    } finally { setPending(false); }
  }
  return <section class="card" aria-label="Proposal integration">
    <h2>Integration</h2>
    <Show when={props.available} fallback={<p class="muted">{props.reason ?? 'This Hub does not provide proposal integration.'}</p>}>
      <Show when={state.loading}><Loading text="Checking publication and evidence…" /></Show>
      <Show when={state.error}><ErrorText text={String(state.error)} /><button type="button" onClick={() => void refetch()}>Retry readiness</button></Show>
      <Show when={error()}><ErrorText text={error()} /></Show>
      <Show when={state()?.receipt} fallback={<>
        <p>Publication: {state()?.state ?? 'checking'}</p>
        <p class="muted">Integration requires one explicit non-author approval and trusted test/CI reports for this exact candidate. Manual review statuses and editable workflow results provide no integration evidence.</p>
        <Show when={state()?.requiredChecksConfigured === false}><p>Blocked: an operator must configure trusted required test reporters.</p></Show>
        <Show when={state()?.binding}>{binding => <>
          <dl class="detail-meta">
            <dt>Source</dt><dd class="mono">{binding().inputs.sourceRef} @ {binding().inputs.sourceSnapshot}</dd>
            <dt>Target before publication</dt><dd class="mono">{binding().inputs.targetRef} @ {binding().inputs.targetSnapshot}</dd>
            <dt>Candidate snapshot</dt><dd class="mono">{binding().candidateSnapshot ?? 'No candidate: conflicts require resolution'}</dd>
            <dt>Binding</dt><dd class="mono">{binding().id}</dd>
            <dt>Approval</dt><dd>{state()?.approved ? 'Authenticated non-author approval recorded' : 'Explicit non-author approval required'}</dd>
          </dl>
          <Show when={binding().candidateSnapshot}><ReviewChanges proposalId={props.proposalId} bindingId={binding().id} /></Show>
          <For each={binding().conflicts}>{conflict => <p>{conflict.path ?? 'History'}: {conflict.conflictType ?? conflict.type ?? 'conflict'} {conflict.id}</p>}</For>
          <For each={state()?.requiredChecks}>{check => <p>{check.id}: {check.outcome}{check.trusted ? ' (trusted exact-candidate success)' : ' (blocking)'}{check.runId ? ` · run ${check.runId}, attempt ${check.attempt}` : ''}</p>}</For>
          <For each={state()?.blockers}>{reason => <p>{reason.replaceAll('_', ' ')}</p>}</For>
          <Show when={state()?.canApprove && binding().candidateSnapshot && !state()?.approved && !state()?.blockers?.some(reason => ['review_changed', 'superseded_source', 'stale_target', 'requirements_changed'].includes(reason))}>
            <button type="button" disabled={pending() || state.loading || Boolean(state.error)} onClick={() => void act('approve')}>Approve this candidate</button>
          </Show>
          <Show when={state()?.ready}><button type="button" disabled={pending() || state.loading || Boolean(state.error)} onClick={() => void act('integrate')}>Integrate into {binding().inputs.targetRef}</button></Show>
        </>}</Show>
        <button type="button" disabled={pending() || state.loading || Boolean(state.error)} onClick={() => void act('prepare')}>Prepare fresh candidate (resets evidence)</button>
        <Show when={attempt()}><button type="button" disabled={pending() || state.loading || Boolean(state.error)} onClick={() => void act('integrate')}>Retry same publication</button></Show>
      </>}>
        {receipt => <><p><strong>Integrated</strong> into {receipt().targetRef}</p><p class="mono">{receipt().candidateSnapshot}</p><p>Publication receipt: {receipt().operationId} · {receipt().publishedAt}</p></>}
      </Show>
      <button type="button" disabled={pending() || state.loading || Boolean(state.error)} onClick={() => void refetch()}>Refresh integration readiness</button>
    </Show>
  </section>;
}
