import type { JSX, ParentProps } from 'solid-js';
import { Show } from 'solid-js';

export function StatusPill(props: { value?: string | null }) {
  const value = () => props.value;
  return (
    <Show when={value()}>
      {(v) => (
        <span class={`pill pill-${String(v()).toLowerCase().replace(/[^a-z0-9]+/g, '-')}`}>
          {v()}
        </span>
      )}
    </Show>
  );
}

export function RefList(props: { label: string; refs?: unknown }) {
  const refs = () => (Array.isArray(props.refs) ? props.refs : []);
  return (
    <Show when={refs().length > 0}>
      <div class="refs">
        <span class="refs-label">{props.label}</span>
        <ul>
          {refs().map((ref) => (
            <li>
              {typeof ref === 'string'
                ? ref
                : `${(ref as { kind?: string }).kind ?? '?'}:${(ref as { id?: string }).id ?? '?'}`}
            </li>
          ))}
        </ul>
      </div>
    </Show>
  );
}

export function FormStatus(props: { message: string; error?: boolean }) {
  return (
    <Show when={props.message}>
      <p class={props.error ? 'error' : 'muted'} aria-live="polite">
        {props.message}
      </p>
    </Show>
  );
}

export function Loading(props: { text: string }) {
  return (
    <div class="loading-state" role="status">
      <span class="loading-pulse" aria-hidden="true" />
      <p class="muted">{props.text}</p>
    </div>
  );
}

export function ErrorText(props: { text: string }) {
  return (
    <div class="error-banner" role="alert">
      <p class="error">{props.text}</p>
    </div>
  );
}

export function PageHeader(
  props: ParentProps<{
    title: string;
    lede?: string;
    actions?: JSX.Element;
  }>,
) {
  return (
    <header class="page-header">
      <div class="page-header-text">
        <h1>{props.title}</h1>
        <Show when={props.lede}>
          <p class="muted lede">{props.lede}</p>
        </Show>
      </div>
      <Show when={props.actions}>
        <div class="page-header-actions">{props.actions}</div>
      </Show>
      {props.children}
    </header>
  );
}

export function EmptyState(props: {
  title: string;
  body?: string;
  action?: JSX.Element;
}) {
  return (
    <div class="empty-state">
      <p class="empty-title">{props.title}</p>
      <Show when={props.body}>
        <p class="muted">{props.body}</p>
      </Show>
      <Show when={props.action}>
        <div class="empty-action">{props.action}</div>
      </Show>
    </div>
  );
}

export function MetaGrid(props: { entries: Array<{ label: string; value: string; mono?: boolean }> }) {
  return (
    <dl class="detail-meta">
      {props.entries.map((entry) => (
        <>
          <dt>{entry.label}</dt>
          <dd class={entry.mono ? 'mono' : undefined}>{entry.value}</dd>
        </>
      ))}
    </dl>
  );
}
