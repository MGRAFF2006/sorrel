import { mountHubApp } from 'sorrel-hub-ui';

const root = document.getElementById('root');
if (!root) {
  throw new Error('#root missing');
}

mountHubApp(root, {
  platformKind: 'web',
  convexUrl: import.meta.env.VITE_CONVEX_URL as string | undefined,
});
