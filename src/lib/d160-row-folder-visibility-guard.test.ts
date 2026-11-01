// D-160 守门：副行的「所属分组」只在跨组视图出现。
//
// 主人反馈（2026-11-01，附超长组名截图）：「只有为全部（全部条目）这里才显示分组名，
// 如果已经在某分组里了，这里就没有必要显示分组名了。」
//
// 判据本身在 `list-scope.ts` 的 `shouldShowRowFolder()` 里，由 `list-scope.test.ts`
// 逐视图钉住（含「收藏仍显示」这个唯一例外）。**本文件只管接线**：模板里那句
// `{#if showRowFolder}` 一旦被删掉或挪位，纯函数测得再对也没用 —— 那是 D-154 的组名
// 又回到每一行、又开始挤副行宽度。
//
// 为什么模板断言也要写：纯函数是「该不该显示」的正确性，模板是「有没有按它显示」的事实。
// 这一轮之前踩过的坑（D-158 的 `toContain` 被同一行的 title 喂饱）提醒：断言必须咬住
// **作为可见文本渲染**的那一处，而不是随便一处同名代码。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(['../components/ItemList.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** 取源码；**必须归一化换行**——文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

/** 切出区间（含首尾标记）。找不到返回空串——断言前先钉长度 > 0，否则切片失败会假绿。 */
function section(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  if (start < 0) {
    return '';
  }
  const end = text.indexOf(close, start + open.length);
  return end < 0 ? '' : text.slice(start, end + close.length);
}

describe('D-160：副行分组名的显示条件', () => {
  it('① 模板用 {#if showRowFolder} 包住「分隔符 + 组名」（删掉这个 if 就回到每行都写）', () => {
    const list = src('/ItemList.svelte');
    // 切到副行块：从 `.list-sub` 起点到它的 {/if}。若 `{#if showRowFolder}` 被删，
    // 这个 close 标记不再存在 → 切片为空 → 长度断言失败（真红，不是假绿）。
    const block = section(list, '<span class="list-sub pms-meta">', '{/if}');
    expect(block.length, '切片失败：副行块或 {#if showRowFolder} 不见了').toBeGreaterThan(0);
    expect(block).toContain('{#if showRowFolder}');
    // 组名必须在 if 之内（顺序断言，防止「if 包住类型、组名漏在外面」）。
    const ifAt = block.indexOf('{#if showRowFolder}');
    const sepAt = block.indexOf('list-sub-sep');
    const folderAt = block.indexOf('list-sub-folder');
    expect(sepAt).toBeGreaterThan(ifAt);
    expect(folderAt).toBeGreaterThan(ifAt);
    // 类型名**不在** if 里：进了分组也要知道这是登录还是笔记。
    expect(block.indexOf('list-sub-type')).toBeLessThan(ifAt);
  });

  it('② 判据取 vault 的三个状态位（漏传 ungroupedOnly = 未分组视图照样满屏「未分组」）', () => {
    const list = src('/ItemList.svelte');
    const block = section(list, 'const showRowFolder = $derived(', ');');
    expect(block.length, '切片失败：找不到 showRowFolder 派生').toBeGreaterThan(0);
    expect(block).toContain('shouldShowRowFolder({');
    expect(block).toContain('folderId: vault.selectedFolderId');
    expect(block).toContain('favoriteOnly: vault.favoriteOnly');
    expect(block).toContain('ungroupedOnly: vault.ungroupedOnly');
  });

  it('③ 「未分组 / 未知分组」的弱化样式仍在（隐藏的是组内重复，不是这个判据）', () => {
    const list = src('/ItemList.svelte');
    // 全部视图里未分组条目仍是「未分组」、悬空引用仍是「未知分组」，靠 is-none 弱化。
    expect(list).toContain(`class:is-none={folderTag.kind !== 'folder'}`);
    expect(list).toContain('.list-sub-folder.is-none');
  });

  it('④ 组名可截断、类型不可（超长组名不许把类型挤成省略号）', () => {
    const list = src('/ItemList.svelte');
    expect(list).toMatch(/\.list-sub-type \{\s*\n\s*flex: 0 0 auto;/);
    expect(list).toMatch(/\.list-sub-folder \{[^}]*text-overflow: ellipsis;/);
  });
});
