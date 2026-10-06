import { useNavigate } from '@solidjs/router';
import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import type { Project } from '../domain.ts';
import { useEffectivePrincipal } from '../session.ts';
import { Icon } from './Icon.tsx';
import { EmptyState, ErrorText, Loading, Modal } from './ui.tsx';

export function ProjectSwitcher() {
  const [open, setOpen] = createSignal(false);
  function show() {
    if (!document.querySelector('dialog[open]')) setOpen(true);
  }
  function onShortcut(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing || event.repeat || event.altKey || event.shiftKey
      || !(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'k') return;
    const target = event.target;
    if (target instanceof Element && target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return;
    if (document.querySelector('dialog[open]')) return;
    event.preventDefault();
    setOpen(true);
  }
  onMount(() => document.addEventListener('keydown', onShortcut));
  onCleanup(() => document.removeEventListener('keydown', onShortcut));

  return <>
    <button type="button" class="icon-button ghost" aria-label="Switch project" title="Switch project (Ctrl/Cmd+K)" aria-expanded={open()} onClick={show}>
      <Icon name="project" />
    </button>
    <Show when={open()}><ProjectPicker onClose={() => setOpen(false)} /></Show>
  </>;
}

function ProjectPicker(props: { onClose: () => void }) {
  const navigate = useNavigate();
  const [query, setQuery] = createSignal('');
  const [projects, { refetch }] = createResource(useEffectivePrincipal(), async () =>
    unwrapList(await apiGet('/projects')) as Project[],
  );
  const matches = createMemo(() => {
    if (projects.error) return [];
    const search = query().trim().toLowerCase();
    return (projects() ?? []).filter(project => project.id &&
      [project.name, project.id, project.organizationId].some(value => value?.toLowerCase().includes(search)));
  });

  return <Modal labelledBy="switch-project-title" onClose={props.onClose}>
    <div class="form-card">
      <div class="dialog-heading">
        <h2 id="switch-project-title">Switch project</h2>
        <button class="dialog-close" type="button" aria-label="Close project switcher" onClick={props.onClose}>×</button>
      </div>
      <label>
        <span>Find a project</span>
        <input type="search" autofocus autocomplete="off" value={query()} onInput={event => setQuery(event.currentTarget.value)} />
      </label>
      <Show when={!projects.loading} fallback={<Loading text="Loading projects…" />}>
        <Show when={!projects.error} fallback={<>
          <ErrorText text="Projects could not be loaded." />
          <button type="button" onClick={() => void refetch()}>Retry projects</button>
        </>}>
          <Show when={matches().length > 0} fallback={<EmptyState title={(projects() ?? []).length === 0 ? 'No projects available' : 'No matching projects'} />}>
            <ul class="plain-list">
              <For each={matches()}>{project => <li>
                <button type="button" class="ghost" onClick={() => {
                  props.onClose();
                  navigate(`/projects/${encodeURIComponent(project.id!)}`);
                }}>{project.name ?? project.id} · {project.organizationId ?? project.id}</button>
              </li>}</For>
            </ul>
          </Show>
        </Show>
      </Show>
    </div>
  </Modal>;
}
