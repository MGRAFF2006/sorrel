import { ConvexClient } from 'convex/browser';
import { anyApi } from 'convex/server';
import { createEffect, createSignal, onCleanup, type Accessor } from 'solid-js';
import { apiGet, unwrapList } from '../api.ts';

/**
 * Live open-proposals counter via Convex subscription.
 * Returns undefined while Convex is disabled or unreachable.
 */
export function useOpenProposalsCount(convexUrl: Accessor<string | null | undefined>): Accessor<
  number | undefined
> {
  const [count, setCount] = createSignal<number | undefined>(undefined);

  createEffect(() => {
    const url = convexUrl();
    setCount(undefined);
    if (!url) return;

    let client: ConvexClient;
    try {
      client = new ConvexClient(url);
    } catch (error) {
      console.warn('Convex client init failed', error);
      return;
    }

    const unsubscribe = client.onUpdate(anyApi.proposals.countOpen, {}, (value) => {
      setCount(typeof value === 'number' ? value : undefined);
    });

    onCleanup(() => {
      unsubscribe();
      void client.close();
    });
  });

  return count;
}

/** Hub API fallback poller used when Convex is not configured. */
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
