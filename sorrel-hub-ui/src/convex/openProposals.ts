import { createEffect, createSignal, onCleanup, type Accessor } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';

/** Count only proposals visible through the authenticated Hub transport. */
export function useOpenProposalsCountFromHub(
  enabled: Accessor<boolean>,
  pollMs = 5_000,
): Accessor<number | undefined> {
  const [count, setCount] = createSignal<number | undefined>(undefined);

  createEffect(() => {
    if (!enabled()) {
      setCount(undefined);
      return;
    }

    setCount(undefined);
    let cancelled = false;

    async function refresh() {
      try {
        const payload = await apiGet('/admin/proposals?status=open');
        if (cancelled) return;
        setCount(unwrapList(payload).length);
      } catch {
        if (!cancelled) setCount(undefined);
      }
    }

    void refresh();
    const timer = setInterval(() => void refresh(), pollMs);
    onCleanup(() => {
      cancelled = true;
      clearInterval(timer);
    });
  });

  return count;
}
