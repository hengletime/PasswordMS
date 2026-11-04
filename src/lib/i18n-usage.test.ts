// i18n 使用覆盖（全局审查项 ③）：
// 组件内出现的每一个**字面量** `ui.t('...')` 键，都必须在 zh-CN 与 en-US
// 两套字典中存在（动态键如 `ui.t(result.messageKey)` 不在静态校验范围）。
// 这补齐了 `Messages` 类型只能保证「两字典互相一致」、却无法保证
// 「组件引用的键确实存在」的缺口。
//
// 说明：用 Vite 的 `import.meta.glob`（`?raw`）读取组件源码，避免依赖 Node 类型。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES } from './i18n/index';

const modules = import.meta.glob('../**/*.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const entries = Object.entries(modules);
const keyPattern = /\.t\(\s*'([^']+)'/g;

/**
 * `ui.t('…')` 字面量引用的下界，**464 由本测试同一 matcher / 同一 glob 面实测**
 * （不是 `git grep` 在另一个路径面上取的数：两把尺子量同一件事，第一次改动就漂）。
 * 写死的意义是"少一条就回头看一眼"，不是精度。
 */
const MIN_UI_T_REFS = 464;

describe('i18n usage coverage', () => {
  it('scans a meaningful number of components', () => {
    expect(entries.length).toBeGreaterThan(15);
  });

  it('every literal ui.t(...) key used in components exists in every locale', () => {
    const missing: string[] = [];
    let totalMatched = 0;
    for (const [path, text] of entries) {
      for (const match of text.matchAll(keyPattern)) {
        totalMatched += 1;
        const key = match[1];
        for (const locale of LOCALES) {
          if (!hasKey(locale, key)) {
            missing.push(`${locale}:${key} (${path})`);
          }
        }
      }
    }
    // 判据不是"循环裸不裸"，而是**循环外面有没有一个断言绑定匹配到的数量**：
    // 有 → 安全；没有 → keyPattern 哪天跟不上写法，missing 仍是空数组、恒绿。
    //
    // 下界 = 本测试**同一 matcher、同一 glob 面**的落盘实测值（不留言量：
    // 留言量等于松到和没有一样）。少一条立刻红，红的意思是"扫描面或正则变了，
    // 回来看"——照 d192-feedback-guard 的 `calls >= 27` 写法。
    // 准数 vs 下界：这个数是附带统计（不是契约），所以用 `>=`；命令表、DTO 字段数
    // 那种契约数字才用 `toBe(…)`（ipc-contract 那 8 处保持不动）。
    expect(
      totalMatched,
      `扫描面可能失效：期望至少 ${MIN_UI_T_REFS} 条 ui.t( 引用，实际 ${totalMatched}`,
    ).toBeGreaterThanOrEqual(MIN_UI_T_REFS);
    expect(missing).toEqual([]);
  });
});
