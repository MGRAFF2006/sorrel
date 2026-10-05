import { cleanup } from '@solidjs/testing-library';
import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';

window.scrollTo = vi.fn();
HTMLDialogElement.prototype.showModal = function () { this.open = true; this.querySelector<HTMLElement>('input, button')?.focus(); };
HTMLDialogElement.prototype.close = function () { this.open = false; };

afterEach(() => {
  cleanup();
  localStorage.clear();
  history.replaceState(null, '', '/');
});
