import { fetch as tauriFetch } from '@tauri-apps/plugin-http';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { openUrl } from '@tauri-apps/plugin-opener';
import { createDesktopPlatform, mountHubApp } from 'sorrel-hub-ui';

const root = document.getElementById('root');
if (!root) {
  throw new Error('#root missing');
}

const platform = createDesktopPlatform({
  openExternal: openUrl,
  async notify(title, body) {
    let allowed = await isPermissionGranted();
    if (!allowed) {
      allowed = (await requestPermission()) === 'granted';
    }
    if (allowed) sendNotification({ title, body });
  },
});

mountHubApp(root, {
  platform,
  apiBase: 'http://127.0.0.1:3000',
  fetch: tauriFetch,
  convexUrl: import.meta.env.VITE_CONVEX_URL as string | undefined,
});
