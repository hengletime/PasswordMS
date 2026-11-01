// D-159 守门：切分组是换**范围**，不是取消**条件** —— 搜索词必须留在原处。
//
// 主人原话（2026-11-01，当天推翻 1.5.14 的行为）：「搜 j → 直接点侧栏『个人』，
// 搜索框不应该自动清空，应该只显示『个人』分组里 j 的搜索结果。」
//
// 1.5.14 按「导航应当重置临时筛选」这条通用直觉清了词，看着很讲道理，但代价是：
// 用户每点一次侧栏，刚建立起来的「组内搜索」视角就被扔掉一次，还得重新打字。
//
// 真正该保证的是**「搜索框显示 = 实际生效」**（D-158，由 ItemList 负责），
// 而不是「切范围就清条件」。词始终看得见、旁边就有一键清空，用户想回全库随时能回；
// 反过来替人清词，等于替人决定他要找什么。
//
// 这里钉四件事：
//   ① 三个视图切换函数体内**不出现**对 query / queryEpoch 的任何写入；
//   ② 那条「导航重置」的私有方法整体消失（留着迟早被再次调用）；
//   ③ 空列表必须把**词本身**写进文案，并就地给清空出口（否则「这组空」和「这组没有 j」分不清）；
//   ④ 四语言 empty_search 都带 `{q}` 占位符。
//
// 与 d158-search-echo-guard 的分工：那个文件管「显示值不许和生效值脱钩」，
// 本文件管「谁有权改动用户的条件」。两条都成立时，这个 bug 才算真修掉。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    './stores/vault.svelte.ts',
    '../components/ItemList.svelte',
    './i18n/zh-CN.ts',
    './i18n/zh-TW.ts',
    './i18n/en-US.ts',
    './i18n/th.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

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

/** 三个「同级单一视图」的切换入口。名字写死：改名等于换语义，得先来改这个测试。 */
const VIEW_SWITCHERS = ['async selectFolder(', 'async selectUngrouped(', 'async toggleFavoriteOnly('];

describe('D-159：切视图不许清搜索词', () => {
  it('① 三个视图切换函数体内没有任何 query / queryEpoch 写入', () => {
    const store = src('/stores/vault.svelte.ts');
    for (const marker of VIEW_SWITCHERS) {
      const body = section(store, marker, '\n  }');
      expect(body, `${marker} 切片失败`).not.toBe('');
      // 读 `this.query` 也一并禁掉：验证「没清词」的方式不该是函数体里偷看再看。
      expect(body, `${marker} 动了搜索词`).not.toMatch(/\bthis\.query\b/);
      expect(body, `${marker} 推进了 queryEpoch`).not.toContain('queryEpoch');
    }
  });

  it('② 「导航重置搜索词」那条私有方法整体不存在（留着迟早被再调一次）', () => {
    const store = src('/stores/vault.svelte.ts');
    expect(store).not.toContain('resetQueryOnNavigate');
  });

  it('③ 空列表把搜索词写进文案，并就地给「清除搜索」出口', () => {
    const list = src('/ItemList.svelte');
    const block = section(list, '{:else if vault.items.length === 0}', '{/if}\n    </div>');
    expect(block, '空态切片失败').not.toBe('');
    // 词必须作为渲染参数出现（「这组空」与「这组没有 j」的区别就在这一个词上）。
    expect(block).toContain(`ui.t('item.empty_search', { q: vault.query })`);
    // 出口就在眼前：用户不必先意识到上面那个输入框里有字、再去摸 ×。
    expect(block).toContain('onclick={clearSearch}');
    expect(block).toContain(`ui.t('common.clear_search')`);
  });

  it('④ 四语言的 empty_search 都带 {q} 占位符（缺一个就是给某语言用户少说半句）', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const text = src(`/i18n/${lang}.ts`);
      expect(text, `${lang} 缺 empty_search`).toContain('empty_search:');
      expect(text, `${lang} 的 empty_search 没带 {q}`).toMatch(/empty_search: '[^']*\{q\}[^']*'/);
    }
  });

  it('⑤ ItemList 挂载初值仍取 store 现值（跨分组回来还看得见自己在搜什么）', () => {
    const list = src('/ItemList.svelte');
    // 词保留了但输入框显示空 = 又回到 D-158 的「看不见的筛选」。
    expect(list).toContain('let query = $state(vault.query);');
  });
});
