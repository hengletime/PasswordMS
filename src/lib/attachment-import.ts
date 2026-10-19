// 附件导入流程（D-59）：把「选文件 → import_attachment IPC」抽成依赖注入的纯逻辑层，
// 使入口行为可被 vitest 行为级验证（取消 / 成功 / 失败 / 守卫四分支），参照 vault-export.ts 模式。
// 组件层（MainLayout / ItemList）只负责注入真实依赖；本模块不做任何 window / tauri 全局访问。
//
// 大小上限与容量校验由 core 层负责（repository.rs `import_attachment`：单文件 ≤ 20 MB，
// 超限报 `ERR_ATTACHMENT_TOO_LARGE`；库总量 ≤ 200 MB，超限报 `VaultCapacityExceeded`），
// 前端**不重复设限**，只负责把错误经 `toAppError` 展示。
//
// 标题语义：默认用文件名（**含扩展名**），与 core 层空 title 的回退行为完全一致
// （`import_attachment` 的 `title.trim().is_empty()` 分支回退 `filename`，含扩展名）。

import { toAppError } from './errors';
import type { ItemDto } from '../types';

/** 导入成功 toast 文案 key（与导出/改密成功一致，复用既有 common.ok）。 */
export const ATTACHMENT_OK_KEY = 'common.ok';

/** 附件导入流程所需的外部依赖（组件注入真实实现，测试注入替身）。 */
export interface AttachmentImportDeps {
  /** 系统文件选择对话框（单选）；resolve null = 用户取消。 */
  pickFile: () => Promise<string | null>;
  /** 导入 IPC（对应 `api.keydocs.importAttachment(source_path, title)`）。 */
  importAttachment: (sourcePath: string, title: string) => Promise<ItemDto>;
  /** Toast 出口（kind + 文案 key）。 */
  toast: (kind: 'info' | 'success' | 'error', messageKey: string) => void;
}

/** 导入流程结果：是否发出了导入 IPC；成功时携带新条目（供调用方刷新并选中）。 */
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
 * 执行附件导入流程。
 *
 * 行为契约（D-59）：
 * 1. 用户取消选择（null）或路径为空字符串 → **不发 IPC**、无任何 toast（`invoked: false`）；
 * 2. 导入成功 → success toast（`common.ok`），返回新条目（调用方负责 `vault.load()` 刷新 + 选中）；
 * 3. IPC 失败（含超限 `ERR_ATTACHMENT_TOO_LARGE`）→ error toast（`toAppError(err).messageKey`），
 *    `item: null`，不刷新列表。
 */
export async function runAttachmentImport(deps: AttachmentImportDeps): Promise<AttachmentImportResult> {
  const sourcePath = await deps.pickFile();
  // 守卫：取消 / 空路径 → 不发 IPC（空路径到后端必 `ERR_IO`，对用户无意义）。
  if (sourcePath === null || sourcePath.length === 0) {
    return { invoked: false, item: null };
  }
  try {
    const item = await deps.importAttachment(sourcePath, attachmentTitleFromPath(sourcePath));
    deps.toast('success', ATTACHMENT_OK_KEY);
    return { invoked: true, item };
  } catch (err) {
    deps.toast('error', toAppError(err).messageKey);
    return { invoked: true, item: null };
  }
}
