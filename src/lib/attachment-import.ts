// 附件添加流程（D-59 引入 / D-60 改造）：把「选文件 → add_attachment IPC」抽成
// 依赖注入的纯逻辑层，使入口行为可被 vitest 行为级验证（守卫 / 取消 / 成功 / 失败
// 四分支），参照 vault-export.ts 模式。组件层（ItemDetail 附件区）只负责注入真实依赖；
// 本模块不做任何 window / tauri 全局访问。
//
// D-60 语义：附件是父条目（登录/安全笔记）内嵌 `attachments` 数组的成员——
// `addAttachment(item_id, source_path, title)` 返回**父条目**（rev+1），调用方刷新即可。
//
// 大小上限与容量校验由 core 层负责（repository.rs `add_attachment`：单文件 ≤ 20 MB
// `ERR_ATTACHMENT_TOO_LARGE`；单条目附件数 ≤ 16 `ERR_ATTACHMENT_LIMIT`；文件名 ≤ 255B
// `ERR_VALIDATION`；库总量 ≤ 200 MB `VaultCapacityExceeded`），前端**不重复设限**，
// 只负责把错误经 `toAppError` 展示。
//
// 标题语义：默认用文件名（**含扩展名**），与 core 层空 title 的回退行为完全一致。

import { toAppError } from './errors';
import type { ItemDto } from '../types';

/** 添加成功 toast 文案 key（与导出/改密成功一致，复用既有 common.ok）。 */
export const ATTACHMENT_OK_KEY = 'common.ok';

/** 附件添加流程所需的外部依赖（组件注入真实实现，测试注入替身）。 */
export interface AttachmentImportDeps {
  /**
   * 系统文件选择对话框（单选）；resolve null = 用户取消。
   *
   * ⚠️ D-66：**实现方**必须保证「用户取消 → resolve null」，不得 reject。Android 上
   * `@tauri-apps/plugin-dialog` 取消时是 **reject("File picker cancelled")**，桌面端返回
   * null —— 注入处需自行 try/catch 归一（见 `AttachmentView.svelte` 的 `pickFile`）。
   * 本模块按契约不额外兜底：`pickFile` 一旦 reject，整个流程会抛出、无人接。
   */
  pickFile: () => Promise<string | null>;
  /** 添加 IPC（对应 `api.keydocs.addAttachment(item_id, source_path, title)`），返回父条目。 */
  addAttachment: (itemId: string, sourcePath: string, title: string) => Promise<ItemDto>;
  /** Toast 出口（kind + 文案 key）。 */
  toast: (kind: 'info' | 'success' | 'error', messageKey: string) => void;
}

/** 添加流程结果：是否发出了 IPC；成功时携带**父条目**（rev+1，供调用方刷新）。 */
export interface AttachmentImportResult {
  invoked: boolean;
  item: ItemDto | null;
}

/**
 * 从源路径提取文件名（含扩展名），作为附件默认标题。
 * 同时兼容 Windows（`\`）与 POSIX（`/`）分隔符；无分隔符时原样返回。
 */
export function attachmentTitleFromPath(sourcePath: string): string {
  const idx = Math.max(sourcePath.lastIndexOf('/'), sourcePath.lastIndexOf('\\'));
  return idx >= 0 ? sourcePath.slice(idx + 1) : sourcePath;
}

/**
 * 执行附件添加流程（针对父条目 `itemId`）。
 *
 * 行为契约（D-59/D-60）：
 * 1. 用户取消选择（null）或路径为空字符串 → **不发 IPC**、无任何 toast（`invoked: false`）；
 * 2. 添加成功 → success toast（`common.ok`），返回父条目（调用方负责刷新详情/列表）；
 * 3. IPC 失败（超限 / 附件数上限 / 文件名超长 / 容量）→ error toast
 *    （`toAppError(err).messageKey`），`item: null`。
 */
export async function runAttachmentImport(
  deps: AttachmentImportDeps,
  itemId: string,
): Promise<AttachmentImportResult> {
  const sourcePath = await deps.pickFile();
  // 守卫：取消 / 空路径 → 不发 IPC（空路径到后端必 `ERR_IO`，对用户无意义）。
  if (sourcePath === null || sourcePath.length === 0) {
    return { invoked: false, item: null };
  }
  try {
    const item = await deps.addAttachment(itemId, sourcePath, attachmentTitleFromPath(sourcePath));
    deps.toast('success', ATTACHMENT_OK_KEY);
    return { invoked: true, item };
  } catch (err) {
    deps.toast('error', toAppError(err).messageKey);
    return { invoked: true, item: null };
  }
}
