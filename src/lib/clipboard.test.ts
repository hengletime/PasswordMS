// 剪贴板倒计时逻辑（PRD F-06：30 秒自动清空）。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { startCountdown } from './clipboard';
import { CLIPBOARD_TTL_SECS } from './constants';

describe('startCountdown', () => {
  beforeEach((): void => {
    vi.useFakeTimers();
  });
  afterEach((): void => {
    vi.useRealTimers();
  });

  it('fires immediately with the full ttl, then decrements each second', () => {
    const ticks: number[] = [];
    const done = vi.fn();
    startCountdown(30, (remaining: number): void => {
      ticks.push(remaining);
    }, done);

    // 首帧：立即以完整 ttl 回调一次。
    expect(ticks[0]).toBe(30);
    expect(done).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1000);
    expect(ticks[ticks.length - 1]).toBe(29);

    vi.advanceTimersByTime(1000);
    expect(ticks[ticks.length - 1]).toBe(28);

    // 走到归零：onDone 恰好一次。
    vi.advanceTimersByTime(28_000);
    expect(done).toHaveBeenCalledTimes(1);
    expect(ticks[ticks.length - 1]).toBe(0);

    // 归零后不再有回调。
    vi.advanceTimersByTime(5_000);
    expect(done).toHaveBeenCalledTimes(1);
  });

  it('calls onDone immediately when ttl <= 0', () => {
    const done = vi.fn();
    const ticks: number[] = [];
    startCountdown(0, (remaining: number): void => {
      ticks.push(remaining);
    }, done);
    expect(done).toHaveBeenCalledTimes(1);
    expect(ticks).toEqual([0]);
  });

  it('fires exactly ttl times (ttl..0) before stopping', () => {
    const ticks: number[] = [];
    const done = vi.fn();
    startCountdown(3, (remaining: number): void => {
      ticks.push(remaining);
    }, done);
    vi.advanceTimersByTime(10_000);
    expect(ticks).toEqual([3, 2, 1, 0]);
    expect(done).toHaveBeenCalledTimes(1);
  });

  it('stops ticking after cancel()', () => {
    const ticks: number[] = [];
    const done = vi.fn();
    const timer = startCountdown(30, (remaining: number): void => {
      ticks.push(remaining);
    }, done);
    vi.advanceTimersByTime(2000);
    const before = ticks.length;
    timer.cancel();
    vi.advanceTimersByTime(10_000);
    expect(ticks.length).toBe(before);
    expect(done).not.toHaveBeenCalled();
  });

  it('default ttl constant is 30 seconds', () => {
    expect(CLIPBOARD_TTL_SECS).toBe(30);
  });
});
