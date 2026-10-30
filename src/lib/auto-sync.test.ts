// 同步自动化（D-129）的假时钟全分支测试。
//
// 覆盖：①`isAutoSyncDue` 四条否决理由逐条 ②「启动时同步」命中即跑
// ③开关关 / 无设备 / 手动同步占用时都不跑 ④同一会话内 start() 幂等
// ⑤定时同步按心跳到期触发 ⑥成功后按 5 分钟、失败按 2→4→5 分钟渐进退避（D-130）
// ⑦stop() 后心跳不再触发 ⑧设置改变下一拍即生效（不用重启）
//
// 敏感性自查：把任一否决分支删掉（例如 `busy` 那条），对应用例立刻变红；
// 把 `startedThisSession` 守卫删掉，幂等用例变红；把失败退避改成和成功一样，
// 「失败后退避」用例变红。

import { describe, expect, it } from 'vitest';

import {
  AUTO_SYNC_FAIL_BACKOFF_MAX_MS,
  AUTO_SYNC_FAIL_BACKOFF_START_MS,
  AUTO_SYNC_INTERVAL_MS,
  AUTO_SYNC_TICK_MS,
  createAutoSync,
  failBackoffMs,
  isAutoSyncDue,
  type AutoSyncDeps,
} from './auto-sync';

const MIN = 60_000;

/** 让微任务链（tick 内的 await）跑完再继续推进假时钟。 */
function flush(): Promise<void> {
  return new Promise<void>((resolve) => setTimeout(resolve, 0));
}

interface HarnessOptions {
  autoSync?: boolean;
  syncOnStart?: boolean;
  devices?: number;
  /** `runOnce` 的返回值（false = 全部设备失败）。 */
  runOk?: boolean;
  /** `busy` 的初值（模拟手动同步在飞）。 */
  busy?: boolean;
}

function makeHarness(opts: HarnessOptions = {}): {
  controller: ReturnType<typeof createAutoSync>;
  runs: () => number;
  now: () => number;
  advance: (ms: number) => Promise<void>;
  setAutoSync: (v: boolean) => void;
  setSyncOnStart: (v: boolean) => void;
  setDevices: (n: number) => void;
  setRunOk: (v: boolean) => void;
} {
  let t = 1_000;
  let runs = 0;
  let autoSync = opts.autoSync ?? false;
  let syncOnStart = opts.syncOnStart ?? false;
  let devices = opts.devices ?? 0;
  let runOk = opts.runOk ?? true;
  let busy = opts.busy ?? false;

  const timers: Array<{ at: number; fn: () => void; cancelled: boolean }> = [];

  const deps: AutoSyncDeps = {
    getSettings: (): { auto_sync: boolean; sync_on_start: boolean } => ({
      auto_sync: autoSync,
      sync_on_start: syncOnStart,
    }),
    getDeviceCount: (): number => devices,
    isBusy: (): boolean => busy,
    runOnce: async (): Promise<boolean> => {
      runs += 1;
      return runOk;
    },
    now: (): number => t,
    schedule: (fn, ms): (() => void) => {
      const timer = { at: t + ms, fn, cancelled: false };
      timers.push(timer);
      return (): void => {
        timer.cancelled = true;
      };
    },
  };

  const controller = createAutoSync(deps);

  return {
    controller,
    runs: (): number => runs,
    now: (): number => t,
    async advance(ms: number): Promise<void> {
      const target = t + ms;
      // 逐拍推进：每触发一个定时器就让微任务跑完，否则 `armTick()` 挂的下一拍
      // 还没进队列，一次跨多分钟推进只会触发第一拍（假绿）。
      for (;;) {
        const next = timers.filter((x) => !x.cancelled && x.at <= target).sort((a, b) => a.at - b.at)[0];
        if (!next) {
          break;
        }
        t = next.at;
        next.cancelled = true;
        next.fn();
        await flush();
      }
      t = target;
      busy = false; // 手动同步占用只影响判定当拍，推进后视为已释放
    },
    setAutoSync(v: boolean): void {
      autoSync = v;
    },
    setSyncOnStart(v: boolean): void {
      syncOnStart = v;
    },
    setDevices(n: number): void {
      devices = n;
    },
    setRunOk(v: boolean): void {
      runOk = v;
    },
  };
}

