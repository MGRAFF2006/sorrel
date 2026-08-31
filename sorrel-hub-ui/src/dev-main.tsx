import { mountHubApp } from './index.tsx';

const root = document.getElementById('root');
if (!root) {
  throw new Error('#root missing');
}

mountHubApp(root, { platformKind: 'web' });
