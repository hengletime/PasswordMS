// D-81 结构守卫：无 DOM 测试环境，用源码断言钉住两处**改了就会复发 P0**的结构事实。
// （判定纯函数与 store 接线分别由 save-visibility.test.ts / vault.test.ts 守，此处只守组件层。）

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/*.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

/** 按键名后缀取源码（不依赖 Vite 的键规整规则；未命中返回空串，由「扫描面有效」判红）。 */
function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-81 组件层结构守卫', () => {
  it('扫描面有效（glob 写错会退化成空串上的空断言假绿）', () => {
    expect(pickSource('ItemEditor.svelte').length).toBeGreaterThan(5000);
    expect(pickSource('ItemList.svelte').length).toBeGreaterThan(2000);
  });

  it('ItemEditor 新建时预填当前选中分组（不预填 = 条目存进根目录 = P0 复发）', () => {
    const src = pickSource('ItemEditor.svelte');
    // 正向锚点：预填表达式必须存在（编辑已有条目路径不变：item?.folder_id 优先）。
    expect(src).toContain('item?.folder_id ?? vault.selectedFolderId');
    // 负向：不许退回「恒为 null」（那是 D-81 的层①根因本体）。
    expect(src).not.toContain('let folderId = $state<string | null>(item?.folder_id ?? null);');
  });

  it('ItemList 搜索框接 queryEpoch（否则保存回落「全部条目」后输入框残留旧词）', () => {
    const src = pickSource('ItemList.svelte');
    // 正向锚点：必须读 epoch 并同步本地显示。
    expect(src).toContain('vault.queryEpoch');
    expect(src).toContain('query = vault.query;');
    // 负向：epoch 没变时不得反向覆盖本地输入（那是用户正在打的字，防抖期间 store 落后是正常的）。
    expect(src).toContain('vault.queryEpoch !== seenQueryEpoch');
  });
});
