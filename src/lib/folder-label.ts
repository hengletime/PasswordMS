// D-154：把条目的 `folder_id` 翻译成「人该怎么读它」。
//
// 主人诉求（2026-10-31）：界面上只能从分组看到条目，**不能从条目看到它属于哪个分组**。
// 100 多条里想确认某条落在哪，只能逐个分组点开翻。
//
// 抽成纯函数的理由不是「两处都要用」，而是两处**判据必须逐字一致**：
// 列表行说「未分组」、详情页说「未知分组」，就是 D-150/D-151 那类自相矛盾的读数。
// 所以三态判定只写一份，UI 只负责把三态翻成文案。

import type { FolderDto } from '../types';

/** 条目所属分组的标签形态。 */
export type FolderLabel =
  /** 确实没有分组（`folder_id` 为空）。 */
  | { kind: 'ungrouped' }
  /** `folder_id` 指向一个不存在的分组（悬空引用）。 */
  | { kind: 'unknown' }
  /** 正常归属。 */
  | { kind: 'folder'; name: string };

/**
 * 判定条目属于哪个分组。
 *
 * **三态，不是两态**：`folder_id` 指向不存在的分组时（旧数据 / 同步残留 / 手工改库），
 * 既不能说它属于某个分组，也**不能冒充「未分组」** —— 侧栏「未分组 N 条」里并不包含它，
 * 冒充等于制造一个新的、没人会报错的矛盾读数。单列 `unknown` 让 UI 显式提示。
 *
 * @param folderId 条目的 `folder_id`（`null` / `undefined` / 空串都视为没有分组）
 * @param folders **全量**分组列表（`vault.folders` 来自独立 IPC，与列表筛选无关）。
 *   传筛选后的子集会把手脚正常的条目误判成 `unknown`。
 */
export function folderLabelOf(
  folderId: string | null | undefined,
  folders: readonly FolderDto[],
): FolderLabel {
  if (folderId === null || folderId === undefined || folderId === '') {
    return { kind: 'ungrouped' };
  }
  const found = folders.find((folder): boolean => folder.folder_id === folderId);
  return found === undefined ? { kind: 'unknown' } : { kind: 'folder', name: found.name };
}

/**
 * 把标签形态翻成界面文案。
 *
 * `t` 由调用方传入（`(key) => ui.t(key)`）而不是直接 import store：
 * 纯函数保持可离屏单测，也避免测试里拉进整个 store。
 * **注意传箭头函数包一层**：`ui.t` 内部读 `this.locale`，直接传方法引用会丢 `this`。
 *
 * @param label `folderLabelOf` 的结果
 * @param t 取文案的函数
 */
export function folderLabelText(label: FolderLabel, t: (key: string) => string): string {
  if (label.kind === 'folder') {
    return label.name;
  }
  return t(label.kind === 'ungrouped' ? 'folder.ungrouped' : 'folder.unknown');
}

/**
 * D-174：把一个 `folder_id` 收成**下拉框能显示**的值（不存在的一律落成 `null`）。
 *
 * 编辑器的分组下拉只有两类选项：`null`（未分组）与现有分组。若把悬空 id 直接塞进
 * `bind:value`，浏览器会**显示**第一个选项（未分组）却**绑定**着那个悬空 id ——
 * 用户看到的是「未分组」，保存时后端 `validate_draft_folder` 却回一句
 * `folder xxx does not exist`，于是这条条目**改任何字段都存不进去**，
 * 而错误信息跟用户眼前看到的东西完全对不上。
 *
 * 判据复用 `folderLabelOf`（不是再写一遍 `folders.some(...)`）：三态只有一处定义，
 * 否则「显示」和「保存」迟早各用一套判断。
 *
 * @param folderId 待收敛的 id（可能为空、可能悬空）
 * @param folders 全量分组列表
 */
export function resolvableFolderId(
  folderId: string | null | undefined,
  folders: readonly FolderDto[],
): string | null {
  return folderLabelOf(folderId, folders).kind === 'folder' ? (folderId ?? null) : null;
}
