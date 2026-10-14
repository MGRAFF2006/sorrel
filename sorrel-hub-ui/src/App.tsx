import { A, Route, Router, useParams, type RouteSectionProps } from '@solidjs/router';
import { createEffect, createMemo, createResource, For, onCleanup, onMount, Show, type ParentProps } from 'solid-js';
import {
  apiGet,
  apiPost,
  getApiBaseUrl,
  fetchCapabilities,
  fetchSession,
  setPrincipalProvider,
  unwrapList,
  useApiConnection,
  useAuthenticationRequired,
  type HubCapabilities,
  type HubSessionInfo,
} from './api.ts';
import { Icon } from './components/Icon.tsx';
import { ProjectSwitcher } from './components/ProjectSwitcher.tsx';
import { ErrorText, Loading } from './components/ui.tsx';
import { useOpenProposalsCountFromHub } from './convex/openProposals.ts';
import type { Project, Proposal } from './domain.ts';
import { initials } from './domain.ts';
import type { Platform } from './platform.ts';
import { DEV_IDENTITY_PRESETS, setActingPrincipal, setSessionPrincipal, useEffectivePrincipal } from './session.ts';
import { InboxView } from './views/InboxView.tsx';
import { OrganizationsView } from './views/OrganizationsView.tsx';
import { ProfileView } from './views/ProfileView.tsx';
import { ProjectOverview } from './views/ProjectOverview.tsx';
import { ProjectsHome } from './views/ProjectsView.tsx';
import { ReviewsView } from './views/ReviewsView.tsx';
import { SyncView } from './views/SyncView.tsx';
import { WorkView } from './views/WorkView.tsx';
import { createAction } from './action.ts';

export type HubAppOptions = {
  platform: Platform;
  base?: string;
};

function IdentityControl(props: {
  capabilities: HubCapabilities | null | undefined;
  hubSession: HubSessionInfo | null | undefined;
}) {
  const principal = useEffectivePrincipal();
  const mode = () => props.capabilities?.auth.mode ?? props.hubSession?.auth.mode ?? 'dev';
  const value = () => `${principal().type}:${principal().id}`;
  const signOut = createAction();

  return (
    <Show
      when={mode() === 'dev'}
      fallback={<>
        <A href="/profile" class="avatar-link" title={value()}>{initials(principal().id)}</A>
        <Show when={props.capabilities?.auth.browserLogin}>
          <button type="button" class="ghost" disabled={signOut.pending()} onClick={() => void signOut.run(async () => {
            const payload = await apiPost('/auth/logout', {}) as { data: { logoutUrl: string } };
            window.location.assign(payload.data.logoutUrl);
          })}>{signOut.pending() ? 'Signing out…' : 'Sign out'}</button>
          <Show when={signOut.error()}><span role="alert">{signOut.error()}</span></Show>
        </Show>
      </>}
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
          <ProjectSwitcher />
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
  const [capabilities, { refetch: refetchCapabilities }] = createResource(fetchCapabilities);
  const [hubSession, { refetch: refetchSession }] = createResource(fetchSession);
  const apiOk = useApiConnection();
  const effectivePrincipal = useEffectivePrincipal();
  const authenticationRequired = useAuthenticationRequired();
  const authorized = () => Boolean(capabilities() && hubSession()
    && (capabilities()?.auth.mode === 'dev' || (hubSession()?.session && !authenticationRequired())));

  createEffect(() => {
    const resolved = hubSession();
    setSessionPrincipal(resolved?.auth.mode !== 'dev' ? resolved?.session?.principal ?? null : null);
  });
  onCleanup(() => {
    setSessionPrincipal(null);
    setPrincipalProvider(null);
  });
  onMount(() => setPrincipalProvider(effectivePrincipal));

  const openCount = useOpenProposalsCountFromHub(authorized);

  onMount(() => {
    void (async () => {
      try {
        await apiGet('/healthz');
      } catch {
        // apiRequest records the failed connection for the shell.
      }
    })();
  });

  return (
    <Show when={!capabilities.loading && !hubSession.loading} fallback={<main class="auth-stage"><Loading text="Connecting to Sorrel Hub…" /></main>}>
    <Show when={capabilities() && hubSession()} fallback={
      <main class="auth-stage"><section class="auth-panel">
        <img src="/favicon.svg" alt="" /><h1>Hub could not be reached</h1>
        <p>Your projects will load when the connection is restored.</p>
        <button type="button" onClick={() => { void refetchCapabilities(); void refetchSession(); }}>Retry connection</button>
      </section></main>
    }>
    <Show when={authorized()} fallback={
      <main class="auth-stage"><section class="auth-panel">
        <img src="/favicon.svg" alt="" /><h1>Sign in to Sorrel Hub</h1>
        <p>Review agent work and collaborate with your team.</p>
        <Show when={new URLSearchParams(window.location.search).has('auth')}>
          <p role="status">{new URLSearchParams(window.location.search).get('auth') === 'logout-incomplete'
            ? 'You are signed out of this Hub. Identity provider sign-out could not complete.'
            : 'Sign-in was not completed. You can try again.'}</p>
        </Show>
        <Show when={capabilities()?.auth.browserLogin && props.platform.kind === 'web'} fallback={
          <p>Browser sign-in is not configured for this connection. Contact your Hub operator.</p>
        }>
          <a class="button primary" href={`${getApiBaseUrl()}/auth/login`}>Sign in</a>
        </Show>
        <p class="muted">Access is by invitation. Signing in gives you access only to projects your team has shared with you.</p>
      </section></main>
    }>
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
          <Route path="/" component={() => <ProjectsHome inviteOnly={capabilities()?.deploy === 'saas'} />} />
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
    </Show>
    </Show>
    </Show>
  );
}
