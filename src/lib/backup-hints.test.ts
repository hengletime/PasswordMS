// D-175：备份卡片那两格危险提示的**正向**绊线。
//
// 既有的 `i18n.test.ts` 是黑名单：那两句文案里**不许**出现"占用空间""disk space"这类词。
// 黑名单只在我们想得到词的时候红，它挡不住更常见的一种垮法——**整块没渲染出来**：
// 组件里少一个 `{#if}`、或者把那两格并进外层 `old_files > 0` 里面，界面一声不吭，
// 而所有文案测试全绿。
//
// 所以这组测试问的是反方向的问题：**给定这些输入，界面必须说一句话。**
//
// | `unreadable_batches` | 必须说什么 | 不许说什么 |
// |---|---|---|
// | `null`（数不清） | `uncounted` 那格 | **任何数字** |
// | `> 0`（数得清） | 带数字那格 | —— |
// | `0`（确认没有） | 不出现 | 任何提示 |
//
// `null` 与 `0` 必须分开：前者是"不知道"，后者是"确认没有"。
// 把 `null` 当 `0` 处理，界面就会在"盘上有、我删不掉、我还不知道有几个"时保持沉默。

import { describe, expect, it } from 'vitest';
import { backupHints } from './backup-hints';

/** 只取函数关心的那个字段，其余留空（它不读别的，也不许读别的）。 */
function status(unreadable_batches: number | null): { unreadable_batches: number | null } {
  return { unreadable_batches };
}

describe('D-175 正向绊线：该说话的时候必须说', () => {
  it('数不清（null）→ 有且只有一条 uncounted 提示', () => {
    const hints = backupHints(status(null));
    expect(hints).toHaveLength(1);
    expect(hints[0].id).toBe('old_unreadable_uncounted');
  });

  it('数得清（1 / 2 / 99）→ 有且只有一条带数字的提示', () => {
    for (const n of [1, 2, 99]) {
      const hints = backupHints(status(n));
      expect(hints, `unreadable_batches=${n}`).toHaveLength(1);
      expect(hints[0].id).toBe('old_unreadable');
      expect(hints[0].count).toBe(n);
    }
  });

  it('恒等式：非 0 必有一句，为 0 必无一句（没有第三种可能）', () => {
    for (const n of [null, -1, 0, 1, 7, 1000]) {
      const expected = n === 0 ? 0 : 1;
      expect(backupHints(status(n)).length, `n=${n}`).toBe(expected);
    }
  });
});

describe('D-175 反向断言：不该说的时候一句都不许说', () => {
  it('确认没有（0）→ 空数组，不是"一条空提示"', () => {
    // 返回 `[{ id: ... }]` 而让组件自己去判空，等于把"不说"又变回布局层的约定。
    expect(backupHints(status(0))).toEqual([]);
  });

  it('数不清那格**不带数字**（count 为 null，组件据此不渲染占位符）', () => {
    const [hint] = backupHints(status(null));
    expect(hint.count).toBeNull();
  });
});

describe('D-175 组件接线：判定必须来自函数，不许写回布局', () => {
  const raw = import.meta.glob(['../components/Settings.svelte'], {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;
  const source = (
    Object.entries(raw).find(([p]) => p.endsWith('/Settings.svelte'))?.[1] ?? ''
  ).replace(/\r\n/g, '\n');

  it('先钉住守卫本身没空跑（读不到源码时后面的 not.toContain 会假绿）', () => {
    expect(source.length).toBeGreaterThan(1000);
    expect(source).toContain('backupHints(backupStatus)');
  });

  it('两格 `{#if}` 不许写回模板（写回去就等于把逻辑藏进布局）', () => {
    expect(source).not.toContain('{#if backupStatus.unreadable_batches');
    expect(source).not.toContain('{:else if backupStatus.unreadable_batches');
  });

  it('两个键都还在模板里（抽了函数却只渲染一句 = 另一句被静默丢掉）', () => {
    expect(source).toContain("ui.t('settings.restore_point_old_unreadable_uncounted')");
    expect(source).toContain("ui.t('settings.restore_point_old_unreadable',");
  });
});
