import { render } from 'solid-js/web';
import { HubApp } from './App.tsx';
import { createWebPlatform, resolvePlatform, type Platform, type PlatformKind } from './platform.ts';
import './styles/hub.css';

export type { HubCapabilities } from './api.ts';
export type { HubAppOptions } from './App.tsx';
export type { Platform, PlatformKind } from './platform.ts';
export {
  createDesktopPlatformStub,
  createMobilePlatformStub,
  createWebPlatform,
  resolvePlatform,
} from './platform.ts';
export { HubApp } from './App.tsx';
export { LOCAL_PRINCIPAL, apiGet, apiPost, apiPatch, apiRequest, unwrapList, fetchSession } from './api.ts';
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
};

/**
 * Mount the shared Hub product UI into a host element.
 * Used by `sorrel-hub-web` and future Tauri shells.
 */
export function mountHubApp(element: HTMLElement, options: MountOptions = {}): () => void {
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
