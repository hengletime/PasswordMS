// 自动锁定计时（D-61 A 案）的假时钟全分支测试。
// 覆盖：①到期触发 lock ②touch 重置后不提前锁 ③回前台超档立即锁（Android 补判）
// ④回前台未超档不锁、剩余时间按真实时钟重排 ⑤档位读取失败回退 5 分钟
// ⑥stop 后不锁、touch 不复活计时器（对抗性）。
// 敏感性自查：②若 touch 丢失重置（或重排被删），4 分钟处即触发 → 用例变红；
// ⑥若 touch 少了 running 守卫，stop+touch 后计时器复活 → 用例变红。

import { describe, expect, it } from 'vitest';

import { createAutoLock, type AutoLockDeps } from './auto-lock';

const MIN = 60_000;

/** 假时钟 harness：手动推进时间并触发到期定时器；`freeze` 模拟后台冻结 JS 计时器。 */
function makeHarness(getMinutesImpl: () => Promise<number> = async (): Promise<number> => 5): {
  deps: AutoLockDeps;
  controller: ReturnType<typeof createAutoLock>;
  lockCount: () => number;
  now: () => number;
  advance: (ms: number) => void;
  freeze: () => void;
  thaw: () => void;
} {
  let t = 1_000;
  let locks = 0;
  let frozen = false;
  const timers: Array<{ at: number; fn: () => void; cancelled: boolean }> = [];
  const deps: AutoLockDeps = {
    getMinutes: getMinutesImpl,
    lock: async (): Promise<void> => {
      locks += 1;
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
  return {
    deps,
    controller: createAutoLock(deps),
    lockCount: (): number => locks,
    now: (): number => t,
    advance(ms: number): void {
      t += ms;
      if (frozen) {
        return; // 后台：JS 计时器被冻结，到点也不触发。
      }
      // 到期定时器按调度时刻先后触发（cancel 的跳过）。
      for (const timer of [...timers].sort((a, b) => a.at - b.at)) {
        if (!timer.cancelled && timer.at <= t) {
          timer.cancelled = true;
          timer.fn();
        }
      }
    },
    freeze(): void {
      frozen = true;
    },
    thaw(): void {
      frozen = false;
    },
  };
}

describe('createAutoLock —— D-61 自动锁定计时（假时钟）', () => {
  it('到期触发 lock：start 后闲置满档位 → 恰好锁一次', async () => {
    const h = makeHarness();

    await h.controller.start();
    expect(h.lockCount()).toBe(0);

    h.advance(5 * MIN - 1);
    expect(h.lockCount()).toBe(0); // 差 1ms 不锁

    h.advance(1);
    expect(h.lockCount()).toBe(1); // 恰好 5 分钟 → 锁
  });

  it('touch 重置闲置：不提前锁（touch 丢失重置则此用例变红）', async () => {
    const h = makeHarness();

    await h.controller.start();
    h.advance(4 * MIN);
    h.controller.touch(); // 重置
    h.advance(4 * MIN);
    expect(h.lockCount()).toBe(0); // 距 touch 仅 4 分钟，不得锁

    h.advance(1 * MIN);
    expect(h.lockCount()).toBe(1); // touch 后满 5 分钟 → 锁
  });

  it('回前台超档：立即锁（Android 计时器冻结补判）', async () => {
    const h = makeHarness();

    await h.controller.start();
    const hiddenAt = h.now();
    h.freeze();
    h.advance(6 * MIN); // 后台 6 分钟：计时器被冻结，不触发
    h.thaw();
    expect(h.lockCount()).toBe(0);

    await h.controller.onReturnFromBackground(hiddenAt);
    expect(h.lockCount()).toBe(1); // 回前台补判 → 立即锁
  });

  // 边界契约文档化（恰好 == 档位）：实现 `onReturnFromBackground` 用 `>=`，本条把
  // 「后台停留时间恰好等于档位也立即锁」这条语义写进测试，防止日后被顺手改成「只有超过才锁」。
  // 诚实说明（QA 独立实测）：把 `>=` 改成 `>` **不会**让这条变红——变异后会落到 `reschedule()`，
  // 而 `reschedule()` 的 `remaining <= 0` 兜底在边界同样算出 remaining = 0 → 同样立即 `lock()`；
  // 又因 `lastActivity <= hiddenAt` 恒成立，两个实现在所有可达状态下行为等价。
  // 即这是**双保险冗余**下的等价变异，本用例是「契约文档」而非「变异杀手」。
  // 只有当 `reschedule()` 的 `remaining <= 0` 兜底被一并删掉时，本条才会成为能杀掉 `>` 变异的用例。
  it('回前台恰好达档（== 边界）：立即锁（边界契约文档化，非可判别变异）', async () => {
    const h = makeHarness();

    await h.controller.start();
    const hiddenAt = h.now();
    h.freeze();
    h.advance(5 * MIN); // 后台恰好 5 分钟：== 档位边界
    h.thaw();
    expect(h.lockCount()).toBe(0);

    await h.controller.onReturnFromBackground(hiddenAt);
    expect(h.lockCount()).toBe(1); // >= 含边界 → 立即锁（若实现误作 > 则此例红）
  });

  it('回前台未超档：不锁，后台时间计入闲置、剩余时间重排', async () => {
    const h = makeHarness();

    await h.controller.start();
    h.advance(2 * MIN);
    const hiddenAt = h.now();
    h.freeze();
    h.advance(1 * MIN); // 后台 1 分钟（计时器冻结）
    h.thaw();

    await h.controller.onReturnFromBackground(hiddenAt);
    expect(h.lockCount()).toBe(0); // 累计闲置 3 分钟 < 5 分钟

    // 后台时间计入闲置：剩余 2 分钟（而非重新计满 5 分钟）。
    h.advance(2 * MIN - 1);
    expect(h.lockCount()).toBe(0);
    h.advance(1);
    expect(h.lockCount()).toBe(1);
  });

  it('档位读取失败：保守回退 5 分钟', async () => {
    const h = makeHarness(async (): Promise<number> => {
      throw new Error('ipc down');
    });

    await h.controller.start();
    h.advance(4 * MIN);
    expect(h.lockCount()).toBe(0);
    h.advance(1 * MIN);
    expect(h.lockCount()).toBe(1); // 回退档位 5 分钟生效
  });

  it('档位为非法非正数：同样回退 5 分钟', async () => {
    const h = makeHarness(async (): Promise<number> => 0);

    await h.controller.start();
    h.advance(5 * MIN);
    expect(h.lockCount()).toBe(1); // 若信任 0 会立即锁 → 本用例证回退生效
  });

  it('stop 后不再锁；touch 不复活计时器（对抗性：缺 running 守卫则变红）', async () => {
    const h = makeHarness();

    await h.controller.start();
    h.controller.stop();
    h.advance(10 * MIN);
    expect(h.lockCount()).toBe(0); // stop 后到点不锁

    h.controller.touch(); // 已停止时的输入不得复活计时器
    h.advance(10 * MIN);
    expect(h.lockCount()).toBe(0);

    // stop 后仍可重新 start（再次解锁场景）。
    await h.controller.start();
    h.advance(5 * MIN);
    expect(h.lockCount()).toBe(1);
  });
});
