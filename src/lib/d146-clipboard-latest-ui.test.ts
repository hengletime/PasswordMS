// D-146 守门：剪贴板是单槽资源，界面只能显示**最新一次复制**的倒计时。
//
// 主人实测：复制用户名 → 提示「剪贴板将在 N 秒后清空」→ 未到点又复制密码 →
// 两路倒计时**交替**刷新同一行提示。剪贴板里早已是密码，用户名那条提示在语义上
// 已经作废，却还挂在界面上抢着写秒数。
//
// 三个必须同时成立的事实：
// 1. 旧代的 tick **不再转发**给界面（但旧代的定时器照旧跑完）；
// 2. 旧代到期**仍然发起清理**（D-135 的按代归属不能因为静默而丢）；
// 3. 复制类 toast 按 dedupeKey 单条化，不并排堆叠。

import { beforeEach, describe, expect, it, vi } from 'vitest';

const components = import.meta.glob(
  '../components/{ItemDetail,PasswordGenerator,SecureNoteView}.svelte',
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const mocks = vi.hoisted(() => ({
  copySecret: vi.fn(),
  clearSecretIfOwn: vi.fn(),
}));

vi.mock('./ipc', () => ({
  api: {
    system: {
      copySecret: (text: string, ttlSecs: number) => mocks.copySecret(text, ttlSecs),
      clearSecretIfOwn: (gen: number) => mocks.clearSecretIfOwn(gen),
    },
  },
}));

import { copySecretWithAutoClear } from './clipboard';
import { ui } from './stores/ui.svelte';

beforeEach(() => {
  vi.useFakeTimers();
  mocks.copySecret.mockReset();
  mocks.clearSecretIfOwn.mockReset();
  let next = 0;
  mocks.copySecret.mockImplementation(async (): Promise<number> => {
    next += 1;
    return next;
  });
  mocks.clearSecretIfOwn.mockResolvedValue(false);
});

describe('D-146 只有最新一代复制能驱动界面', () => {
  it('第二次复制之后，旧代不再往界面写任何剩余秒数', async () => {
    const ticks: number[] = [];
    await copySecretWithAutoClear('USERNAME-A', 30, (r) => ticks.push(r));
    vi.advanceTimersByTime(5_000); // 旧代已走 5 秒（30 → 25）

    const beforeSecond = ticks.length;
    await copySecretWithAutoClear('PASSWORD-B', 10, (r) => ticks.push(r));
    vi.advanceTimersByTime(10_000); // 旧代继续走（会经过 24…15），新代走完 9…0

    const afterSecond = ticks.slice(beforeSecond);
    // 旧实现把两路 tick 混在一起，这里会出现 24、23…（旧代的剩余秒数）——
    // 那正是主人看到的「数字来回跳」。
    expect(afterSecond.every((n) => n <= 10)).toBe(true);
    // 新代自身必须完整：首帧 10，之后 9…1、0，单调递减、不重不漏。
    expect(afterSecond).toEqual([10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);
  });

  it('旧代被静默，但到期仍按自己的代际号发起清理（D-135 不回归）', async () => {
    let silent = 0;
    await copySecretWithAutoClear('USERNAME-A', 2, () => {
      silent += 1;
    });
    // 第二代立刻覆盖剪贴板：第一代从这一刻起对界面沉默。
    await copySecretWithAutoClear('PASSWORD-B', 10);
    const afterSecond = silent;

    vi.advanceTimersByTime(2_000); // 第一代到期
    expect(mocks.clearSecretIfOwn.mock.calls.map((c) => c[0])).toEqual([1]);
    vi.advanceTimersByTime(8_000); // 第二代到期
    expect(mocks.clearSecretIfOwn.mock.calls.map((c) => c[0])).toEqual([1, 2]);

    // 静默的是界面，不是清理：第一代此后再没有 tick 送到调用方。
    expect(afterSecond).toBe(1);
  });

  it('复制失败不抢走上一代的界面所有权', async () => {
    const ticks: number[] = [];
    await copySecretWithAutoClear('PASSWORD-A', 4, (r) => ticks.push(r));
    mocks.copySecret.mockRejectedValueOnce(new Error('ipc down'));
    await expect(copySecretWithAutoClear('BROKEN', 4)).rejects.toThrow('ipc down');
    vi.advanceTimersByTime(1_000);
    // 上一代仍在驱动界面（4 → 3）。
    expect(ticks.at(-1)).toBe(3);
  });
});

describe('D-146 复制类 toast 单条化', () => {
  it('同 dedupeKey 的新 toast 顶掉旧的，界面上只留一条', () => {
    const first = ui.toast('success', 'common.copied_countdown', { n: 30 }, 'clipboard-copy');
    const second = ui.toast('success', 'common.copied_countdown', { n: 30 }, 'clipboard-copy');
    const same = ui.toasts.filter((t) => t.dedupeKey === 'clipboard-copy');
    expect(same).toHaveLength(1);
    expect(same[0].id).toBe(second);
    expect(same.some((t) => t.id === first)).toBe(false);
    ui.dismiss(second);
  });

  it('不传 dedupeKey 时保持原样堆叠（既有调用点行为不变）', () => {
    const a = ui.toast('info', 'common.copied');
    const b = ui.toast('info', 'common.copied');
    expect(ui.toasts.filter((t) => t.id === a || t.id === b)).toHaveLength(2);
    ui.dismiss(a);
    ui.dismiss(b);
  });
});

describe('D-146 调用点', () => {
  it('三个复制入口都用同一个 dedupeKey', () => {
    const entries = Object.entries(components);
    expect(entries).toHaveLength(3);
    for (const [key, src] of entries) {
      expect(src, key).toContain("'clipboard-copy'");
    }
  });

  it('详情页提示行点名字段，且字段与秒数同为渲染条件', () => {
    const detailKey = Object.keys(components).find((k) => k.endsWith('ItemDetail.svelte'));
    expect(detailKey, 'ItemDetail 必须在扫描面内').toBeDefined();
    const detail = components[detailKey as string];
    // 提示文案必须带字段占位符（否则「已复制」到底复制了什么，用户只能猜）。
    expect(detail).toContain('common.copied_countdown_field');
    expect(detail).toMatch(/\{#if copyCountdown > 0 && copiedField !== null\}/);
    // 两个复制出口都必须传字段名——少一个就会出现「字段永远是上一次的值」。
    expect(detail.match(/'(?:username|password)',/g) ?? []).toHaveLength(2);
    // 到点收提示：剪贴板已空，不该继续显示「还剩 0 秒」。
    expect(detail).toContain('copiedField = null');
  });
});
