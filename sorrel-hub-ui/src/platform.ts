/**
 * Platform seams shared by browser, Tauri desktop, and Tauri mobile hosts.
 * Shells inject chrome/OS capabilities; product UI stays in this package.
 */

export type PlatformKind = 'web' | 'desktop' | 'mobile';

export type PlatformCapabilities = {
  /** Local `.sorrel` workspace via Core embed (desktop first). */
  localCore: boolean;
  /** OS notifications. */
  notifications: boolean;
  /** Deep-link / custom URL scheme handling. */
  deepLinks: boolean;
  /** Biometric unlock (mobile). */
  biometrics: boolean;
  /** System keychain / secure storage. */
  keychain: boolean;
};

export type Platform = {
  kind: PlatformKind;
  label: string;
  capabilities: PlatformCapabilities;
  /** Open a URL in the system browser / external handler. */
  openExternal(url: string): Promise<void>;
  /** Best-effort notification; no-op when unsupported. */
  notify(title: string, body: string): Promise<void>;
};

const WEB_CAPABILITIES: PlatformCapabilities = {
  localCore: false,
  notifications: typeof Notification !== 'undefined',
  deepLinks: false,
  biometrics: false,
  keychain: false,
};

const DESKTOP_CAPABILITIES: PlatformCapabilities = {
  localCore: false,
  notifications: true,
  deepLinks: false,
  biometrics: false,
  keychain: false,
};

const MOBILE_STUB_CAPABILITIES: PlatformCapabilities = {
  localCore: false,
  notifications: true,
  deepLinks: true,
  biometrics: true,
  keychain: true,
};

export function createWebPlatform(): Platform {
  return {
    kind: 'web',
    label: 'Web',
    capabilities: WEB_CAPABILITIES,
    async openExternal(url) {
      window.open(url, '_blank', 'noopener,noreferrer');
    },
    async notify(title, body) {
      if (typeof Notification === 'undefined') return;
      if (Notification.permission === 'granted') {
        new Notification(title, { body });
        return;
      }
      if (Notification.permission !== 'denied') {
        const permission = await Notification.requestPermission();
        if (permission === 'granted') new Notification(title, { body });
      }
    },
  };
}

export type DesktopPlatformOptions = {
  openExternal(url: string): Promise<void>;
  notify(title: string, body: string): Promise<void>;
};

/** Native adapters are supplied by the Tauri host without forking product UI. */
export function createDesktopPlatform(options: DesktopPlatformOptions): Platform {
  return {
    kind: 'desktop',
    label: 'Desktop',
    capabilities: DESKTOP_CAPABILITIES,
    openExternal: options.openExternal,
    notify: options.notify,
  };
}

/** Browser-safe fallback used by shared UI previews and tests. */
export function createDesktopPlatformStub(): Platform {
  return {
    kind: 'desktop',
    label: 'Desktop (stub)',
    capabilities: DESKTOP_CAPABILITIES,
    async openExternal(url) {
      window.open(url, '_blank', 'noopener,noreferrer');
    },
    async notify(title, body) {
      console.info(`[desktop-stub notify] ${title}: ${body}`);
    },
  };
}

/** Tauri mobile host will replace stubs with adaptive chrome + biometrics. */
export function createMobilePlatformStub(): Platform {
  return {
    kind: 'mobile',
    label: 'Mobile (stub)',
    capabilities: MOBILE_STUB_CAPABILITIES,
    async openExternal(url) {
      window.open(url, '_blank', 'noopener,noreferrer');
    },
    async notify(title, body) {
      console.info(`[mobile-stub notify] ${title}: ${body}`);
    },
  };
}

export function resolvePlatform(kind: PlatformKind = 'web'): Platform {
  if (kind === 'desktop') return createDesktopPlatformStub();
  if (kind === 'mobile') return createMobilePlatformStub();
  return createWebPlatform();
}
