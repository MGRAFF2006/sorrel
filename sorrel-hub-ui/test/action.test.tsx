import { describe, expect, test } from 'vitest';
import { createAction } from '../src/action.ts';

describe('mutations', () => {
  test('rejects duplicate pending operations and retries failures', async () => {
    const action = createAction();
    let calls = 0;
    let finish!: () => void;
    const operation = async () => { calls++; await new Promise<void>(resolve => { finish = resolve; }); throw new Error('Policy denied'); };
    const first = action.run(operation);
    await action.run(operation);
    expect(calls).toBe(1);
    expect(action.pending()).toBe(true);
    finish(); await first;
    expect(action.error()).toBe('Policy denied');
    expect(action.pending()).toBe(false);
    const retry = action.retry();
    expect(calls).toBe(2);
    finish(); await retry;
    expect(action.error()).toBe('Policy denied');
  });
});
