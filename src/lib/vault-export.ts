// 导出保险库流程（D-51 / Q-1）：把「空密码守卫 + 确认 + 选目标 + IPC + 事后清空」
// 从 Settings.svelte 抽成依赖注入的纯逻辑层，使守卫可被 vitest 行为级验证——
// 守卫被误删时 `runVaultExport` 会让空密码继续走到 export IPC，测试随即变红。
// 组件层只负责把真实依赖（ui.confirm / save 对话框 / api.vault.exportVault / ui.toast）
// 注入进来，本模块不做任何 window / tauri 全局访问，Node 测试环境可直接运行。

import { formatBytes } from './constants';
import { runSaveToDestination } from './save-dialog';

/** 空密码就地提示的文案 key（D-51 引入）。 */
export const EXPORT_PASSWORD_REQUIRED_KEY = 'settings.export_password_required';

/** 导出成功后的成功文案 key（后端未回报字节数时的兜底）。 */
export const EXPORT_OK_KEY = 'common.ok';

/**
 * 导出成功且**确有字节写入**时的文案 key（带 `{size}` 插值，D-144）。
 *
 * 为什么非要把体积念出来：2026-10-31 的真机事故里，Android 导出静默失败、
 * 盘上只剩 SAF 建好的 0 字节占位文件，而界面显示「确定」——用户因此相信备份好了，
 * 卸载清库后才发现无可恢复。**把字节数摆在成功提示里，「0 B」就无法假装成功。**
 */
export const EXPORT_OK_WITH_SIZE_KEY = 'settings.export_ok_size';

/** 写盘"成功"但一个字节都没落地时的文案 key（D-144 兜底，正常路径不该走到）。 */
export const EXPORT_EMPTY_KEY = 'settings.export_empty_failed';

/** 导出流程所需的外部依赖（由组件注入真实实现，测试注入替身）。 */
export interface VaultExportDeps {
  /** 用户在导出卡输入的主密码（D-51：独立于改密码卡片的输入框）。 */
  exportPassword: string;
  /** 二次确认框；resolve false = 用户取消。 */
  confirm: () => Promise<boolean>;
  /** 系统保存对话框；resolve null = 用户取消。 */
  pickDestination: () => Promise<string | null>;
  /** 导出 IPC（对应 `api.vault.exportVault(dest, password)`），resolve 写入字节数。 */
  exportVault: (dest: string, password: string) => Promise<number>;
  /** Toast 出口（kind + 文案 key + 可选插值参数）。 */
  toast: (kind: 'info' | 'success' | 'error', messageKey: string, params?: Record<string, string | number>) => void;
}

/** 导出流程结果：是否发出了导出 IPC；是否应清空密码输入框；实际写入字节数。 */
export interface VaultExportResult {
  invoked: boolean;
  cleared: boolean;
  /**
   * 实际写入目标的字节数（`null` = 后端没回报，例如旧包或测试替身）。
   * 调用点用它做「上次备份：时间 + 体积」的记录；`0` 永远不会走到成功分支。
   */
  bytes: number | null;
}

/**
 * 执行导出保险库流程。
 *
 * 行为契约（与 D-51 修复后的 Settings.svelte 逐条对应）：
 * 1. `exportPassword` 为空字符串 → **不发 IPC**，就地 error toast（`EXPORT_PASSWORD_REQUIRED_KEY`）；
 * 2. 用户在确认框/保存对话框取消 → 不发 IPC，且**正常返回**（对话框 reject 也算取消，
 *    归一逻辑见 `save-dialog.ts`；Android 上取消就是 reject）；
 * 3. 导出成功 → success toast（`common.ok`），调用方须清空输入框（`cleared: true`）；
 * 4. 导出失败 → error toast（`toAppError(err).messageKey`），不清空（保留现场供重试）。
 */
export async function runVaultExport(deps: VaultExportDeps): Promise<VaultExportResult> {
  // D-51 守卫：为空时不发 IPC（空密码到后端必 UnlockFailed →「解锁失败，请重试。」误导用户）。
  if (deps.exportPassword.length === 0) {
    deps.toast('error', EXPORT_PASSWORD_REQUIRED_KEY);
    return { invoked: false, cleared: false, bytes: null };
  }
  const ok = await deps.confirm();
  if (!ok) {
    return { invoked: false, cleared: false, bytes: null };
  }
  // D-71/D-72：选目标 + 写盘的取消/失败归一收敛在 `save-dialog.ts`。
  // 旧实现把 `pickDestination()` 放在 try 外面：Android 取消是 reject，异常直接
  // 逃出本函数，而 `Settings.svelte` 的 `await runVaultExport(...)` 没有 try/catch →
  // **未捕获 rejection**，用户点取消什么都不发生（只有控制台有红字）。
  const outcome = await runSaveToDestination({
    pickDestination: deps.pickDestination,
    write: (dest: string): Promise<number> =>
      deps.exportVault(dest, deps.exportPassword),
  });
  if (outcome.kind === 'cancelled') {
    // 取消必须**正常返回**，不许抛 —— 这是「未捕获 rejection」的回归锚点。
    return { invoked: false, cleared: false, bytes: null };
  }
  if (outcome.kind === 'failed') {
    // 已选到目标但写盘失败：IPC 确实发出过，故 `invoked: true`；保留密码供重试。
    deps.toast('error', outcome.messageKey);
    return { invoked: true, cleared: false, bytes: null };
  }
  // D-144：字节数决定「成功」是不是真的。
  //  · 正数 → 成功，文案里念出体积（0 B 骗不了人）；
  //  · 恰好 0 → 后端/SAF 一个字节都没落地，**这是失败**，不许报成功；
  //  · 非数字（旧包、测试替身）→ 沿用旧文案，不因拿不到数字就判失败。
  const written = typeof outcome.data === 'number' && Number.isFinite(outcome.data)
    ? outcome.data
    : null;
  if (written === 0) {
    deps.toast('error', EXPORT_EMPTY_KEY);
    return { invoked: true, cleared: false, bytes: 0 };
  }
  if (written !== null) {
    deps.toast('success', EXPORT_OK_WITH_SIZE_KEY, { size: formatBytes(written) });
    return { invoked: true, cleared: true, bytes: written };
  }
  deps.toast('success', EXPORT_OK_KEY);
  return { invoked: true, cleared: true, bytes: null };
}
