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
        title="Your projects"
        lede="Review changes, coordinate lanes, and keep every workspace in sync."
        actions={
          <>
            <button type="button" class="ghost" onClick={() => setReloadToken((n) => n + 1)}>
              Refresh
            </button>
            <button
              type="button"
              onClick={() => setCreating(true)}
              aria-expanded={creating()}
            >
              Create project <span aria-hidden="true">＋</span>
            </button>
          </>
        }
      />

      <div class="toolbar">
        <input
          type="search"
          placeholder="Filter by organization…"
          autocomplete="off"
          value={orgFilter()}
          onInput={(e) => setOrgFilter(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') setReloadToken((n) => n + 1);
          }}
        />
      </div>

      <Show when={creating()}>
        <div class="dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setCreating(false);
        }}>
          <form class="form-card project-dialog" role="dialog" aria-modal="true" aria-labelledby="create-project-title" onSubmit={onCreate}>
            <div class="dialog-heading">
              <div>
                <span class="eyebrow">01 / New workspace</span>
                <h2 id="create-project-title">Create a project</h2>
                <p class="muted">A shared home for reviews, repositories, and local workspaces.</p>
              </div>
              <button class="dialog-close" type="button" aria-label="Close dialog" onClick={() => setCreating(false)}>×</button>
            </div>
            <div class="form-grid two">
            <label>
              <span>Organization</span>
              <input name="organizationId" required placeholder="Your team" autocomplete="off" />
            </label>
            <label>
              <span>Project name</span>
              <input name="name" required placeholder="acme-platform" autocomplete="off" />
            </label>
            </div>
            <label>
              <span>Description <i>optional</i></span>
              <textarea name="description" rows={3} placeholder="What is this project for?" />
            </label>
            <div class="dialog-actions">
              <button type="button" class="ghost" onClick={() => setCreating(false)}>Cancel</button>
              <button type="submit">Create and continue <span aria-hidden="true">→</span></button>
            </div>
            <FormStatus message={status()} error={statusError()} />
          </form>
        </div>
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
                  body="Projects connect local workspaces without moving development into the cloud."
                  action={
                    <button type="button" onClick={() => setCreating(true)}>
                      Create your first project
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
