import { A, Route, Router, useParams, type RouteSectionProps } from '@solidjs/router';
import { createMemo, createResource, createSignal, For, onMount, Show, type ParentProps } from 'solid-js';
import {
  apiGet,
  fetchCapabilities,
  fetchSession,
  setPrincipalProvider,
  unwrapList,
  type HubCapabilities,
  type HubSessionInfo,
} from './api.ts';
import { Icon } from './components/Icon.tsx';
import { ErrorText, Loading } from './components/ui.tsx';
import { useOpenProposalsCount, useOpenProposalsCountFromHub } from './convex/openProposals.ts';
import type { Project, Proposal } from './domain.ts';
import { initials } from './domain.ts';
import type { Platform } from './platform.ts';
import { DEV_IDENTITY_PRESETS, setActingPrincipal, useActingPrincipal } from './session.ts';
import { InboxView } from './views/InboxView.tsx';
import { OrganizationsView } from './views/OrganizationsView.tsx';
import { ProfileView } from './views/ProfileView.tsx';
import { ProjectOverview } from './views/ProjectOverview.tsx';
import { ProjectsHome } from './views/ProjectsView.tsx';
import { ReviewsView } from './views/ReviewsView.tsx';
import { SyncView } from './views/SyncView.tsx';
import { WorkView } from './views/WorkView.tsx';

export type HubAppOptions = {
  platform: Platform;
  convexUrl?: string;
  base?: string;
};

function IdentityControl(props: {
  capabilities: HubCapabilities | null | undefined;
  hubSession: HubSessionInfo | null | undefined;
}) {
  const principal = useActingPrincipal();
  const mode = () => props.capabilities?.auth.mode ?? props.hubSession?.auth.mode ?? 'dev';
  const value = () => `${principal().type}:${principal().id}`;

  return (
    <Show
      when={mode() === 'dev'}
      fallback={<A href="/profile" class="avatar-link" title={value()}>{initials(principal().id)}</A>}
    >
      <label class="identity-control" title="Development acting principal">
        <span class="sr-only">Acting principal</span>
        <select
          value={value()}
          onChange={(event) => {
            const [type, ...id] = event.currentTarget.value.split(':');
            if (type && id.length) setActingPrincipal({ type, id: id.join(':') });
          }}
        >
          <For each={DEV_IDENTITY_PRESETS}>
            {(preset) => <option value={`${preset.type}:${preset.id}`}>{preset.id}</option>}
          </For>
        </select>
      </label>
    </Show>
  );
}

function GlobalShell(
  props: RouteSectionProps & {
    capabilities: HubCapabilities | null | undefined;
    hubSession: HubSessionInfo | null | undefined;
    openCount: number | undefined;
    apiOk: boolean | null;
  },
) {
  return (
    <div class="hub-shell">
      <header class="global-bar">
        <A href="/" class="global-brand" aria-label="Sorrel Hub projects">
          <img src="/favicon.svg" alt="" />
          <strong>Sorrel</strong>
          <span>Hub</span>
        </A>
        <nav class="global-nav" aria-label="Global">
          <A href="/" end activeClass="active"><Icon name="project" />Projects</A>
          <A href="/inbox" activeClass="active">
            <Icon name="inbox" />Inbox
            <Show when={props.openCount !== undefined && props.openCount > 0}>
              <span class="nav-count">{props.openCount}</span>
            </Show>
          </A>
          <A href="/orgs" activeClass="active"><Icon name="org" />Organizations</A>
        </nav>
        <div class="global-tools">
          <span class={`api-indicator ${props.apiOk === true ? 'ok' : props.apiOk === false ? 'down' : ''}`}>
            <span />Hub
          </span>
          <IdentityControl capabilities={props.capabilities} hubSession={props.hubSession} />
          <A href="/profile" class="icon-button" aria-label="Open profile"><Icon name="user" /></A>
        </div>
      </header>
      {props.children}
    </div>
  );
}

function GlobalPage(props: ParentProps) {
  return <main class="global-stage">{props.children}</main>;
}

