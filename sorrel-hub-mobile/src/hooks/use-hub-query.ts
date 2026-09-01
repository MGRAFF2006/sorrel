import { useFocusEffect } from 'expo-router';
import { useCallback, useRef, useState } from 'react';

export function useHubQuery<T>(load: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const generation = useRef(0);

  const run = useCallback(
    async (refresh = false) => {
      const current = ++generation.current;
      setError(null);
      if (refresh) {
        setRefreshing(true);
      } else {
        setLoading(true);
      }
      try {
        const next = await load();
        if (current === generation.current) setData(next);
      } catch (caught) {
        if (current === generation.current) setError(caught);
      } finally {
        if (current === generation.current) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [load],
  );

  useFocusEffect(
    useCallback(() => {
      void run(false);
      return () => {
        generation.current += 1;
      };
    }, [run]),
  );

  return {
    data,
    error,
    loading,
    refreshing,
    reload: () => run(true),
  };
}