describe('isAutoSyncDue（四类否决理由）', () => {
  const base = { enabled: true, deviceCount: 1, busy: false, now: 10_000, dueAt: null };

  it('开关没开 ⇒ 不跑', () => {
    expect(isAutoSyncDue({ ...base, enabled: false })).toBe(false);
  });

  it('一台设备都没配对 ⇒ 不跑（没有同步的对象）', () => {
    expect(isAutoSyncDue({ ...base, deviceCount: 0 })).toBe(false);
  });

  it('手动同步正在跑 ⇒ 不插队', () => {
    expect(isAutoSyncDue({ ...base, busy: true })).toBe(false);
  });

  it('本次解锁还没跑过（dueAt=null）⇒ 可跑', () => {
    expect(isAutoSyncDue(base)).toBe(true);
  });

  it('未到下次允许时刻 ⇒ 不跑；到点 ⇒ 跑', () => {
    expect(isAutoSyncDue({ ...base, dueAt: 10_001 })).toBe(false);
    expect(isAutoSyncDue({ ...base, dueAt: 10_000 })).toBe(true);
  });
});

describe('「启动时同步」', () => {
  it('开关开 + 有设备 ⇒ start() 立刻同步一次', async () => {
    const h = makeHarness({ syncOnStart: true, devices: 2 });
    await h.controller.start();
    expect(h.runs()).toBe(1);
    h.controller.stop();
  });

  it('开关关 ⇒ start() 不同步（但心跳照挂，等自动同步）', async () => {
    const h = makeHarness({ syncOnStart: false, devices: 2 });
    await h.controller.start();
    expect(h.runs()).toBe(0);
    h.controller.stop();
  });

  it('没有任何已配对设备 ⇒ 不跑（否则每次解锁都发一次注定失败的 IPC）', async () => {
    const h = makeHarness({ syncOnStart: true, devices: 0 });
    await h.controller.start();
    expect(h.runs()).toBe(0);
    h.controller.stop();
  });

  it('手动同步正在进行 ⇒ 让路，不插队', async () => {
    const h = makeHarness({ syncOnStart: true, devices: 1, busy: true });
    await h.controller.start();
    expect(h.runs()).toBe(0);
    h.controller.stop();
  });

  it('同一会话内重复 start() 幂等 ⇒ 只同步一次', async () => {
    const h = makeHarness({ syncOnStart: true, devices: 1 });
    await h.controller.start();
    await h.controller.start();
    await h.controller.start();
    expect(h.runs()).toBe(1);
    h.controller.stop();
  });

  it('stop() 之后再 start() 视为新会话 ⇒ 重新跑启动同步', async () => {
    const h = makeHarness({ syncOnStart: true, devices: 1 });
    await h.controller.start();
    h.controller.stop();
    await h.controller.start();
    expect(h.runs()).toBe(2);
    h.controller.stop();
  });
});

