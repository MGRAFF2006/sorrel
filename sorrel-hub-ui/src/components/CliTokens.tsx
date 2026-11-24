import { createResource, For, Show } from 'solid-js';
import { createAction } from '../action.ts';
import { apiDelete, apiGet, unwrapList } from '../api.ts';
import { EmptyState, ErrorText, Loading, StatusPill } from './ui.tsx';
import { Icon } from './Icon.tsx';

export type CliToken = {
  id: string;
  label: string;
  status: 'active' | 'expired' | 'revoked';
  createdAt: string;
  expiresAt: string;
  lastUsedAt: string | null;
};

const day = (value: string | null) => (value ? value.slice(0, 10) : 'never');

/** The signed-in user's Hub-issued CLI tokens, with revoke. No secrets are ever shown. */
export function CliTokensPanel() {
  const [tokens, { refetch }] = createResource(async () => unwrapList(await apiGet('/auth/cli/tokens')) as CliToken[]);
  const revoke = createAction();
  return (
    <section class="profile-activity surface cli-tokens">
      <header><Icon name="code" /><strong>CLI tokens</strong></header>
      <Show when={!tokens.loading} fallback={<Loading text="Loading CLI tokens…" />}>
        <Show when={!tokens.error} fallback={<ErrorText text="CLI tokens could not be loaded." />}>
          <Show when={(tokens() ?? []).length > 0} fallback={<EmptyState title="No CLI tokens" body="Run sorrel login <hub-url> in a terminal to sign in the CLI." />}>
            <ul>
              <For each={tokens()}>{(token) => (
                <li>
                  <div>
                    <strong>{token.label}</strong>
                    <p class="muted">Created {day(token.createdAt)} · expires {day(token.expiresAt)} · last used {day(token.lastUsedAt)}</p>
                  </div>
                  <StatusPill value={token.status} />
                  <Show when={token.status === 'active'}>
                    <button type="button" class="btn-small danger" disabled={revoke.pending()} aria-label={`Revoke ${token.label}`}
                      onClick={() => void revoke.run(async () => {
                        await apiDelete(`/auth/cli/tokens/${encodeURIComponent(token.id)}`);
                        await refetch();
                      })}>Revoke</button>
                  </Show>
                </li>
              )}</For>
            </ul>
          </Show>
          <Show when={revoke.error()}><p class="error" role="alert">{revoke.error()}</p></Show>
        </Show>
      </Show>
    </section>
  );
}
