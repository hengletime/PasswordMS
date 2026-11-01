// D-157：手机端顶栏标题必须说出「现在在看哪个分组」。
//
// 主人诉求（2026-11-01）：「在手机端，由于左侧标题栏是隐藏的，在显示条目列表时，
// 不知道当前处于哪一个分组。可以把顶部的『全部条目』改成当前分组名吗？」
//
// 桌面端靠侧栏高亮给出位置感；手机端侧栏收进抽屉、默认不可见，位置感只剩顶栏一个落脚点。
// 而顶栏原来固定显示导航段名（`nav.items` =「全部条目」）—— 进了「工作」组也照旧，
// 等于一直在回答「你在条目区」这个用户从没问过的问题。
//
// 抽成纯函数的理由（同 folder-label.ts）：三态判定（组名 / 未分组 / 未知分组）与
// 列表行、详情页**必须逐字一致**，各写一份必然漂移。

import { folderLabelOf, folderLabelText } from './folder-label';
import type { FolderDto } from '../types';

/** 列表当前的筛选作用域，对应 `vault` 上三个互斥的状态位。 */
export interface ListScope {
  /** 选中的分组 id（`null` = 全部）。 */
  folderId: string | null;
  /** 仅收藏。 */
  favoriteOnly: boolean;
  /** 未分组视图（`folderId` 也是 `null`，靠这一位区分「全部」）。 */
  ungroupedOnly: boolean;
}

/**
 * 当前列表作用域的可读名字，用于移动端顶栏标题。
 *
 * 参数用**结构**而不是整个 `vault` store：纯函数可离屏单测，也不必在测试里拉进 store。
 *
 * @param scope 三个互斥状态位
 * @param folders **全量**分组列表（`vault.folders` 来自独立 IPC，与列表筛选无关）；
 *   传筛选后的子集会把正常分组误判成「未知分组」。
 * @param t 取文案的函数。**必须传箭头函数包一层**：`ui.t` 内部读 `this.locale`，
 *   直接传方法引用会丢 `this`。
 */
export function listScopeLabel(
  scope: ListScope,
  folders: readonly FolderDto[],
  t: (key: string) => string,
): string {
  // 三个位互斥（`vault.selectFolder` / `selectUngrouped` / `toggleFavoriteOnly` 各自
  // 会把另两位清掉），但判定顺序仍然写死：将来有人加了新的组合位，不会因为顺序不定
  // 而让同一份数据出现两种读法。
  if (scope.favoriteOnly) {
    return t('nav.favorites');
  }
  if (scope.ungroupedOnly) {
    return t('folder.ungrouped');
  }
  if (scope.folderId !== null && scope.folderId !== '') {
    // 悬空引用 → 「未知分组」，不冒充「未分组」也不退回「全部条目」：
    // 侧栏「未分组 N 条」里并不包含它，冒充等于制造自相矛盾的读数（D-154 同一判据）。
    return folderLabelText(folderLabelOf(scope.folderId, folders), t);
  }
  return t('nav.items');
}

/**
 * D-160：列表行要不要在副行显示「所属分组」。
 *
 * 主人反馈（2026-11-01，附超长组名截图）：「只有为全部（全部条目）这里才显示分组名，
 * 如果已经在某分组里了，这里就没有必要显示分组名了。」
 *
 * 判据不是「进没进组」，而是**「当前列表里会不会出现不同分组的条目」**——
 * 组名唯一的信息量是「这条和旁边的不是一家」。按这个判据，四个视图分成两类：
 *
 * | 视图 | 显示 | 理由 |
 * |------|------|------|
 * | 全部 | **显示** | 全库混合，组名是唯一的归属线索（D-154 的原始诉求） |
 * | 收藏 | **显示** | 收藏是**跨组**的，条目依旧来自各个组 |
 * | 某个分组内 | 隐藏 | 20 条全是同一组，重复 20 遍只是噪音，还挤占副行宽度（长组名尤甚） |
 * | 未分组 | 隐藏 | 每行都写「未分组」= 说 20 遍同一句话 |
 *
 * **收藏这一行的存在正是本函数存在的理由**：如果只写 `folderId === null`，
 * 收藏视图会被一起隐藏掉——那是唯一「看着像全部、其实需要组名」的视图。
 *
 * @param scope 三个互斥状态位（与 `listScopeLabel` 同源，避免两处判据漂移）
 */
export function shouldShowRowFolder(scope: ListScope): boolean {
  // 顺序与 `listScopeLabel` 一致：收藏先于未分组先于 folderId。
  if (scope.favoriteOnly) {
    return true;
  }
  if (scope.ungroupedOnly) {
    return false;
  }
  // 空串按「没有分组」处理（与 `listScopeLabel` 同口径：空串不算未知分组）。
  return scope.folderId === null || scope.folderId === '';
}
