import { A, useParams } from '@solidjs/router';
import { createMemo, createResource, For, Show } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';
import { Icon } from '../components/Icon.tsx';
import { MarkdownDocument } from '../components/MarkdownDocument.tsx';
import { EmptyState, ErrorText, Loading, StatusPill } from '../components/ui.tsx';
import type { Organization, Project } from '../domain.ts';
import { initials, metadataString } from '../domain.ts';

export function OrganizationsView() {
  const params = useParams<{ orgId?: string }>();
  const routeOrgId = () => params.orgId ? decodeURIComponent(params.orgId) : undefined;
  const [data] = createResource(async () => {
    const [organizationsPayload, projectsPayload] = await Promise.all([
      apiGet('/admin/organizations'),
      apiGet('/projects'),
    ]);
    return {
      organizations: unwrapList(organizationsPayload) as Organization[],
      projects: unwrapList(projectsPayload) as Project[],
    };
  });
  const organizations = createMemo(() => {
    const value = data();
    if (!value) return [] as Organization[];
    const result = [...value.organizations];
    const known = new Set(result.flatMap((item) => [item.id, item.slug, item.name]).filter(Boolean));
    for (const project of value.projects) {
      const id = project.organizationId;
      if (!id || known.has(id)) continue;
      result.push({ id, name: id, slug: id, principalRefs: [], metadata: {} });
      known.add(id);
    }
    return result;
  });
  const organization = createMemo(() => {
    const id = routeOrgId();
    if (!id) return undefined;
    return organizations().find((item) => item.id === id || item.slug === id || item.name === id);
  });
  const projects = createMemo(() => {
    const current = organization();
    if (!current) return [];
    return (data()?.projects ?? []).filter((project) => project.organizationId === current.id || project.organizationId === current.slug || project.organizationId === current.name);
  });

  return (
    <Show when={!data.loading} fallback={<Loading text="Loading organizations…" />}>
      <Show when={!data.error} fallback={<ErrorText text="Organizations could not be loaded." />}>
        <Show
          when={routeOrgId()}
          fallback={
            <div class="directory-page view-enter">
              <header class="workspace-heading"><div><p class="section-label">Shared identity</p><h1>Organizations</h1><p>Projects, members, and the story behind the work.</p></div></header>
              <Show when={organizations().length > 0} fallback={<EmptyState title="No organizations yet" body="Organizations created through the Hub API will appear here." />}>
                <div class="organization-grid">
                  <For each={organizations()}>{(org) => (
                    <A href={`/orgs/${encodeURIComponent(org.id ?? org.slug ?? '')}`} class="organization-card surface">
                      <span class="identity-mark small">{initials(org.name)}</span>
                      <div><h2>{org.name ?? org.id}</h2><p>{metadataString(org.metadata, 'description') ?? `${org.principalRefs?.length ?? 0} members · Sorrel organization`}</p></div>
                      <Icon name="chevron" />
                    </A>
                  )}</For>
                </div>
              </Show>
            </div>
          }
        >
          <Show when={organization()} fallback={<div class="directory-page"><ErrorText text={`Organization ${routeOrgId()} was not found.`} /></div>}>
            {(org) => (
              <div class="identity-page view-enter">
                <header class="identity-header">
                  <span class="identity-mark">{initials(org().name)}</span>
                  <div>
                    <p class="section-label">Organization</p>
                    <h1>{org().name ?? org().id}</h1>
                    <p>{metadataString(org().metadata, 'description') ?? 'A Sorrel organization for shared projects and collaboration.'}</p>
                    <div class="identity-meta"><span><Icon name="users" />{org().principalRefs?.length ?? 0} members</span><Show when={metadataString(org().metadata, 'location')}><span>{metadataString(org().metadata, 'location')}</span></Show></div>
                  </div>
                </header>
                <nav class="identity-tabs"><span class="active">Overview</span><span>Projects <b>{projects().length}</b></span><span>Members <b>{org().principalRefs?.length ?? 0}</b></span></nav>
                <div class="identity-layout">
                  <article class="identity-readme surface">
                    <header><Icon name="book" /><strong>{org().slug ?? org().name} / README.md</strong></header>
                    <Show
                      when={metadataString(org().metadata, 'readme')}
                      fallback={<EmptyState title="No organization README yet" body="Store a Markdown README in organization metadata to explain what this organization builds and how it works." />}
                    >
                      {(readme) => <MarkdownDocument source={readme()} />}
                    </Show>
                  </article>
                  <aside class="identity-projects surface">
                    <header><Icon name="project" /><strong>Projects</strong></header>
                    <Show when={projects().length > 0} fallback={<EmptyState title="No projects" />}>
                      <For each={projects()}>{(project) => (
                        <A href={`/projects/${encodeURIComponent(project.id ?? '')}`}>
                          <div><strong>{project.name ?? project.id}</strong><p>{project.description ?? 'Sorrel project'}</p></div><StatusPill value={project.status} />
                        </A>
                      )}</For>
                    </Show>
                  </aside>
                </div>
              </div>
            )}
          </Show>
        </Show>
      </Show>
    </Show>
  );
}
