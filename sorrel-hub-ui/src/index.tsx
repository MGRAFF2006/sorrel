import { render } from 'solid-js/web';
import { HubApp } from './App.tsx';
import { configureApiClient, type ApiFetch } from './api.ts';
import { createWebPlatform, resolvePlatform, type Platform, type PlatformKind } from './platform.ts';
import './styles/hub.css';

export type { HubCapabilities } from './api.ts';
export type { HubAppOptions } from './App.tsx';
export type { Platform, PlatformKind } from './platform.ts';
export {
  createDesktopPlatform,
  createDesktopPlatformStub,
  createMobilePlatformStub,
  createWebPlatform,
  resolvePlatform,
} from './platform.ts';
export { HubApp } from './App.tsx';
export {
  LOCAL_PRINCIPAL,
  apiGet,
  apiPost,
  apiPatch,
  apiRequest,
  configureApiClient,
  unwrapList,
  fetchSession,
} from './api.ts';
export {
  getActingPrincipal,
  setActingPrincipal,
  useActingPrincipal,
  DEV_IDENTITY_PRESETS,
} from './session.ts';

export type MountOptions = {
  /** Defaults to web platform. */
  platform?: Platform;
  platformKind?: PlatformKind;
  convexUrl?: string;
  base?: string;
  /** Defaults to the browser host's same-origin `/api` proxy. */
  apiBase?: string;
  /** Native hosts can inject a transport such as Tauri's scoped HTTP plugin. */
  fetch?: ApiFetch;
};

/**
 * Mount the shared Hub product UI into a host element.
 * Used by `sorrel-hub-web` and future Tauri shells.
 */
export function mountHubApp(element: HTMLElement, options: MountOptions = {}): () => void {
  configureApiClient({ baseUrl: options.apiBase, fetch: options.fetch });
  const platform =
    options.platform ??
    (options.platformKind ? resolvePlatform(options.platformKind) : createWebPlatform());

  return render(
    () => (
      <HubApp platform={platform} convexUrl={options.convexUrl} base={options.base} />
    ),
    element,
  );
}
