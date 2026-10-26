// D-103：详情页头部「标题 + 类型徽章」横排布局的防挤压守门。
// 根因：flex 子项默认 min-width:auto，长标题撑住 h2 不收缩（ellipsis 失效），
// 右侧「登录/笔记」徽章被挤到逐字换行变竖排。修复 = h2 可收缩 + 徽章禁换行。
// 两条 CSS 规则缺一不可，源码断言锁死（样式级修复无运行时断言可写）。
// 源码读取用 import.meta.glob('?raw')（svelte-check 不认 node:fs，见 d100 教训）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(['../components/ItemDetail.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const src = modules['../components/ItemDetail.svelte'] ?? '';

describe('D-103 detail header badge anti-squeeze', () => {
  it('h2 允许 flex 收缩（min-width: 0），ellipsis 才能真正生效', () => {
    expect(src).not.toBe('');
    const h2 = src.match(/h2 \{[^}]*\}/);
    expect(h2).not.toBeNull();
    expect(h2![0]).toContain('min-width: 0');
    expect(h2![0]).toContain('text-overflow: ellipsis');
  });

  it('类型徽章禁换行且不被压缩（flex-shrink: 0 + nowrap）', () => {
    const rule = src.match(/\.detail-titles :global\(\.pms-badge\) \{[^}]*\}/);
    expect(rule).not.toBeNull();
    expect(rule![0]).toContain('flex-shrink: 0');
    expect(rule![0]).toContain('white-space: nowrap');
  });
});
