import { For, Show } from 'solid-js';

/** Keep malformed/binary data and unbounded DOM work out of text previews. */
export function SourcePreview(props: { content: string | null; label: string; reason?: string }) {
  const reasons: Record<string, string> = {
    unsupported_file: 'Binary files cannot be previewed as text.',
    file_too_large: 'This file is too large to preview.',
    preview_limit: 'The comparison preview limit was reached. Open the workspace to inspect this file.',
  };
  const unavailable = () => props.content === null ? (props.reason ? reasons[props.reason] ?? 'This file cannot be previewed.' : 'No file on this side.')
    : props.content.length > 512 * 1024 ? 'This file is too large to preview.'
    : /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(props.content) ? 'Binary files cannot be previewed as text.' : '';
  const lines = () => (props.content ?? '').split('\n');
  return <Show when={!unavailable()} fallback={<p class="muted">{unavailable()}</p>}>
    <pre class="source-preview" aria-label={props.label}><code><For each={lines().slice(0, 2000)}>{(line, index) =>
      <span class="source-line"><span class="line-number" aria-hidden="true">{index() + 1}</span><span>{line || ' '}</span></span>
    }</For></code></pre>
    <Show when={lines().length > 2000}><p class="muted">Showing the first 2,000 lines of {lines().length}. Open the workspace to read the complete file.</p></Show>
  </Show>;
}
