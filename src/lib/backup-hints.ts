// D-175：设置页「备份」卡片里那两格危险提示，从布局里取出来。
//
// 原来它们是 `Settings.svelte` 里两个相邻的 `{#if}`：`unreadable_batches === null` 一格、
// `> 0` 一格。功能没坏，但**正确性依赖两个 `{#if}` 的兄弟关系** —— 而兄弟关系是布局，
// 不是逻辑。重构时最容易被"顺手"嵌进上面那块 `{#if backupStatus.old_files > 0}` 里面，
// 改完测试全绿、那一格静默消失。
//
// 这不是假设：**最坏的情形恰好会被嵌进去**。全部批次都读不出时，后端数不出文件数、
// `old_files` 正好是 0，于是外层条件为假，里面那格永远不显示 —— 而它要说的正是
// 「盘上有东西，我数不清」。沉默的位置刚好在最该开口的时候。
//
// 抽成纯函数是为了能对它写**正向**断言（"给定这些输入，必须说一句话"）。原来只能写
// 黑名单（"这几句里不许出现 X"），黑名单挡不住"整块没渲染出来"。

import type { BackupPointDto, BackupPointKind, BackupStatusDto } from '../types';

/**
 * 提示的身份。
 *
 * 用稳定字符串而不是下标：数组下标会随着以后加提示而整体漂移，
 * 组件里按 id 分支才不会张冠李戴。
 */
export type BackupHintId =
  /** 数得清：盘上有 N 个读不出/认不出的批次目录。带数字。 */
  | 'old_unreadable'
  /** 数不清：连有几个都不知道。**不带数字**。 */
  | 'old_unreadable_uncounted';

/** 一条要显示的提示。 */
export interface BackupHint {
  id: BackupHintId;
  /**
   * 要渲染进文案的数字；`null` = **界面不许出现任何数字**。
   *
   * 分开带一个字段而不是让组件自己去看 `status.unreadable_batches`：那样"数不清那格
   * 不许有数字"就又变回组件里的隐式约定，而它是一条会被打破的规则（见 i18n.test.ts
   * 那条词表绊线）。
   */
  count: number | null;
}

/**
 * 当前该显示哪几条危险提示。
 *
 * 三态，不是两态：
 *
 * | `unreadable_batches` | 说明 | 提示 |
 * |---|---|---|
 * | `null` | 后端数不清（目录读不出） | `uncounted`，**不带数字** |
 * | `> 0`  | 数得清，确实有读不出的 | 带数字 |
 * | `0`    | 一个都没有 | 不出现 |
 *
 * **`null` 与 `0` 必须分开**：前者是"不知道"，后者是"确认没有"。
 * 把 `null` 当 `0` 处理，界面就会在"盘上有、我删不掉、我还不知道有几个"的时候
 * 一声不吭 —— 用户以为一切正常，直到自己去翻文件夹。
 *
 * 判据落地成一句可检查的话：**只有明确的 `0` 才允许沉默**（其余一切都要开口）。
 * 这样的函数对定义域是**全覆盖**的，"该开口时沉默"不可能发生 —— 正向绊线才写得出来。
 *
 * @param status 备份状态（只读 `unreadable_batches`，其余字段与这两格无关）
 */
export function backupHints(
  status: Pick<BackupStatusDto, 'unreadable_batches'>,
): BackupHint[] {
  const n = status.unreadable_batches;
  // **只有"明确等于 0"才允许沉默。**
  //
  // 这是这组提示唯一的明文规则，也是它敢放在危险色的理由：沉默需要有证据
  // （后端确认一个读不出的批次都没有），而"不知道"不构成证据。
  //
  // 后半段那些异常值（负数 / NaN）按后端类型（`Option<usize>`）不可能出现，
  // 真出现就说明上游坏了——那时的正确动作是**照旧开口**，而不是安静地把
  // "盘上有东西"这件事藏起来。宁可说"数不清"，也不编一个数字。
  if (n === 0) {
    return [];
  }
  if (n === null || !Number.isFinite(n) || n < 0) {
    return [{ id: 'old_unreadable_uncounted', count: null }];
  }
  return [{ id: 'old_unreadable', count: n }];
}

