import { A } from '@solidjs/router';
import { createSignal, Show } from 'solid-js';
import { createAction } from '../action.ts';
import { apiPost } from '../api.ts';
import { useEffectivePrincipal } from '../session.ts';

type Decision = { status: 'approved' | 'denied'; label: string };

/** Display form of a typed or linked code: upper case, XXXX-XXXX. */
export function formatUserCode(value: string): string {
  const compact = value.toUpperCase().replace(/[^A-Z]/g, '').slice(0, 8);
  return compact.length > 4 ? `${compact.slice(0, 4)}-${compact.slice(4)}` : compact;
}

/**
 * Approve or deny a pending `sorrel login`. Hub takes the identity from this
 * browser's signed-in session; the page only sends the code and the decision.
 */
export function CliActivateView() {
  const principal = useEffectivePrincipal();
  const linked = typeof window === 'undefined' ? '' : new URLSearchParams(window.location.search).get('code') ?? '';
  const [code, setCode] = createSignal(formatUserCode(linked));
  const [decision, setDecision] = createSignal<Decision | null>(null);
  const action = createAction();
  const complete = () => code().length === 9;
  const decide = (choice: 'approve' | 'deny') => void action.run(async () => {
    const payload = await apiPost('/auth/cli/approve', { user_code: code(), decision: choice }) as { data: Decision };
    setDecision(payload.data);
  });

  return (
    <section class="cli-activate surface view-enter" aria-labelledby="cli-activate-title">
      <h1 id="cli-activate-title">Approve CLI sign-in</h1>
      <Show when={decision()} fallback={
        <form onSubmit={(event) => { event.preventDefault(); if (complete()) decide('approve'); }}>
          <p>
            Approve only if you just ran <code>sorrel login</code> yourself and the code below matches the one in
            your terminal. Never approve a code someone else sent you.
          </p>
          <p class="muted">
            Approving gives that terminal a revocable token that acts as <span class="mono">{principal().type}:{principal().id}</span> on
            this Hub for 30 days. It cannot change what you have access to, and it cannot administer policies.
          </p>
          <label class="cli-code-field">
            <span>Code from your terminal</span>
            <input
              class="mono"
              name="user_code"
              autocomplete="off"
              spellcheck={false}
              placeholder="XXXX-XXXX"
              value={code()}
              onInput={(event) => setCode(formatUserCode(event.currentTarget.value))}
            />
          </label>
          <div class="cli-activate-actions">
            <button type="submit" disabled={!complete() || action.pending()}>{action.pending() ? 'Working…' : 'Approve'}</button>
            <button type="button" class="ghost" disabled={!complete() || action.pending()} onClick={() => decide('deny')}>Deny</button>
          </div>
          <Show when={action.error()}><p class="error" role="alert">{action.error()}</p></Show>
        </form>
      }>
        {(result) => (
          <div role="status">
            <Show when={result().status === 'approved'} fallback={
              <p>Denied. The terminal will report that the login was refused. No token was issued.</p>
            }>
              <p>Approved “{result().label}”. Return to your terminal; it finishes signing in within a few seconds.</p>
            </Show>
            <p class="muted">Review or revoke CLI tokens on your <A href="/profile">profile</A>.</p>
          </div>
        )}
      </Show>
    </section>
  );
}
