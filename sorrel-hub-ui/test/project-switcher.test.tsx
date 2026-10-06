import { Route, Router } from '@solidjs/router';
import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ProjectSwitcher } from '../src/components/ProjectSwitcher.tsx';
import { configureApiClient, LOCAL_PRINCIPAL, setPrincipalProvider } from '../src/api.ts';
import { getEffectivePrincipal, setActingPrincipal, setSessionPrincipal } from '../src/session.ts';

const projects = [
  { id: 'project/a.b', name: 'Alpha', organizationId: 'Acme' },
  { id: 'project_beta', name: 'Beta', organizationId: 'Other' },
];
function response(data: unknown, status = 200) {
  return new Response(JSON.stringify({ data }), { status, headers: { 'content-type': 'application/json' } });
}
function mount() {
  return render(() => <Router root={props => <><ProjectSwitcher />{props.children}</>}>
    <Route path="*" component={() => <p>Current page</p>} />
  </Router>);
}
function shortcut(options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true, ...options });
  document.dispatchEvent(event);
  return event;
}

beforeEach(() => {
  setSessionPrincipal(null);
  setActingPrincipal(LOCAL_PRINCIPAL);
  configureApiClient({ baseUrl: '/api' });
  setPrincipalProvider(getEffectivePrincipal);
  vi.stubGlobal('fetch', vi.fn(async () => response(projects)));
  // Model native dialog autofocus; real browsers provide it in showModal().
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) {
    this.open = true;
    this.querySelector<HTMLElement>('[autofocus]')?.focus();
  });
});
afterEach(() => {
  configureApiClient();
  setPrincipalProvider(null);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  setSessionPrincipal(null);
  setActingPrincipal(LOCAL_PRINCIPAL);
});

describe('project switcher', () => {
  test('loads only on opening and searches names, ids, and organizations before navigating home', async () => {
    history.replaceState(null, '', '/projects/current/reviews');
    mount();
    expect(fetch).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project' }));
    const search = screen.getByRole('searchbox', { name: 'Find a project' });
    expect(search).toHaveFocus();
    await screen.findByRole('button', { name: 'Alpha · Acme' });
    for (const query of ['ALPHA', 'project/a.b', 'acme']) {
      await fireEvent.input(search, { target: { value: query } });
      expect(screen.getByRole('button', { name: 'Alpha · Acme' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Beta · Other' })).not.toBeInTheDocument();
    }
    await fireEvent.input(search, { target: { value: 'missing' } });
    expect(screen.getByText('No matching projects')).toBeInTheDocument();
    await fireEvent.input(search, { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Alpha · Acme' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() => expect(location.pathname).toBe('/projects/project%2Fa.b'));
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(vi.mocked(fetch).mock.calls[0][0]).toBe('/api/projects');
    expect(vi.mocked(fetch).mock.calls[0][1]?.method).toBe('GET');
  });

  test.each([{ ctrlKey: true, metaKey: false }, { ctrlKey: false, metaKey: true }])('opens with modifier shortcut %j and closes with native Escape cancellation', async modifiers => {
    mount();
    expect(shortcut(modifiers).defaultPrevented).toBe(true);
    const dialog = screen.getByRole('dialog', { name: 'Switch project' });
    await fireEvent(dialog, new Event('cancel', { cancelable: true }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Switch project' })).toHaveAttribute('aria-expanded', 'false');
  });

  test('preserves editable shortcuts and skips dialogs already open, including itself', async () => {
    mount();
    for (const tag of ['input', 'textarea', 'select']) {
      const editable = document.createElement(tag);
      document.body.append(editable);
      const event = new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true });
      editable.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
      editable.remove();
    }
    const editable = document.createElement('div');
    editable.setAttribute('contenteditable', 'true');
    editable.innerHTML = '<span>Editable content</span>';
    document.body.append(editable);
    editable.firstElementChild!.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }));
    editable.remove();
    const otherDialog = document.createElement('dialog');
    otherDialog.open = true;
    document.body.append(otherDialog);
    expect(shortcut().defaultPrevented).toBe(false);
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project', hidden: true }));
    expect(fetch).not.toHaveBeenCalled();
    otherDialog.remove();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(shortcut().defaultPrevented).toBe(true);
    await screen.findByRole('button', { name: 'Alpha · Acme' });
    expect(shortcut().defaultPrevented).toBe(false);
    expect(screen.getAllByRole('dialog')).toHaveLength(1);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  test('ignores unrelated keys and disposes its global listener', () => {
    const view = mount();
    for (const options of [{ ctrlKey: false }, { key: 'p' }, { altKey: true }, { shiftKey: true }, { repeat: true }, { isComposing: true }]) {
      expect(shortcut(options).defaultPrevented).toBe(false);
    }
    view.unmount();
    expect(shortcut().defaultPrevented).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('shows loading, supports retry after failure, and handles an empty authorized list', async () => {
    let finish!: (response: Response) => void;
    vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
    mount();
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project' }));
    expect(screen.getByRole('status')).toHaveTextContent('Loading projects…');
    finish(response({ message: 'Unavailable' }, 503));
    expect(await screen.findByRole('alert')).toHaveTextContent('Projects could not be loaded.');
    await fireEvent.click(screen.getByRole('button', { name: 'Retry projects' }));
    expect(screen.getByRole('status')).toHaveTextContent('Loading projects…');
    finish(response([]));
    expect(await screen.findByText('No projects available')).toBeInTheDocument();
  });

  test('reopening fetches current projects instead of retaining the previous list', async () => {
    mount();
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project' }));
    await screen.findByRole('button', { name: 'Alpha · Acme' });
    await fireEvent.click(screen.getByRole('button', { name: 'Close project switcher' }));
    vi.mocked(fetch).mockResolvedValue(response([]));
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project' }));
    await screen.findByText('No projects available');
    expect(screen.queryByRole('button', { name: 'Alpha · Acme' })).not.toBeInTheDocument();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  test('changes effective principals while open and ignores the old principal response', async () => {
    const pending: Array<(response: Response) => void> = [];
    configureApiClient({ baseUrl: '/private-hub', fetch: (input, init) => globalThis.fetch(input, {
      ...init, headers: { ...init?.headers, authorization: 'Bearer synthetic-fixture' },
    }) });
    vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(resolve => pending.push(resolve))));
    mount();
    await fireEvent.click(screen.getByRole('button', { name: 'Switch project' }));
    setSessionPrincipal({ type: 'user', id: 'signed-in-fixture' });
    await waitFor(() => expect(pending).toHaveLength(2));
    pending[1](response([{ id: 'allowed', name: 'Current user project' }]));
    await screen.findByRole('button', { name: 'Current user project · allowed' });
    pending[0](response(projects));
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(screen.queryByRole('button', { name: 'Alpha · Acme' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Current user project · allowed' })).toBeInTheDocument();
    for (const [url, init] of vi.mocked(fetch).mock.calls) {
      expect(url).toBe('/private-hub/projects');
      expect(init?.headers).toMatchObject({ authorization: 'Bearer synthetic-fixture' });
      expect(init?.method).toBe('GET');
    }
  });
});
