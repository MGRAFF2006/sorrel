import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';
import { ProposalIntegration } from '../src/components/ProposalIntegration.tsx';
import { configureApiClient } from '../src/api.ts';

afterEach(() => { configureApiClient(); vi.unstubAllGlobals(); });
const binding = { id: 'binding', candidateSnapshot: 'candidate', inputs: { sourceRef: 'source', targetRef: 'main', sourceSnapshot: 'source-id', targetSnapshot: 'target-id' }, conflicts: [] };
function client(initial: Record<string, unknown>, mutate?: (url: string, body: Record<string, unknown>) => Record<string, unknown>) {
  let state = initial;
  const writes: { url: string; body: Record<string, unknown> }[] = [];
  configureApiClient({ fetch: async (input, init) => {
    const url = String(input);
    if (init?.method === 'POST') {
      const body = JSON.parse(String(init.body)) as Record<string, unknown>;
      writes.push({ url, body });
      state = mutate?.(url, body) ?? state;
    }
    const data = url.includes('integration-changes') ? { sourceSnapshot: 'candidate', targetSnapshot: 'target-id', changes: [] } : state;
    return new Response(JSON.stringify({ data }), { status: 200 });
  } });
  return writes;
}
test('capability gating explains unavailable integration and makes no request', () => {
  const writes = client({ state: 'unprepared' });
  render(() => <ProposalIntegration proposalId="p" available={false} reason="Filesystem stores required" />);
  expect(screen.getByText('Filesystem stores required')).toBeInTheDocument();
  expect(screen.queryByText(/Prepare fresh/)).toBeNull();
  expect(writes).toHaveLength(0);
});
test('preparing never automatically approves or invents check success', async () => {
  const writes = client({ state: 'unprepared', requiredChecksConfigured: false }, () => ({ state: 'blocked', binding, canApprove: true, ready: false, approved: false, blockers: ['trusted_required_checks_not_configured', 'non_author_approval_required'], requiredChecks: [{ id: 'tests', outcome: 'missing', trusted: false }] }));
  render(() => <ProposalIntegration proposalId="p" available />);
  await screen.findByText(/operator must configure/);
  await fireEvent.click(screen.getByText(/Prepare fresh/));
  await screen.findByRole('button', { name: 'Approve this candidate' });
  expect(writes).toEqual([{ url: '/api/admin/proposals/p/integration', body: {} }]);
  expect(screen.queryByRole('button', { name: 'Integrate into main' })).toBeNull();
  await fireEvent.click(screen.getByRole('button', { name: 'Approve this candidate' }));
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1].body).toEqual({ bindingId: 'binding', candidateSnapshot: 'candidate' });
});
test('stale/conflicted evidence blocks approval and integration', async () => {
  client({ state: 'conflicted', binding: { ...binding, candidateSnapshot: null, conflicts: [{ path: 'file', conflictType: 'content', id: 'conflict-id' }] }, canApprove: true, ready: false, blockers: ['stale_target'] });
  render(() => <ProposalIntegration proposalId="p" available />);
  await screen.findByText(/file: content/);
  expect(screen.queryByRole('button', { name: 'Approve this candidate' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Integrate into main' })).toBeNull();
});
test('only a successful receipt displays Integrated; a lost response retries the same operation', async () => {
  const calls: Record<string, unknown>[] = [];
  let published = false;
  let failOnce = true;
  configureApiClient({ fetch: async (input, init) => {
    if (init?.method === 'POST') {
      calls.push(JSON.parse(String(init.body)) as Record<string, unknown>);
      if (failOnce) { failOnce = false; throw new Error('response lost'); }
      published = true;
    }
    const data = String(input).includes('integration-changes') ? { sourceSnapshot: 'candidate', targetSnapshot: 'target-id', changes: [] } : published ? { state: 'integrated', receipt: { candidateSnapshot: 'candidate', targetRef: 'main', operationId: 'receipt', publishedAt: 'today' } } : { state: 'ready', binding, ready: true };
    return new Response(JSON.stringify({ data }));
  } });
  render(() => <ProposalIntegration proposalId="p" available />);
  await fireEvent.click(await screen.findByRole('button', { name: 'Integrate into main' }));
  expect(screen.queryByText('Integrated')).toBeNull();
  const retry = await screen.findByRole('button', { name: 'Retry same publication' });
  await waitFor(() => expect(retry).not.toBeDisabled());
  await fireEvent.click(retry);
  await screen.findByText('Integrated');
  expect(calls).toHaveLength(2);
  expect(calls[1]).toEqual(calls[0]);
});
