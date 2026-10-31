// D-152 的源码结构守卫：侧栏条目数。
//
// 主人诉求（2026-10-31）：三台设备都同步完、待同步已归零，但**用户无法判断同步
// 有没有丢数据**——界面上一个条目数都没有。要求在分组名后加组内条目数。
//
// 这件事最容易在两个地方悄悄做错，而且都不会报错：
//
//   ① 用 `vault.items.length` 当「总条数」。`vault.items` 是**筛选后**的列表——
//      一切分组、一搜索、一点「只看收藏」就变样。用户正是拿这些数字核对同步结果，
//      数字随视图漂移等于把判据本身毁掉（D-150 的同型错误：读数不成立却没人报错）。
//   ② 只在「有分组时」显示，或把 0 藏起来。一个组突然变 0 条，恰恰是「同步会不会
//      丢数据」这个疑问最需要被看见的形态。
//
// 行为（回收站是否计入、恒等式是否成立）由 `crates/passwordms-core/tests/d152_item_counts.rs`
// 负责；这里只钉接线与文案，防止跨语言字面值静默漂移。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../lib/stores/vault.svelte.ts',
    './ipc.ts',
    '../components/FolderSidebar.svelte',
    '../types.ts',
    '../lib/i18n/zh-CN.ts',
    '../lib/i18n/zh-TW.ts',
    '../lib/i18n/en-US.ts',
    '../lib/i18n/th.ts',
    '../../src-tauri/src/dto.rs',
    '../../src-tauri/src/commands/item_cmd.rs',
    '../../src-tauri/src/lib.rs',
    '../../crates/passwordms-core/src/store/repository.rs',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——这些文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

describe('D-152：计数必须与筛选无关', () => {
  it('`vault.load()` 必须取一次 `api.items.counts()`', () => {
    const store = src('/stores/vault.svelte.ts');
    expect(store).toContain('api.items.counts()');
  });

  it('`counts` 不许由 `items.length` 或 `folders` 推导出来', () => {
    const store = src('/stores/vault.svelte.ts');
    // 只要出现 `this.counts = <含 items.length 的表达式>` 就是走回了漂移的老路。
    const bad = /this\.counts\s*=\s*\{[^}]*items\.length/s.test(store);
    expect(bad, '`counts` 不得从筛选后的 `items` 推导').toBe(false);
    // 反向确认它确实来自后端回包（变量名与解构一起钉，改名会立刻红）。
    expect(store).toMatch(/const \[items, folders, counts\] = await Promise\.all/);
    expect(store).toMatch(/this\.counts = counts;/);
  });

  it('`item_counts` 命令不带任何筛选参数（带上就等于把读数做成了视图相关）', () => {
    const ipc = src('ipc.ts');
    expect(ipc).toContain(`invoke<ItemCountsDto>('item_counts')`);
    const rust = src('/commands/item_cmd.rs');
    // Rust 侧形参表只有 state：一旦有人加 folder_id，wire 契约与语义一起变。
    expect(rust).toMatch(/pub fn item_counts\(state: tauri::State<'_, AppState>\) -> IpcResult<ItemCountsDto>/);
  });

  it('命令已注册（忘了注册 = 前端 invoke 直接 command not found）', () => {
    expect(src('/src-tauri/src/lib.rs')).toContain('commands::item_cmd::item_counts');
  });
});

describe('D-152：跨语言字段名逐字一致', () => {
  it('`FolderUsage` / `FolderDto` / `FolderDto`(ts) 三处都叫 `item_count`', () => {
    expect(src('/store/repository.rs')).toMatch(/pub item_count: usize/);
    expect(src('/src-tauri/src/dto.rs')).toMatch(/pub item_count: usize/);
    expect(src('/types.ts')).toMatch(/item_count\?: number/);
  });

  it('`ItemCounts` 三个字段两端同名', () => {
    const repo = src('/store/repository.rs');
    const dto = src('/src-tauri/src/dto.rs');
    const ts = src('/types.ts');
    for (const field of ['total', 'favorites', 'ungrouped']) {
      expect(repo, `core 缺 ${field}`).toMatch(new RegExp(`pub ${field}: usize`));
      expect(dto, `dto 缺 ${field}`).toMatch(new RegExp(`pub ${field}: usize`));
      expect(ts, `ts 缺 ${field}`).toMatch(new RegExp(`\\b${field}: number;`));
    }
  });

  it('`item_count` 不参与分组排序（#531 的排序键不许被顺手改）', () => {
    const repo = src('/store/repository.rs');
    const start = repo.indexOf('pub fn list_folders(');
    expect(start, '找不到 list_folders').toBeGreaterThan(-1);
    const end = repo.indexOf('pub fn create_folder(', start);
    expect(end).toBeGreaterThan(start);
    const body = repo.slice(start, end);
    expect(body).not.toContain('item_count');
  });
});

describe('D-152：侧栏必须把数字显示出来', () => {
  it('四个读数都进模板：全部 / 收藏 / 各组 / 未分组', () => {
    const side = src('/FolderSidebar.svelte');
    expect(side).toContain('vault.counts.total');
    expect(side).toContain('vault.counts.favorites');
    expect(side).toContain('folder.item_count');
    // D-153 起未分组那行从「说明文字」升级为可点的视图行，读数本身没变。
    expect(side).toContain('vault.counts.ungrouped');
  });

  // 本条只管**分组行**（以及全部/收藏）：某个组突然变 0 条，正是「同步会不会丢数据」
  // 最需要被看见的形态。未分组那行是视图入口，按 D-156 改为 0 时隐藏，见下一条。
  it('分组的 0 条也要显示（不能 `{#if count > 0}` 藏起来）', () => {
    const side = src('/FolderSidebar.svelte');
    // 直接钉「数字绑定」那一行的最近上下文里没有 `> 0` 之类的前置条件。
    // 用一个更稳的判据：`is-zero` 这个「零态样式钩子」必须在模板里被用上，
    // 它的存在本身就意味着 0 是要渲染的（藏起来就没必要给 0 单独配色了）。
    expect(side).toContain('class:is-zero');
    expect(side).toMatch(/\.side-count\.is-zero\s*\{/);
  });

  it('D-156：未分组为 0 时隐藏入口，但正停在该视图时例外', () => {
    const side = src('/FolderSidebar.svelte');
    // 判据必须**同时**含两个条件。只判 `> 0` 会这样出错：用户在未分组视图里删掉最后一条
    // → 读数归 0 → 入口连同 `active` 一起消失，人卡在一个空列表里、侧栏没有任何高亮项，
    // 看不出自己在哪。「选中的筛选项不因结果为空而消失」。
    expect(side).toMatch(/\{#if vault\.counts\.ungrouped > 0 \|\| vault\.ungroupedOnly\}/);
    expect(side).toContain('class:active={vault.ungroupedOnly}');
  });
});
