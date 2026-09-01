import { A, Route, Router, useParams, type RouteSectionProps } from '@solidjs/router';
import {
  createMemo,
  createResource,
  createSignal,
  For,
  onMount,
  Show,
  type ParentProps,
} from 'solid-js';
import {
  apiGet,
  fetchCapabilities,
  fetchSession,
  setPrincipalProvider,
  unwrapList,
  type HubCapabilities,
  type HubSessionInfo,
} from './api.ts';
import {
  useOpenProposalsCount,
  useOpenProposalsCountFromHub,
} from './convex/openProposals.ts';
import type { Platform } from './platform.ts';
import {
  DEV_IDENTITY_PRESETS,
  setActingPrincipal,
  useActingPrincipal,
} from './session.ts';
import { ErrorText, Loading } from './components/ui.tsx';
import { ProjectsHome } from './views/ProjectsView.tsx';
import { ProjectOverview } from './views/ProjectOverview.tsx';
import { ReviewsView } from './views/ReviewsView.tsx';
import { SyncView } from './views/SyncView.tsx';

export type HubAppOptions = {
  platform: Platform;
  convexUrl?: string;
  base?: string;
};

type Project = {
  id?: string;
  name?: string;
  organizationId?: string;
  description?: string;
  status?: string;
};

function IdentityChip(props: {
  capabilities: HubCapabilities | null | undefined;
  hubSession: HubSessionInfo | null | undefined;
}) {
  const principal = useActingPrincipal();
  const mode = () => props.capabilities?.auth.mode ?? props.hubSession?.auth.mode ?? 'dev';
  const isDev = () => mode() === 'dev';
  const label = () => {
    const p = principal();
    return `${p.type}:${p.id}`;
  };

  return (
    <div class="identity-chip" title={`Auth mode: ${mode()}`}>
      <span class="identity-mode">{mode()}</span>
      <Show when={isDev()} fallback={<span class="identity-principal">{label()}</span>}>
        <label class="identity-select-wrap">
          <span class="sr-only">Acting principal</span>
          <select
            class="identity-select"
            value={label()}
            onChange={(event) => {
              const [type, ...rest] = event.currentTarget.value.split(':');
              const id = rest.join(':');
              if (type && id) setActingPrincipal({ type, id });
            }}
          >
            <For each={DEV_IDENTITY_PRESETS}>
              {(preset) => (
                <option value={`${preset.type}:${preset.id}`}>
                  {preset.type}:{preset.id}
                </option>
              )}
            </For>
          </select>
        </label>
      </Show>
      <Show when={!isDev() && props.hubSession?.session == null}>
        <span class="identity-hint">sign in via IdP</span>
      </Show>
    </div>
  );
}

function GlobalShell(
  props: ParentProps<{
    platform: Platform;
    capabilities: HubCapabilities | null | undefined;
    hubSession: HubSessionInfo | null | undefined;
    openCount: number | undefined;
    apiStatus: string;
    apiOk: boolean | null;
  }>,
) {
  return (
    <div class="app-shell">
      {props.children}
      <header class="app-topbar">
        <div class="topbar-context">
          <span class="topbar-product">Workspace</span>
          <span class="topbar-separator" aria-hidden="true">/</span>
          <span class="topbar-view">Collaboration hub</span>
        </div>
        <div class="header-status">
          <IdentityChip capabilities={props.capabilities} hubSession={props.hubSession} />
          <Show when={props.openCount !== undefined}>
            <span class="review-badge" title="Open reviews">
              {props.openCount} review{props.openCount === 1 ? '' : 's'}
            </span>
          </Show>
          <span
            class={`status ${
              props.apiOk === true
                ? 'status-ok'
                : props.apiOk === false
                  ? 'status-down'
                  : 'status-unknown'
            }`}
          >
            <span class="status-dot" aria-hidden="true" />
            <span class="status-label">{props.apiStatus.replace('API: ', '')}</span>
          </span>
        </div>
      </header>
    </div>
  );
}

function HomeLayout(
  props: RouteSectionProps & {
    platform: Platform;
    capabilities: HubCapabilities | null | undefined;
  },
) {
  return (
    <>
      <aside class="app-sidebar">
        <A href="/" class="brand brand-link">
          <span class="brand-symbol" aria-hidden="true">S</span>
          <span><span class="brand-mark">Sorrel</span><span class="brand-sub">Hub</span></span>
        </A>
        <p class="nav-eyebrow">Workspace</p>
        <nav class="side-nav" aria-label="Primary">
          <A href="/" class="nav-item" end activeClass="active">
            <span class="nav-index" aria-hidden="true">01</span>Projects
          </A>
        </nav>
        <div class="sidebar-callout">
          <span class="callout-kicker">Local-first</span>
          <strong>Your code stays yours.</strong>
          <p>Hub coordinates reviews and sync. It does not host your development environment.</p>
        </div>
        <div class="side-meta">
          <CapabilitiesHint capabilities={props.capabilities} />
          <p>{props.platform.label}</p>
        </div>
      </aside>
      <main class="app-main">{props.children}</main>
    </>
  );
}

