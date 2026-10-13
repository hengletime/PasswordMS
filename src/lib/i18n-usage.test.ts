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

describe('i18n usage coverage', () => {
  it('scans a meaningful number of components', () => {
    expect(entries.length).toBeGreaterThan(15);
  });

  it('every literal ui.t(...) key used in components exists in every locale', () => {
    const missing: string[] = [];
    for (const [path, text] of entries) {
      for (const match of text.matchAll(keyPattern)) {
        const key = match[1];
        for (const locale of LOCALES) {
          if (!hasKey(locale, key)) {
            missing.push(`${locale}:${key} (${path})`);
          }
        }
      }
    }
    expect(missing).toEqual([]);
  });
});
