import { cleanup } from '@solidjs/testing-library';
import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';

window.scrollTo = vi.fn();

afterEach(() => {
  cleanup();
  localStorage.clear();
  history.replaceState(null, '', '/');
});