describe('定时自动同步', () => {
  it('只开「自动同步」时，start() 不跑，第一拍心跳才跑', async () => {
    const h = makeHarness({ autoSync: true, syncOnStart: false, devices: 1 });
    await h.controller.start();
    expect(h.runs()).toBe(0);
    await h.advance(AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(1);
    h.controller.stop();
  });

  it('成功后按 5 分钟排下次：4 分钟内不动，越过 5 分钟再跑', async () => {
    const h = makeHarness({ autoSync: true, devices: 1 });
    await h.controller.start();
    await h.advance(AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(1);
    // 首拍在 t=60s 触发，所以从此刻再推 4 分钟 = 距上次同步正好 4 分钟，未到期。
    await h.advance(4 * MIN);
    expect(h.runs()).toBe(1);
    // 再推 2 分钟：心跳在 5 分钟线上（含）那拍命中。
    await h.advance(2 * MIN);
    expect(h.runs()).toBe(2);
    h.controller.stop();
  });

  // D-130：原实现失败后固定等 15 分钟，主人实测「解锁手机改完条目、回电脑还是没有」——
  // 因为下一次重试还排在十几分钟后。现在 2 分钟起跳。
  it('失败后 2 分钟就重试（不再是 15 分钟）', async () => {
    const h = makeHarness({ autoSync: true, devices: 1, runOk: false });
    await h.controller.start();
    await h.advance(AUTO_SYNC_TICK_MS); // t=60s，首拍
    expect(h.runs()).toBe(1);
    await h.advance(1 * MIN); // t=120s：距上次仅 60s，未到 2 分钟线
    expect(h.runs()).toBe(1);
    await h.advance(1 * MIN); // t=180s：距上次正好 2 分钟，命中
    expect(h.runs()).toBe(2);
    h.controller.stop();
  });

  it('连续失败渐进退避 2→4→5 分钟封顶', async () => {
    const h = makeHarness({ autoSync: true, devices: 1, runOk: false });
    await h.controller.start();
    await h.advance(AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(1); // 第 1 次失败 → 下次 2 分钟后
    await h.advance(2 * MIN);
    expect(h.runs()).toBe(2); // 第 2 次失败 → 下次 4 分钟后
    await h.advance(4 * MIN);
    expect(h.runs()).toBe(3); // 第 3 次失败 → 下次 8 分钟被截成 5 分钟
    await h.advance(5 * MIN);
    expect(h.runs()).toBe(4);
    h.controller.stop();
  });

  // 注意：本用例必须让「成功之后**再失败**」——只测「成功后按 5 分钟」是测不到清零的，
  // 因为成功分支的间隔恒为 AUTO_SYNC_INTERVAL_MS，与 failures 无关（第一版就写成那样，
  // 名字骗人且删掉 `failures = 0` 也不会红）。
  it('一次成功即把失败计数清零（之后再失败从 2 分钟重新起算）', async () => {
    const h = makeHarness({ autoSync: true, devices: 1, runOk: false });
    await h.controller.start();
    await h.advance(1 * MIN); // t=60s：第 1 次失败 → failures=1
    expect(h.runs()).toBe(1);
    await h.advance(2 * MIN); // t=180s：第 2 次失败 → failures=2（下次应 4 分钟）
    expect(h.runs()).toBe(2);
    h.setRunOk(true);
    await h.advance(4 * MIN); // t=420s：成功 → failures 必须归零
    expect(h.runs()).toBe(3);
    h.setRunOk(false);
    await h.advance(5 * MIN); // t=720s：又失败 → 若已归零则 failures=1（下次 2 分钟）
    expect(h.runs()).toBe(4);
    await h.advance(2 * MIN); // t=840s：清零过 ⇒ 此刻到点，应跑第 5 次
    expect(h.runs()).toBe(5);
    h.controller.stop();
  });

  it('无配对设备时心跳空转，不产生同步', async () => {
    const h = makeHarness({ autoSync: true, devices: 0 });
    await h.controller.start();
    await h.advance(3 * AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(0);
    h.controller.stop();
  });

  it('设置改动下一拍即生效（不必重启）', async () => {
    const h = makeHarness({ autoSync: false, devices: 1 });
    await h.controller.start();
    await h.advance(2 * AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(0);
    h.setAutoSync(true);
    await h.advance(AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(1);
    h.controller.stop();
  });

  it('stop() 之后心跳不再触发', async () => {
    const h = makeHarness({ autoSync: true, devices: 1 });
    await h.controller.start();
    await h.advance(AUTO_SYNC_TICK_MS);
    expect(h.runs()).toBe(1);
    h.controller.stop();
    await h.advance(30 * MIN);
    expect(h.runs()).toBe(1);
  });

  it('间隔常量与退避曲线被钉死（防被悄悄改回长退避）', () => {
    expect(AUTO_SYNC_INTERVAL_MS).toBe(5 * MIN);
    expect(AUTO_SYNC_TICK_MS).toBe(MIN);
    expect(AUTO_SYNC_FAIL_BACKOFF_START_MS).toBe(2 * MIN);
    expect(AUTO_SYNC_FAIL_BACKOFF_MAX_MS).toBe(5 * MIN);
    // 曲线本体：2 → 4 → 5（8 被上限截断）→ 恒 5。
    expect(failBackoffMs(0)).toBe(0);
    expect(failBackoffMs(1)).toBe(2 * MIN);
    expect(failBackoffMs(2)).toBe(4 * MIN);
    expect(failBackoffMs(3)).toBe(5 * MIN);
    expect(failBackoffMs(9)).toBe(5 * MIN);
  });
});
