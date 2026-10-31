// D-135 守门：剪贴板清空的**代际归属**与**非敏感文本不清空**。
//
// 两个必须同时成立的事实：
// 1. 每次写入拿到一个代际号，到期清理时**只清自己那一代**——否则「复制密码 A →
//    10 秒后复制用户名 B → A 的定时器到期」会把 B 清空（用户看到的是「用户名又没了」）。
// 2. 非敏感文本（用户名 / 配对码）走 `copyPlainText`：TTL 传 0，且**不启动任何
//    倒计时**（用 `copySecretWithAutoClear(…, 0)` 会立刻触发 onDone → 立刻清空）。

import { beforeEach, describe, expect, it, vi } from 'vitest';

// 源码断言面：用项目惯例的 `import.meta.glob(..., '?raw')`（无 node 类型依赖）。
const components = import.meta.glob(
  '../components/{ItemDetail,QuickSearch,sync/PairShow}.svelte',
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

beforeEach(() => {
  vi.useFakeTimers();
  mocks.copySecret.mockReset();
  mocks.clearSecretIfOwn.mockReset();
  // 后端每次写入分配自增代际号（与 `AppState::next_clipboard_gen` 同构）。
  let next = 0;
  mocks.copySecret.mockImplementation(async (): Promise<number> => {
    next += 1;
    return next;
  });
  mocks.clearSecretIfOwn.mockResolvedValue(false);
});

describe('D-135 剪贴板清理的代际归属', () => {
  it('两次复制的到期清理各带自己的代际号（不许不带号）', async () => {
    await copySecretWithAutoClear('PASSWORD-A', 30);
    await copySecretWithAutoClear('USERNAME-B', 10);
    vi.advanceTimersByTime(30_000);

    const gens = mocks.clearSecretIfOwn.mock.calls.map((c) => c[0] as number);
    // 10 秒的先到期、30 秒的后到期，与顺序无关，只要求「两代都各自认领」。
    expect(gens.slice().sort((a, b) => a - b)).toEqual([1, 2]);
    // 关键：不许出现 `undefined`（= 调用时没传号，后端将无法判断归属）。
    expect(gens.every((g) => typeof g === 'number' && Number.isInteger(g))).toBe(true);
  });

  it('先复制的那个到期时，不会用「最新一次」的代际号顶替', async () => {
    await copySecretWithAutoClear('PASSWORD-A', 30);
    await copySecretWithAutoClear('USERNAME-B', 5);
    vi.advanceTimersByTime(5_000);
    // 5 秒只到期第二个（gen=2）。若实现里把 gen 存成「最近一次」的共享变量，
    // 这里会拿到 2 也过——所以再推 25 秒看第一个是否仍报 1。
    expect(mocks.clearSecretIfOwn.mock.calls.map((c) => c[0])).toEqual([2]);
    vi.advanceTimersByTime(25_000);
    expect(mocks.clearSecretIfOwn.mock.calls.map((c) => c[0])).toEqual([2, 1]);
  });
});

describe('D-135 调用点不得再传 TTL=0', () => {
  it('三个复制入口都不传 0，且详情页用户名/密码同走带倒计时的路径', () => {
    const entries = Object.entries(components);
    expect(entries).toHaveLength(3);
    for (const [key, src] of entries) {
      // 传 0 会被后端读成「0 秒后清空」——这正是缺陷本体，必须绝迹。
      expect(src, key).not.toMatch(/copySecret\([^)]*,\s*0\s*\)/);
    }
    const detailKey = Object.keys(components).find((k) => k.endsWith('ItemDetail.svelte'));
    expect(detailKey, 'ItemDetail 必须在扫描面内').toBeDefined();
    const detail = components[detailKey as string];
    // 用户名与密码必须走同一个出口（同一出口才谈得上「两者行为一致」）。
    // D-146 起该调用拆成多行并多带一个字段名参数，按调用前缀计数。
    expect(detail.match(/void copyText\(/g) ?? []).toHaveLength(2);
    expect(detail).toContain('await copySecretWithAutoClear(text, ttl');
  });
});
