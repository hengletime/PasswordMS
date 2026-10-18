// 导出保险库流程（D-51 / Q-1）：把「空密码守卫 + 确认 + 选目标 + IPC + 事后清空」
// 从 Settings.svelte 抽成依赖注入的纯逻辑层，使守卫可被 vitest 行为级验证——
// 守卫被误删时 `runVaultExport` 会让空密码继续走到 export IPC，测试随即变红。
// 组件层只负责把真实依赖（ui.confirm / save 对话框 / api.vault.exportVault / ui.toast）
// 注入进来，本模块不做任何 window / tauri 全局访问，Node 测试环境可直接运行。

import { toAppError } from './errors';

/** 空密码就地提示的文案 key（D-51 引入）。 */
export const EXPORT_PASSWORD_REQUIRED_KEY = 'settings.export_password_required';

/** 导出成功后的成功文案 key（与组件既有行为一致）。 */
export const EXPORT_OK_KEY = 'common.ok';

/** 导出流程所需的外部依赖（由组件注入真实实现，测试注入替身）。 */
export interface VaultExportDeps {
  /** 用户在导出卡输入的主密码（D-51：独立于改密码卡片的输入框）。 */
  exportPassword: string;
  /** 二次确认框；resolve false = 用户取消。 */
  confirm: () => Promise<boolean>;
  /** 系统保存对话框；resolve null = 用户取消。 */
  pickDestination: () => Promise<string | null>;
  /** 导出 IPC（对应 `api.vault.exportVault(dest, password)`）。 */
  exportVault: (dest: string, password: string) => Promise<void>;
  /** Toast 出口（kind + 文案 key）。 */
  toast: (kind: 'info' | 'success' | 'error', messageKey: string) => void;
}

/** 导出流程结果：是否发出了导出 IPC；是否应清空密码输入框。 */
export interface VaultExportResult {
  invoked: boolean;
  cleared: boolean;
}

/**
 * 执行导出保险库流程。
 *
 * 行为契约（与 D-51 修复后的 Settings.svelte 逐条对应）：
 * 1. `exportPassword` 为空字符串 → **不发 IPC**，就地 error toast（`EXPORT_PASSWORD_REQUIRED_KEY`）；
 * 2. 用户在确认框/保存对话框取消 → 不发 IPC；
 * 3. 导出成功 → success toast（`common.ok`），调用方须清空输入框（`cleared: true`）；
 * 4. 导出失败 → error toast（`toAppError(err).messageKey`），不清空（保留现场供重试）。
 */
export async function runVaultExport(deps: VaultExportDeps): Promise<VaultExportResult> {
  // D-51 守卫：为空时不发 IPC（空密码到后端必 UnlockFailed →「解锁失败，请重试。」误导用户）。
  if (deps.exportPassword.length === 0) {
    deps.toast('error', EXPORT_PASSWORD_REQUIRED_KEY);
    return { invoked: false, cleared: false };
  }
  const ok = await deps.confirm();
  if (!ok) {
    return { invoked: false, cleared: false };
  }
  const dest = await deps.pickDestination();
  if (dest === null) {
    return { invoked: false, cleared: false };
  }
  try {
    await deps.exportVault(dest, deps.exportPassword);
    deps.toast('success', EXPORT_OK_KEY);
    return { invoked: true, cleared: true };
  } catch (err) {
    deps.toast('error', toAppError(err).messageKey);
    return { invoked: true, cleared: false };
  }
}
