// D-153 / D-154 的源码结构守卫：「分组归属」必须在两个方向上都成立。
//
// 主人诉求（2026-10-31，连续两条）：
//   ① 「显示另有 4 条未分组，但我怎么从 100 多条里找出这 4 条？未分组的也应该可以点击后筛选出来。」
//   ② 「其实目前用户只能看到每个分组里有哪些条目，但不知道某个条目是属于哪个分组。」
//
// 两句话是同一件事的两面：分组与条目的关系在界面上是**单向可见**的。数字点不进去，
// 条目也不说自己在哪。这里钉的正是这两个方向上的接线，以及最容易悄悄做错的几点：
//
//   · 未分组用 `selectedFolderId === null` 表达 → 点「未分组」会列出全库（100 多条），
//     比没有入口更受挫。必须是 wire 上的独立参数 + store 里的独立布尔位。
//   · 侧栏「全部」行的 active 判据忘了排除 ungroupedOnly → 两行同时高亮。
//   · 条目分组标签两处各写一份 `folders.find` → 列表说「未分组」、详情说「未知分组」。
//
// 行为（条数与读数是否相等、排序是否一致）由 `crates/passwordms-core/tests/d153_ungrouped_filter.rs`
// 负责；文案三态的判定由 `folder-label.test.ts` 负责；这里只钉跨文件接线。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    './ipc.ts',
    './stores/vault.svelte.ts',
    './folder-label.ts',
    '../components/FolderSidebar.svelte',
    '../components/ItemList.svelte',
    '../components/ItemDetail.svelte',
    '../lib/i18n/zh-CN.ts',
    '../lib/i18n/zh-TW.ts',
    '../lib/i18n/en-US.ts',
    '../lib/i18n/th.ts',
    '../../src-tauri/src/commands/item_cmd.rs',
    '../../crates/passwordms-core/src/store/repository.rs',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——这些文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

describe('D-153：未分组必须是可点进去的视图', () => {
  it('IPC 传独立的 `ungrouped_only`（不许用 folder_id 的空值表达）', () => {
    expect(src('./ipc.ts')).toMatch(/ungrouped_only: ungroupedOnly \?\? null/);
  });

  it('Rust 命令形参表里有 `ungrouped_only`，且真的分流到未分组查询', () => {
    const rust = src('/commands/item_cmd.rs');
    expect(rust).toMatch(/ungrouped_only: Option<bool>/);
    expect(rust).toContain('repo.list_ungrouped_items(');
    // 分流必须发生在 list_items 之前，否则参数收下了却当「全部」用（读数对、行为错）。
    const branch = rust.indexOf('if ungrouped_only.unwrap_or(false)');
    const fallthrough = rust.indexOf('repo.list_items(folder_id.as_deref()');
    expect(branch).toBeGreaterThan(-1);
    expect(fallthrough).toBeGreaterThan(branch);
  });

  it('core 侧：未分组视图与侧栏读数用**同一个判据**', () => {
    const repo = src('/store/repository.rs');
    // 视图：`FolderScope::Ungrouped => item.folder_id.is_none()`
    expect(repo).toMatch(/Self::Ungrouped => item\.folder_id\.is_none\(\)/);
    // 读数：item_counts 内同一判据。
    const counts = repo.slice(repo.indexOf('pub fn item_counts('));
    expect(counts).toMatch(/if item\.folder_id\.is_none\(\) \{\n\s*out\.ungrouped \+= 1;/);
    // 视图入口必须真的存在（否则命令层的调用是幽灵）。
    expect(repo).toMatch(/pub fn list_ungrouped_items\(/);
  });

  it('store：独立布尔位 + 独立入口，load 时把它传给 IPC', () => {
    const store = src('/stores/vault.svelte.ts');
    expect(store).toMatch(/ungroupedOnly = \$state\(false\)/);
    expect(store).toMatch(/async selectUngrouped\(\): Promise<void>/);
    // 入口的方法体必须真的把标记立起来并清掉另两个视图标记 ——
    // 只断言方法名存在的话，方法体写成空壳也能过。
    const sel = store.slice(store.indexOf('async selectUngrouped'));
    const body = sel.slice(0, sel.indexOf('await this.load()'));
    expect(body).toContain('this.ungroupedOnly = true;');
    expect(body).toContain('this.selectedFolderId = null;');
    expect(body).toContain('this.favoriteOnly = false;');
    // 三处必传：列表查询、过滤器快照、保存后回落判定。
    expect(store).toMatch(/this\.ungroupedOnly,\n\s*\),/);
    expect(store).toMatch(/ungroupedOnly: this\.ungroupedOnly,/);
    expect(store).toMatch(/this\.ungroupedOnly = next\.ungroupedOnly;/);
    // 重置时必须清零，否则锁定后重新解锁仍停在「未分组」视图。
    const reset = store.slice(store.indexOf('reset(): void'));
    expect(reset).toContain('this.ungroupedOnly = false;');
  });

  it('侧栏：未分组那行是 `<button>` + 选中态，不再是不可点的说明文字', () => {
    const side = src('/FolderSidebar.svelte');
    expect(side).toMatch(/onclick=\{\(\) => pickUngrouped\(\)\}/);
    expect(side).toMatch(/class:active=\{vault\.ungroupedOnly\}/);
    // 反向：旧的「说明文字」实现不许回来（它就是主人抱怨的那个「点不进去」）。
    expect(side).not.toContain('ungrouped_note');
    expect(side).not.toContain('side-note');
  });

  it('侧栏：「全部」行的 active 必须排除未分组，否则两行同时高亮', () => {
    const side = src('/FolderSidebar.svelte');
    expect(side).toMatch(
      /class:active=\{vault\.selectedFolderId === null &&\s*!vault\.favoriteOnly &&\s*!vault\.ungroupedOnly\}/,
    );
  });
});

describe('D-154：条目必须说明自己属于哪个分组', () => {
  it('列表行渲染“类型 · 分组”，详情头部也渲染分组徽标', () => {
    const list = src('/ItemList.svelte');
    const detail = src('/ItemDetail.svelte');
    // 必须是**作为文本渲染**的那一处：`title={folderText(...)}` 只是悬停提示，
    // 断言写宽了会被这一处同名代码喂饱（本条变异测试踩过）。
    expect(list).toMatch(/\{folderText\(folderTag\)\}<\/span>/);
    expect(detail).toMatch(/\{folderText\(folderTag\)\}<\/span>/);
    expect(list).toMatch(/\{@const folderTag = folderLabelOf\(item\.folder_id, vault\.folders\)\}/);
    expect(detail).toMatch(/\{@const folderTag = folderLabelOf\(item\.folder_id, vault\.folders\)\}/);
  });

  it('两处共用同一个判定函数（各写一份 `folders.find` 迟早读数不一致）', () => {
    for (const file of ['/ItemList.svelte', '/ItemDetail.svelte']) {
      const text = src(file);
      expect(text, `${file} 没引用共享判定`).toContain("from '../lib/folder-label'");
      expect(text, `${file} 自己又写了一份分组查找`).not.toMatch(/folders\.find\(/);
    }
  });

  it('三态文案分开取键：未分组 / 未知分组不许共用一个兜底', () => {
    expect(src('./folder-label.ts')).toMatch(
      /label\.kind === 'ungrouped' \? 'folder\.ungrouped' : 'folder\.unknown'/,
    );
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const dict = src(`/i18n/${lang}.ts`);
      expect(dict, `${lang} 缺 folder.unknown`).toContain('unknown:');
      expect(dict, `${lang} 缺 folder.label`).toContain('label:');
    }
  });
});