function ProjectLayout(
  props: RouteSectionProps & {
    platform: Platform;
    capabilities: HubCapabilities | null | undefined;
  },
) {
  const params = useParams<{ projectId: string }>();
  const projectId = () => decodeURIComponent(params.projectId);

  const [project] = createResource(projectId, async (id) => {
    const payload = (await apiGet(`/projects/${encodeURIComponent(id)}`)) as {
      data?: Project;
    };
    return (payload.data ?? payload) as Project;
  });

  const base = () => `/projects/${encodeURIComponent(projectId())}`;

  return (
    <>
      <aside class="app-sidebar">
        <A href="/" class="brand brand-link">
          <span class="brand-symbol" aria-hidden="true">S</span>
          <span><span class="brand-mark">Sorrel</span><span class="brand-sub">Hub</span></span>
        </A>

        <div class="project-switcher">
          <A href="/" class="project-back">
            All projects
          </A>
          <Show when={!project.loading} fallback={<p class="muted project-name">Loading…</p>}>
            <Show
              when={!project.error && project()}
              fallback={<p class="error project-name">Project unavailable</p>}
            >
              <p class="project-name" title={project()?.id}>
                {project()?.name ?? projectId()}
              </p>
              <p class="project-org muted">{project()?.organizationId}</p>
            </Show>
          </Show>
        </div>

        <nav class="side-nav" aria-label="Project">
          <A href={base()} class="nav-item" end activeClass="active">
            <span class="nav-index" aria-hidden="true">01</span>Overview
          </A>
          <A href={`${base()}/reviews`} class="nav-item" activeClass="active">
            <span class="nav-index" aria-hidden="true">02</span>Reviews
          </A>
          <A href={`${base()}/sync`} class="nav-item" activeClass="active">
            <span class="nav-index" aria-hidden="true">03</span>Repositories
          </A>
        </nav>

        <div class="side-meta">
          <CapabilitiesHint capabilities={props.capabilities} />
          <p>{props.platform.label}</p>
        </div>
      </aside>

      <main class="app-main">
        <Show when={project.error}>
          <ErrorText
            text={`Could not load project: ${project.error instanceof Error ? project.error.message : String(project.error)}`}
          />
        </Show>
        <Show when={project.loading}>
          <Loading text="Loading project…" />
        </Show>
        <Show when={project() && !project.error}>{props.children}</Show>
      </main>
    </>
  );
}

export function CapabilitiesHint(props: { capabilities: HubCapabilities | null | undefined }) {
  const caps = () => props.capabilities;
  return (
    <Show when={caps()}>
      {(c) => (
        <p class="deployment-label">
          <span class="deployment-dot" aria-hidden="true" />
          {c().deploy === 'saas' ? 'Hosted Hub' : 'Local development'}
        </p>
      )}
    </Show>
  );
}

export function HubApp(props: HubAppOptions) {
  const [capabilities] = createResource(fetchCapabilities);
  const [hubSession, { refetch: refetchSession }] = createResource(fetchSession);
  const [apiStatus, setApiStatus] = createSignal('API: checking…');
  const [apiOk, setApiOk] = createSignal<boolean | null>(null);
  const actingPrincipal = useActingPrincipal();

  onMount(() => {
    setPrincipalProvider(() => actingPrincipal());
  });

  const convexUrl = createMemo(() => {
    if (props.convexUrl) return props.convexUrl;
    const fromCaps = capabilities()?.convex;
    if (fromCaps?.enabled === false) return null;
    return fromCaps?.url ?? (import.meta.env.VITE_CONVEX_URL as string | undefined) ?? null;
  });

  const convexCount = useOpenProposalsCount(convexUrl);
  const hubFallbackCount = useOpenProposalsCountFromHub(
    () => !convexUrl() && apiOk() === true,
  );
  const openCount = createMemo(() => convexCount() ?? hubFallbackCount());
  onMount(() => {
    void (async () => {
      try {
        const health = (await apiGet('/healthz')) as { status?: string };
        setApiStatus(`API: ${health.status ?? 'ok'}`);
        setApiOk(true);
        void refetchSession();
      } catch {
        setApiStatus('API: unreachable');
        setApiOk(false);
      }
    })();
  });

  const shell = (children: ParentProps['children']) => (
    <GlobalShell
      platform={props.platform}
      capabilities={capabilities()}
      hubSession={hubSession()}
      openCount={openCount()}
      apiStatus={apiStatus()}
      apiOk={apiOk()}
    >
      {children}
    </GlobalShell>
  );

  return (
    <Router base={props.base}>
      <Route
        path="/"
        component={(routeProps) =>
          shell(
            <HomeLayout
              {...routeProps}
              platform={props.platform}
              capabilities={capabilities()}
            />,
          )
        }
      >
        <Route path="/" component={ProjectsHome} />
      </Route>

      <Route
        path="/projects/:projectId"
        component={(routeProps) =>
          shell(
            <ProjectLayout
              {...routeProps}
              platform={props.platform}
              capabilities={capabilities()}
            />,
          )
        }
      >
        <Route path="/" component={ProjectOverview} />
        <Route path="/reviews" component={() => <ReviewsView />} />
        <Route path="/sync" component={() => <SyncView />} />
      </Route>
    </Router>
  );
}