/**
 * 「这份备份是哪一步的」→ 文案键。
 *
 * ## 为什么是 `Record` 而不是模板字符串
 *
 * 组件里写 `ui.t(\`settings.restore_point_kind_${kind}\`)` 能跑，但**加一种备份类型时
 * 不会有任何东西变红**：四份字典各少一个键，界面就会把那串 key 原样显示给用户。
 * `Record<BackupPointKind, string>` 让"少一种"变成**类型错误**，而 `backup-hints.test.ts`
 * 再把每个键钉到四本字典上（缺一本就红）。
 *
 * 文案本身的人话标准：说「哪一步之前/之后」，不出现「回滚点」「旧代」「批次」「归档」。
 * 用户要选的是"回到哪一刻"，不是"这是一份什么类型的产物"。
 */
export const RESTORE_POINT_KIND_KEY: Record<BackupPointKind, string> = {
  restore_before: 'settings.restore_point_kind_restore_before',
  password_before: 'settings.restore_point_kind_password_before',
  password_after: 'settings.restore_point_kind_password_after',
  compact_before: 'settings.restore_point_kind_compact_before',
  import_before: 'settings.restore_point_kind_import_before',
  daily: 'settings.restore_point_kind_daily',
  last_write: 'settings.restore_point_kind_last_write',
};

/**
 * 取文案键，**认不出的类型给一句通用话**而不是把 key 裸奔给用户。
 *
 * 后端类型是封闭枚举，理论上进不了兜底分支；但 IPC 的返回是解析出来的 JSON，
 * 版本不一致（前端旧、后端新）时真会出现一个前端没听说过的 `kind`。
 * 那一刻的正确表现是"还能退回去，只是说不清是哪一步"，不是让 `settings.restore_point_kind_xxx`
 * 出现在屏幕上。
 */
export function restorePointKindKey(kind: string): string {
  return (
    (RESTORE_POINT_KIND_KEY as Record<string, string | undefined>)[kind] ??
    'settings.restore_point_kind_other'
  );
}

/**
 * 两张表：**现在的密码能打开** 与 **改密码之前的备份**。
 *
 * ## 为什么分组判据就是 `needs_old_password`
 *
 * 界面分两组的唯一理由是"要用哪个密码"，而那件事后端已经判过了
 * （[`BackupPointDto.needs_old_password`]）。前端**不许**再从 `kind` 或文件名
 * 推一遍——两份判据一有分歧，就会出现"标题说这组能用现在的密码、行里却说要用旧密码"
 * 这种自相矛盾的卡片，而它比不分组更让人糊涂。
 *
 * ## 为什么不是"按批次分"
 *
 * 批次是磁盘结构（`old-key/<epoch-ms>/`），用户在意的只是"这个要不要另一个密码"。
 * 换过两次密码的人会有两批，但两批都归"改密码之前"——界面不必让他先理解批次。
 *
 * 组内**保持后端给的顺序**（新的在前）：排序是后端的判据（`created_ms` 来自文件名，
 * 前端拿不到可信时刻），前端重排会让两张表的顺序对不上。
 */
export interface PointGroups {
  /** 现在的密码就能打开（当前代的事件快照、日备份、`last.bak`）。 */
  current: BackupPointDto[];
  /** 要用**改密码之前**那个密码才能打开（旧代整批 + 留在当前代的 `pre-rekey`）。 */
  older: BackupPointDto[];
}

export function splitPoints(points: readonly BackupPointDto[]): PointGroups {
  const current: BackupPointDto[] = [];
  const older: BackupPointDto[] = [];
  for (const p of points) {
    (p.needs_old_password ? older : current).push(p);
  }
  return { current, older };
}

/** 一组备份的字节合计（组标题里那句"N 份，共 X"用）。 */
export function sumBytes(points: readonly BackupPointDto[]): number {
  return points.reduce((n, p) => n + (Number.isFinite(p.bytes) ? p.bytes : 0), 0);
}

