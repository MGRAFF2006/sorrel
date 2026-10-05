import { createSignal } from 'solid-js';

/** Keep the failed operation for an explicit retry, without losing form drafts. */
export function createAction() {
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  let retryOperation: (() => Promise<void>) | undefined;
  async function run(operation: () => Promise<void>) {
    if (pending()) return;
    retryOperation = operation;
    setPending(true);
    setError('');
    try {
      await operation();
      retryOperation = undefined;
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  }
  return { pending, error, run, retry: () => retryOperation && run(retryOperation) };
}
