// 冲突三选一仲裁的纯逻辑（PRD Q3 裁定 / 架构 §3.9）。
//
// UI 必须提供「保留本机 / 采用远端 / 两者都留」，且**默认不自动选择**；
// 本模块只描述选择语义，实际落库由后端 `resolve_conflict` 执行。

import type { ConflictChoice } from '../types';

/** 默认选择：null（未选择，禁止自动提交）。 */
export const DEFAULT_CONFLICT_CHOICE: ConflictChoice | null = null;

/** 选择语义效果。 */
export interface ChoiceEffect {
  /** 是否保留本机版本为正式条目。 */
  keepLocal: boolean;
  /** 是否用远端版本覆盖本机。 */
  takeRemote: boolean;
  /** 是否额外保留一份远端副本（两者都留）。 */
  duplicateRemote: boolean;
}

/** 由选项推导效果。 */
export function choiceEffect(choice: ConflictChoice): ChoiceEffect {
  switch (choice) {
    case 'keep_local':
      return { keepLocal: true, takeRemote: false, duplicateRemote: false };
    case 'take_remote':
      return { keepLocal: false, takeRemote: true, duplicateRemote: false };
    case 'keep_both':
      return { keepLocal: true, takeRemote: false, duplicateRemote: true };
  }
}

/** 三选一选项清单（供 UI 渲染，顺序固定）。 */
export const CONFLICT_CHOICES: readonly ConflictChoice[] = [
  'keep_local',
  'take_remote',
  'keep_both',
];
