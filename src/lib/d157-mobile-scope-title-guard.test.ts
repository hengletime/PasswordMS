// D-157 的跨文件接线守卫：手机端顶栏标题必须说出「当前在哪个分组」。
//
// 主人诉求（2026-11-01）：「在手机端，由于左侧标题栏是隐藏的，在显示条目列表时，
// 不知道当前处于哪一个分组。可以把顶部的『全部条目』改成当前分组名吗？」
//
// 判据的**内容**（组名 / 未分组 / 未知分组 / 收藏 / 全部）由 `list-scope.test.ts` 单测覆盖，
// 这里只钉三件容易悄悄退化的接线：
//
//   · 顶栏那行绑的是 `scopeLabel`，不是又写回 `ui.t(sectionLabel)`（改回去谁都不会报错，
//     只是手机端重新变瞎）。
//   · `scopeLabel` 只在条目分区替换，且**把 vault 的三个作用域位全传进去**——
//     少传一位（比如 favoriteOnly）就会在收藏视图里显示「全部条目」。
//   · `.tb-title-mobile` 必须能吃 ellipsis：组名是用户自由输入，`display: inline` 下
//     `text-overflow` 不生效，长组名会顶开 ⋯ 按钮、把顶栏撑出横向滚动条（D-37 同型）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(['../components/MainLayout.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** 取源码；**必须归一化换行**——文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

/** 切出某段区间（含首尾标记）。找不到返回空串——断言前先钉长度 > 0，
 *  否则「切片失败」会被 `not.toContain` 当成通过（D-155 踩过）。 */
function section(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  if (start < 0) {
    return '';
  }
  const end = text.indexOf(close, start + open.length);
  return end < 0 ? '' : text.slice(start, end + close.length);
}

describe('D-157：手机端顶栏标题 = 当前分组名', () => {
  it('顶栏那行绑的是 scopeLabel（不是又写回 sectionLabel）', () => {
    const layout = src('/MainLayout.svelte');
    expect(layout).toMatch(/<span class="topbar-title tb-title-mobile">\{scopeLabel\}<\/span>/);
    // 反向：旧的写法不许回来。只查 tb-title-mobile 这一段，避免误伤桌面那行。
    const mobileLine = layout.split('\n').find((l) => l.includes('tb-title-mobile"'));
    expect(mobileLine ?? '').not.toContain('sectionLabel');
  });

  it('scopeLabel 把 vault 的三个作用域位全传进 listScopeLabel', () => {
    const layout = src('/MainLayout.svelte');
    const block = section(layout, 'const scopeLabel = $derived(', ');');
    expect(block.length).toBeGreaterThan(0);
    expect(block).toContain('listScopeLabel(');
    expect(block).toContain('folderId: vault.selectedFolderId');
    expect(block).toContain('favoriteOnly: vault.favoriteOnly');
    expect(block).toContain('ungroupedOnly: vault.ungroupedOnly');
    // 传的必须是全量分组：传筛选后的子集会把手脚正常的分组误判成「未知分组」。
    expect(block).toContain('vault.folders');
  });

  it('只在条目分区替换；其余分区仍用段落名', () => {
    const layout = src('/MainLayout.svelte');
    const block = section(layout, 'const scopeLabel = $derived(', ');');
    expect(block).toContain('inItems');
    expect(block).toContain('ui.t(sectionLabel)');
  });

  it('长组名不撑破顶栏：.tb-title-mobile 可收缩 + ellipsis', () => {
    const layout = src('/MainLayout.svelte');
    const block = section(layout, '.tb-title-mobile {\n      /* D-157', '}');
    expect(block.length).toBeGreaterThan(0);
    expect(block).toContain('text-overflow: ellipsis');
    expect(block).toContain('overflow: hidden');
    expect(block).toContain('min-width: 0');
    // `display: inline` 会让上面三条全部失效——这是本次改动的原始坑，钉死。
    expect(block).not.toMatch(/display:\s*inline\s*;/);
  });
});
