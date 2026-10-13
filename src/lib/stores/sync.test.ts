// 同步 store 的冲突三选一：默认不选择、显式点选、未选择拒绝提交。

import { beforeEach, describe, expect, it } from 'vitest';

import { sync } from './sync.svelte';

describe('sync store conflict selection', () => {
  beforeEach((): void => {
    sync.reset();
  });

  it('starts with no choice for any conflict', () => {
    expect(sync.choiceOf('conflict-1')).toBeNull();
  });

  it('records an explicit choice only when set', () => {
    sync.setChoice('conflict-1', 'keep_both');
    expect(sync.choiceOf('conflict-1')).toBe('keep_both');
    // 其他冲突仍保持未选择。
    expect(sync.choiceOf('conflict-2')).toBeNull();
  });

  it('rejects resolve() when no choice was made (no auto-selection)', async () => {
    const resolved = await sync.resolve('conflict-without-choice');
    expect(resolved).toBe(false);
  });

  it('reset() clears conflicts and choices', () => {
    sync.setChoice('conflict-1', 'keep_local');
    sync.reset();
    expect(sync.choiceOf('conflict-1')).toBeNull();
    expect(sync.conflicts).toEqual([]);
    expect(sync.devices).toEqual([]);
  });
});
