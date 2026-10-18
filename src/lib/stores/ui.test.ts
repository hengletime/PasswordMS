// D-58a：Toast 时长策略 —— success/info 固定档 + error 按渲染后文案长度自适应。

import { describe, expect, it } from 'vitest';

import { toastDurationMs, ui } from './ui.svelte';

describe('D-58a toast duration', () => {
  it('success stays at 3000ms regardless of message length', () => {
    expect(toastDurationMs('success', '')).toBe(3000);
    expect(toastDurationMs('success', 'ok')).toBe(3000);
    expect(toastDurationMs('success', 'x'.repeat(200))).toBe(3000);
  });

  it('info stays at 5000ms regardless of message length', () => {
    expect(toastDurationMs('info', '')).toBe(5000);
    expect(toastDurationMs('info', 'x'.repeat(200))).toBe(5000);
  });

  it('error short message floors at 6000ms', () => {
    expect(toastDurationMs('error', '')).toBe(6000);
    expect(toastDurationMs('error', 'ok')).toBe(6000);
    // 边界：40 字 × 150ms = 6000ms，恰好触底。
    expect(toastDurationMs('error', 'x'.repeat(40))).toBe(6000);
  });

  it('error scales 150ms per character within bounds', () => {
    // 41 字 → 6150ms（离开下限）；60 字 → 9000ms。
    expect(toastDurationMs('error', 'x'.repeat(41))).toBe(6150);
    expect(toastDurationMs('error', 'x'.repeat(60))).toBe(9000);
    // 边界：80 字 × 150ms = 12000ms，恰好触顶。
    expect(toastDurationMs('error', 'x'.repeat(80))).toBe(12000);
  });

  it('error long message caps at 12000ms', () => {
    expect(toastDurationMs('error', 'x'.repeat(81))).toBe(12000);
    expect(toastDurationMs('error', 'x'.repeat(120))).toBe(12000);
  });

  it('counts unicode code points (CJK = 1 char each)', () => {
    // 中文按码点计：10 个汉字 × 150ms = 1500ms → 触底 6000ms。
    expect(toastDurationMs('error', '错'.repeat(10))).toBe(6000);
    // 60 个汉字 → 9000ms（与等量 ASCII 一致）。
    expect(toastDurationMs('error', '错'.repeat(60))).toBe(9000);
  });

  it('toast() derives duration from ui.t() under current locale (Node-safe, no window)', () => {
    const id = ui.toast('success', 'common.ok');
    expect(ui.toasts.some((t): boolean => t.id === id)).toBe(true);
    ui.dismiss(id);
    expect(ui.toasts.some((t): boolean => t.id === id)).toBe(false);
  });
});
