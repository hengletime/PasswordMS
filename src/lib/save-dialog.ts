// 「选目标 + 写盘」的取消/失败归一（D-71 / D-72）。
//
// 缺陷回顾（三处同源、三种错法，全在一条流程上）：
//   ① `vault-export.ts` 把 `pickDestination()` 放在 try **外面** → Android 取消时 reject
//      直接逃出函数，`Settings.svelte` 的 `await runVaultExport(...)` 没有 try/catch →
//      **未捕获 rejection**，用户点取消毫无反应（控制台才有红字）；
//   ② `AttachmentView.svelte` 的 `exportAttachment` 用 try 同时包住「选目标 + 写盘」→
//      **用户点取消会弹一条红「内部错误」**（把用户主动放弃当成故障）；
//   ③ `ImportWizard.svelte` 的 `downloadTemplate` 用空 catch 吞一切 → 取消是对的，
//      但**真实失败（写盘失败 / invalid args / 命令未注册）也静默**，即
//      `docs/03-impl.md` §14.3 说的「点了没反应、无堆栈」。
//
// 根因不在某处 catch 写错，而在 `attachment-import.ts` 那条约定：
// 「Android 取消是 reject，由注入方自行 try/catch 归一成 null」——
// **归一动作被外包给了每个调用点**，漏一处就出一个洞，现在漏了三处。
//
// 本模块把那件事收进来一次：**try 里只放对话框调用**。
//   · 对话框抛异常 → 用户取消（主动行为，不是错误）；
//   · `write` 抛异常 → 真实失败（必须冒出来）。
//
// 这样不必赌插件取消时到底是 `reject` 还是 `resolve(null)`：两条路都判成取消。
// 赌平台行为是这三处缺陷的共同成因，不是偶然。

import { toAppError } from './errors';

/** 「选目标 + 写盘」三种结局。调用点按 `kind` 分流，不再各自写 catch。 */
export type SaveOutcome =
  /** 用户取消（对话框 reject / resolve null / 空串）：不写盘、不报错、不 toast。 */
  | { kind: 'cancelled' }
  /** 已写盘成功。 */
  | { kind: 'saved' }
  /** 已选到目标但写盘失败：`messageKey` 是可直接 toast 的文案键。 */
  | { kind: 'failed'; messageKey: string };

/** 归一流程所需的外部依赖（组件注入真实实现，测试注入替身）。 */
export interface SaveDialogDeps {
  /**
   * 系统保存对话框。
   *
   * resolve `null` / reject **均**视为用户取消 —— 桌面插件 resolve null，
   * Android 插件取消时走 reject（`DialogPlugin.kt` 的 `saveFileDialog` 基于
   * `ACTION_CREATE_DOCUMENT`，回调经异常/URI 两条路返回）。这里不要求调用点归一，
   * 直接两条都吃。
   */
  pickDestination: () => Promise<string | null>;
  /**
   * 真正写盘（IPC 或 fs）。
   *
   * 合同：**跑到这里就说明用户已经选定了目标**，所以此处抛错一定是真实失败，
   * 必须冒到调用点。不要在 `write` 里再吞异常 —— 那会让本模块的 `failed` 分支永远走不到。
   */
  write: (dest: string) => Promise<unknown>;
}

/**
 * 执行「选目标 + 写盘」，并把三种平台差异归一成 {@link SaveOutcome}。
 *
 * 行为契约：
 * 1. `pickDestination` **抛异常** → `cancelled`，`write` **一次都不调用**；
 * 2. `pickDestination` resolve `null` → `cancelled`，`write` 未调用；
 * 3. `pickDestination` resolve **空串**（或非字符串）→ `cancelled`，`write` 未调用
 *    —— 空串也是「没选到位置」，与 null 同义；
 * 4. `write` 抛异常 → `failed`（`messageKey` 经 `toAppError` 归一），**不得静默**；
 * 5. `write` 正常返回 → `saved`。
 *
 * 本函数**不 toast、不写日志、不做 UUID/路径处理**：文案与出口留在调用点，
 * 以便三处调用各自保留自己的成功文案（`common.ok` / `import.template_saved`）。
 * 纯逻辑、无 Tauri / window 访问，Node 测试环境可直接跑。
 *
 * @param deps 对话框与写盘依赖。
 * @returns 归一后的结局。
 */
export async function runSaveToDestination(deps: SaveDialogDeps): Promise<SaveOutcome> {
  let dest: string | null;
  try {
    dest = await deps.pickDestination();
  } catch {
    // 唯一被当作「取消」的异常来源。这里**不区分错误内容**：对话框这一层能抛出的
    // 只有取消与平台不可用，二者对用户的正确反应都是「什么都不做」。
    // 真失败发生在 write 阶段，那里会如实上报。
    return { kind: 'cancelled' };
  }
  if (typeof dest !== 'string' || dest.length === 0) {
    // 空串与 null 同义。⚠️ 不要退化成 `dest === null` 判断：桌面插件在某些平台
    // 会返回空串，那会让流程继续走到 write("")，报一条莫名其妙的写盘失败。
    return { kind: 'cancelled' };
  }
  try {
    await deps.write(dest);
    return { kind: 'saved' };
  } catch (err) {
    return { kind: 'failed', messageKey: toAppError(err).messageKey };
  }
}
