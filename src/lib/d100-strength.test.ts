// D-100 守卫（2026-10-25）：强度评分必须由「字符池 + 长度」的熵连续决定。
//
// 主人实测缺陷：旧公式（字符类数 + 长度加成）在 4 类全开时恒封顶——生成器里拖
// 长度质量恒为「很强」。新公式 bits = len × log2(pool) 下，长度变化必须能移动评级。
// 回归口径：
// 1. 同字符集下长度增长 → 评级单调不减，且在生成器常用区间（6→16 位）至少升 2 档；
// 2. 典型锚点：4 类 6 位=弱、8 位=一般、10 位=强、13 位=很强；纯小写 8 位=弱；
// 3. 组件源码必须引用 lib/strength（不许再内联类数公式）。

import { describe, expect, it } from 'vitest';
import { scoreOf } from './strength';

const components = import.meta.glob(['../components/**/*.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

const meterSrc = Object.entries(components).find(([path]) =>
  path.endsWith('common/StrengthMeter.svelte'),
)?.[1];

const FOUR_CLASS = 'Ab1!'; // 每类至少一个字符 → 池 94
const LOWER_ONLY = 'abcdefg'; // 池 26

describe('scoreOf（D-100 熵评分）', () => {
  it('空串 = 0', () => {
    expect(scoreOf('')).toBe(0);
  });

  it('典型锚点：4 类 6/8/10/13 位 = 1/2/3/4', () => {
    const base = FOUR_CLASS;
    expect(scoreOf(base.repeat(1) + 'A')).toBe(1); // 6 位 ≈ 39 bits
    expect(scoreOf(base.repeat(2))).toBe(2); // 8 位 ≈ 52 bits
    expect(scoreOf(base.repeat(2) + 'Ab1')).toBe(3); // 10 位 ≈ 66 bits
    expect(scoreOf(base.repeat(3) + 'A')).toBe(4); // 13 位 ≈ 85 bits
  });

  it('同字符集下长度单调不减，6→16 位至少升 2 档（长度必须可感知）', () => {
    const scores: number[] = [];
    for (let len = 4; len <= 24; len += 1) {
      const pw = (FOUR_CLASS + LOWER_ONLY).slice(0, len).padEnd(len, 'x');
      scores.push(scoreOf(pw));
    }
    for (let i = 1; i < scores.length; i += 1) {
      expect(scores[i]).toBeGreaterThanOrEqual(scores[i - 1]);
    }
    expect(scores.at(-1)! - scores[0]).toBeGreaterThanOrEqual(2);
  });

  it('纯小写 8 位 = 弱；拉到 20 位 = 很强（长度能救回单一字符集）', () => {
    expect(scoreOf('password')).toBe(1);
    expect(scoreOf(LOWER_ONLY.repeat(3).slice(0, 20))).toBe(4);
  });

  it('组件必须引用 lib/strength，且源码不再含旧的类数封顶公式', () => {
    expect(meterSrc).toBeDefined();
    expect(meterSrc).toContain("from '../../lib/strength'");
    expect(meterSrc).not.toContain('base += 1');
    expect(meterSrc).not.toContain('Math.min(4, base)');
  });
});
