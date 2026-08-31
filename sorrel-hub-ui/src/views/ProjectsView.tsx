import { A, useNavigate } from '@solidjs/router';
import { createResource, createSignal, For, Show } from 'solid-js';
import { apiGet, apiPost, unwrapList } from '../api.ts';
import {
  EmptyState,
  ErrorText,
  FormStatus,
  Loading,
  PageHeader,
  StatusPill,
} from '../components/ui.tsx';

type Project = {
  id?: string;
  name?: string;
  organizationId?: string;
  description?: string;
  status?: string;
  slug?: string;
};

/** Home: project picker — GitHub-style entry point. */
export function ProjectsHome() {
  const navigate = useNavigate();
  const [orgFilter, setOrgFilter] = createSignal('');
  const [status, setStatus] = createSignal('');
  const [statusError, setStatusError] = createSignal(false);
  const [reloadToken, setReloadToken] = createSignal(0);
  const [creating, setCreating] = createSignal(false);

  const [projects] = createResource(
    () => ({ org: orgFilter(), token: reloadToken() }),
    async ({ org }) => {
      const query = org ? `?organizationId=${encodeURIComponent(org)}` : '';
      const data = await apiGet(`/projects${query}`);
      return unwrapList(data) as Project[];
    },
  );

  function openProject(id: string) {
    navigate(`/projects/${encodeURIComponent(id)}`);
  }

  async function onCreate(event: Event) {
    event.preventDefault();
    const form = event.currentTarget as HTMLFormElement;
    const data = Object.fromEntries(new FormData(form).entries());
    setStatusError(false);
    setStatus('Creating…');
    try {
      const created = (await apiPost('/projects', {
        organizationId: data.organizationId,
        name: data.name,
        description: data.description || undefined,
      })) as { data: { id: string } };
      setStatus(`Created ${created.data.id}`);
      form.reset();
      setCreating(false);
      setReloadToken((n) => n + 1);
      openProject(created.data.id);
    } catch (error) {
      setStatusError(true);
      setStatus(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <div class="page view-enter">
      <PageHeader
        title="Projects"
        lede="Pick a project to work in. Reviews, sync, and actions live inside the project."
        actions={
          <>
            <button type="button" class="ghost" onClick={() => setReloadToken((n) => n + 1)}>
              Refresh
            </button>
            <button
              type="button"
              onClick={() => setCreating((v) => !v)}
              aria-expanded={creating()}
            >
              {creating() ? 'Cancel' : 'New project'}
            </button>
          </>
        }
      />

      <div class="toolbar">
        <input
          type="search"
          placeholder="Filter by organizationId…"
          autocomplete="off"
          value={orgFilter()}
          onInput={(e) => setOrgFilter(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') setReloadToken((n) => n + 1);
          }}
        />
      </div>

      <Show when={creating()}>
        <form class="form-card" onSubmit={onCreate}>
          <h2>Create project</h2>
          <div class="form-grid two">
            <label>
              Organization id
              <input name="organizationId" required placeholder="org_local" autocomplete="off" />
            </label>
            <label>
              Name
              <input name="name" required placeholder="Platform" autocomplete="off" />
            </label>
          </div>
          <label>
            Description
            <input name="description" placeholder="optional" autocomplete="off" />
          </label>
          <button type="submit">Create project</button>
          <FormStatus message={status()} error={statusError()} />
        </form>
      </Show>

      <div class="surface surface-flush" aria-live="polite">
        <Show when={!projects.loading} fallback={<Loading text="Loading projects…" />}>
          <Show
            when={!projects.error}
            fallback={
              <ErrorText
                text={`Could not load projects: ${projects.error instanceof Error ? projects.error.message : String(projects.error)}`}
              />
            }
          >
            <Show
              when={(projects() ?? []).length > 0}
              fallback={
                <EmptyState
                  title="No projects yet"
                  body="Create a project to start collaborating through Hub."
                  action={
                    <button type="button" onClick={() => setCreating(true)}>
                      New project
                    </button>
                  }
                />
              }
            >
              <div class="list-stack" role="listbox" aria-label="Projects">
                <For each={projects()}>
                  {(project) => (
                    <A
                      href={project.id ? `/projects/${encodeURIComponent(project.id)}` : '/'}
                      class="list-row list-row-link"
                      role="option"
                    >
                      <div>
                        <div class="row-title">{project.name ?? project.id ?? 'Untitled'}</div>
                        <p class="row-sub mono">{project.id ?? '—'}</p>
                        <Show when={project.organizationId}>
                          <p class="row-sub">org · {project.organizationId}</p>
                        </Show>
                        <Show when={project.description}>
                          <p class="row-sub">{project.description}</p>
                        </Show>
                      </div>
                      <StatusPill value={project.status} />
                    </A>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Show>
      </div>
    </div>
  );
}
