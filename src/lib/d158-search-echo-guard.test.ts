// D-158 守门：搜索框显示值必须始终等于「实际生效的筛选词」。
//
// 主人反馈（2026-11-01，附截图）：搜索框输入 "j" 筛出 5 条 → 想回到全部，**回不去了**，
// 切分组、切标签再回来都一样。截图里最刺眼的一点：**搜索框是空的**，列表却只有 5 条。
//
// 根因不是搜索逻辑，是「显示值」与「生效值」脱钩：
//   `ItemList` 的本地 `query` 初值写的是 `''`，而组件会因切分区 / 手机端列表↔详情推进
//   而被**卸载重建**。重建后本地归零、store 里那个 'j' 还在生效，epoch 又没变（没人推进它），
//   于是输入框空着、列表还在筛。用户看到的是「数据只剩下 5 条，且怎么都回不去」——
//   比单纯的分组内搜索难懂得多，因为它**没有任何界面痕迹**。
//
// 这里钉四件事，每一件单独漏掉都能复活这个 bug：
//   ① 初值取 store 现值（漏 = 重建即脱钩）；
//   ② 空闲对齐分支，判据是 `pending === null`（漏 = 只靠 epoch，下次仍会漏路径）；
//   ③ `cancelDebounce` 同时清 timer 与 pending（只清 timer = 空闲对齐永久瘫痪）；
//   ④ `setQuery` 必须在 `pending = null` **之前**（反过来会把自己刚打的词清回旧值）。
//
// 另有一条同族：空态共用一个文案时，搜不到东西会显示「还没有条目」，读起来像数据丢了。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
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

describe('D-158：搜索框显示值 = 实际筛选词', () => {
  it('① 本地 query 初值取 vault.query（不是空串）', () => {
    const list = src('/ItemList.svelte');
    expect(list).toContain('let query = $state(vault.query);');
    // 反向：空的初值就是本 bug 的成因，不许回来。
    expect(list).not.toContain("let query = $state('');");
  });

  it('② 存在空闲对齐分支，判据是 pending === null', () => {
    const list = src('/ItemList.svelte');
    const block = section(list, 'let seenQueryEpoch = vault.queryEpoch;', '});');
    expect(block.length).toBeGreaterThan(0);
    expect(block).toMatch(/if \(pending === null && query !== storeQuery\) \{\s*\n\s*query = storeQuery;/);
    // 必须读的是 store 的当前值（快照进局部变量），不是又读一次 vault.query 之外的猜测。
    expect(block).toContain('const storeQuery = vault.query;');
  });

  it('③ cancelDebounce 同时清 timer 与 pending（缺一即半残）', () => {
    const list = src('/ItemList.svelte');
    const block = section(list, 'function cancelDebounce()', '\n  }');
    expect(block.length).toBeGreaterThan(0);
    expect(block).toContain('clearTimeout(timer)');
    expect(block).toContain('pending = null');
  });

  it('④ setQuery 在 pending = null 之前（否则刚打的字被清回旧值）', () => {
    const list = src('/ItemList.svelte');
    const body = section(list, 'function onSearchInput', '}, 220);');
    expect(body.length).toBeGreaterThan(0);
    const submit = body.indexOf('void vault.setQuery(value);');
    // 用 **lastIndexOf**：回调里「本地被外部改过就放弃」那个兜底分支也有一句
    // `pending = null`（它必须在 return 之前），拿 indexOf 会命中它、把顺序读反。
    const release = body.lastIndexOf('pending = null;');
    expect(submit).toBeGreaterThan(-1);
    expect(release).toBeGreaterThan(submit);
    // 兜底分支仍必须自己清 pending——否则 pending 永久非 null，空闲对齐从此瘫痪。
    expect(body).toMatch(/if \(query !== value\) \{\s*\n\s*pending = null;\s*\n\s*return;/);
  });

  it('打字时必须先立 pending 再挂 timer（effect 在事件处理后才跑）', () => {
    const list = src('/ItemList.svelte');
    const body = section(list, 'function onSearchInput', '}, 220);');
    const mark = body.indexOf('pending = value');
    const arm = body.indexOf('timer = setTimeout');
    expect(mark).toBeGreaterThan(-1);
    expect(arm).toBeGreaterThan(mark);
  });

  it('⑤ 卸载时丢弃未提交的词（组件消失后不该还往 store 写）', () => {
    const list = src('/ItemList.svelte');
    // 220ms 窗口内切分区：timer 会在组件已经销毁后 fire，把用户放弃的词写进 store，
    // 下次重建时它又冒出来 = 没有来源的幽灵筛选。
    expect(list).toMatch(/\$effect\(\(\): \(\(\) => void\) => \(\) => \{\s*\n\s*cancelDebounce\(\);/);
  });

  it('空态按「有无生效搜索词」分流，不是共用一句「还没有条目」', () => {
    const list = src('/ItemList.svelte');
    const block = section(list, '{:else if vault.items.length === 0}', '{/if}\n    </div>');
    expect(block.length).toBeGreaterThan(0);
    // 判据必须是**生效值** vault.query，不是打字中的本地 query。
    expect(block).toContain('vault.query.length > 0');
    // D-159：词要写进文案里（跨分组保留后，空列表得自己说明是被哪个词筛掉的）。
    expect(block).toContain(`ui.t('item.empty_search', { q: vault.query })`);
    expect(block).toContain(`ui.t('item.empty')`);
  });

  it('四语言都有 empty_search，且与 empty 不同句（键以 zh-CN 为准）', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const text = src(`/i18n/${lang}.ts`);
      expect(text, `${lang} 缺 empty_search`).toContain('empty_search:');
    }
    const zh = src('/i18n/zh-CN.ts');
    expect(zh).toContain(`empty: '还没有条目'`);
    expect(zh).toContain(`empty_search: '没有匹配「{q}」的条目'`);
  });
});