function ProjectLayout(props: RouteSectionProps) {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);
  const base = () => `/projects/${encodeURIComponent(projectId())}`;

  const [project] = createResource(projectId, async (id) => {
    const payload = (await apiGet(`/projects/${encodeURIComponent(id)}`)) as { data?: Project };
    return (payload.data ?? payload) as Project;
  });
  const [proposals] = createResource(projectId, async (id) => {
    const payload = await apiGet(`/admin/proposals?projectId=${encodeURIComponent(id)}`);
    return unwrapList(payload) as Proposal[];
  });
  const reviewCount = () => (proposals() ?? []).filter((item) => ['draft', 'open', 'approved', 'rejected'].includes(item.status ?? '')).length;

  return (
    <Show when={!project.loading} fallback={<main class="global-stage"><Loading text="Loading project…" /></main>}>
      <Show
        when={!project.error && project()}
        fallback={<main class="global-stage"><ErrorText text="This project could not be loaded." /></main>}
      >
        {(current) => (
          <>
            <header class="project-chrome">
              <div class="project-heading">
                <A href={`/orgs/${encodeURIComponent(current().organizationId ?? '')}`} class="project-owner-mark">
                  {initials(current().organizationId)}
                </A>
                <div class="project-title">
                  <p>{current().organizationId ?? 'Personal'} /</p>
                  <h1>{current().name ?? current().id}</h1>
                  <span>{current().description ?? 'A Sorrel collaboration project.'}</span>
                </div>
                <span class={`project-state ${current().status === 'archived' ? 'archived' : ''}`}>
                  {current().status ?? 'active'}
                </span>
                <div class="project-actions">
                  <A href={`${base()}/sync`} class="button secondary"><Icon name="sync" />Connect</A>
                  <A href={`${base()}/reviews`} class="button primary"><Icon name="branch" />Open review</A>
                </div>
              </div>
              <nav class="project-tabs" aria-label="Project">
                <A href={base()} end activeClass="active"><Icon name="code" />Code</A>
                <A href={`${base()}/work`} activeClass="active"><Icon name="layers" />Work</A>
                <A href={`${base()}/reviews`} activeClass="active">
                  <Icon name="branch" />Reviews
                  <Show when={reviewCount() > 0}><span class="tab-count">{reviewCount()}</span></Show>
                </A>
                <A href={`${base()}/sync`} activeClass="active"><Icon name="repo" />Repositories</A>
              </nav>
            </header>
            <main class="project-stage">{props.children}</main>
          </>
        )}
      </Show>
    </Show>
  );
}

export function HubApp(props: HubAppOptions) {
  const [capabilities] = createResource(fetchCapabilities);
  const [hubSession, { refetch: refetchSession }] = createResource(fetchSession);
  const [apiOk, setApiOk] = createSignal<boolean | null>(null);
  const actingPrincipal = useActingPrincipal();

  onMount(() => setPrincipalProvider(() => actingPrincipal()));

  const convexUrl = createMemo(() => {
    if (props.convexUrl) return props.convexUrl;
    const configured = capabilities()?.convex;
    if (configured?.enabled === false) return null;
    return configured?.url ?? (import.meta.env.VITE_CONVEX_URL as string | undefined) ?? null;
  });
  const convexCount = useOpenProposalsCount(convexUrl);
  const fallbackCount = useOpenProposalsCountFromHub(() => !convexUrl() && apiOk() === true);
  const openCount = createMemo(() => convexCount() ?? fallbackCount());

  onMount(() => {
    void (async () => {
      try {
        await apiGet('/healthz');
        setApiOk(true);
        void refetchSession();
      } catch {
        setApiOk(false);
      }
    })();
  });

  return (
    <Router base={props.base}>
      <Route
        path="/"
        component={(routeProps) => (
          <GlobalShell
            {...routeProps}
            capabilities={capabilities()}
            hubSession={hubSession()}
            openCount={openCount()}
            apiOk={apiOk()}
          />
        )}
      >
        <Route path="/" component={GlobalPage}>
          <Route path="/" component={ProjectsHome} />
          <Route path="/inbox" component={InboxView} />
          <Route path="/orgs" component={OrganizationsView} />
          <Route path="/orgs/:orgId" component={OrganizationsView} />
          <Route path="/profile" component={ProfileView} />
        </Route>
        <Route path="/projects/:projectId" component={ProjectLayout}>
          <Route path="/" component={ProjectOverview} />
          <Route path="/work" component={WorkView} />
          <Route path="/reviews" component={ReviewsView} />
          <Route path="/sync" component={SyncView} />
        </Route>
      </Route>
    </Router>
  );
}
