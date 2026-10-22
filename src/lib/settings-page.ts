// 设置页四块「旁路数据」的加载与失败隔离（D-73 / D-76）。
//
// 缺陷回顾（主人 1.2.6 真机反馈，一处根因、四处症状）：
// `Settings.svelte` 的 `$effect` 第一行曾用 `structuredClone` 深拷 `settings.value`，
// 而 `settings.value` 是 Svelte 5 的**深层响应式 Proxy** —— HTML 结构化克隆算法
// 遇到 Proxy 必抛 `DataCloneError`（实测：把 `new Proxy({a:1},{})` 交给该 API，
// 报 `DataCloneError #<Object> could not be cloned.`）。
//
// 于是 effect **在第一行就中断**，后面三块从不执行，用户看到：
//   · 「保险库文件」卡片只有标题、下面全空；
//   · 「关于 → 版本」显示 `—`；
//   · 自动锁定下拉恒为默认档（1 或 15 分钟都显示 5 分钟）—— **这条主人还没发现**。
//
// 注：全仓已禁用该 API（`src/lib/state-clone-guard.test.ts` 守门），深拷 `$state`
// 请用 `$state.snapshot()`。本注释刻意不写出调用形态，否则会命中那条守卫的正则。
//
// 真正的教训不是「用错了一个 API」，而是：**一个 effect 里塞了三件互不相关的事，
// 第一件炸了后面全哑；而每块的 `.catch(() => null)` 又把失败吞成「空状态」**，
// 于是「读失败」与「本来就没有」在界面上长得一模一样（D-27 同源）。
//
// 所以本模块做两件事：
//   1. 四个 load **各自** try/catch —— 任一失败不影响其余三项；
//   2. 失败**显式建模**（`fileInfoFailed` / `autoLockFailed` / `compactionStatsFailed`），让 UI 能画出
//      「读取失败」而不是留白。
//
// 纯逻辑、依赖注入、无 Tauri / window 访问，Node 测试环境可直接跑。

import type { CompactionStatsDto, VaultFileInfoDto } from '../types';

/** 四块旁路数据源（由组件注入真实 IPC，测试注入替身）。 */
export interface SettingsPageSideDeps {
  /** 库文件路径与大小（`api.system.vaultFileInfo`）。 */
  loadVaultFileInfo: () => Promise<VaultFileInfoDto>;
  /** 自动锁定档位（分钟，`api.vault.getAutoLock`，D-61）。 */
  loadAutoLock: () => Promise<number>;
  /** 应用版本号（`getVersion`，D-63；禁硬编码）。 */
  loadVersion: () => Promise<string>;
  /**
   * 保险库体积构成（`api.system.vaultCompactionStats`，D-76；**只读、廉价**）。
   *
   * 作为第四块并入，同样逐块隔离失败 —— 它失败只影响「已删附件占 / 可回收 / 压缩按钮」，
   * 不影响大小、档位、版本号。
   */
  loadCompactionStats: () => Promise<CompactionStatsDto>;
}

/** 四块旁路数据的加载结果。 */
export interface SettingsPageSideState {
  /** 库文件信息；`null` = 未取到（此时看 `fileInfoFailed` 区分「失败」与「尚未返回」）。 */
  fileInfo: VaultFileInfoDto | null;
  /** `true` = 读取**失败**（UI 必须显示「读取失败」，**不得**渲染成空白卡片）。 */
  fileInfoFailed: boolean;
  /** 自动锁定档位；失败时为传入的 `fallbackAutoLock`（保持旧行为：不阻塞页面）。 */
  autoLockMinutes: number | null;
  /** `true` = 档位读取失败，当前显示的是兜底档而非库里真实档位。 */
  autoLockFailed: boolean;
  /** 版本号；`null` = 读取失败（UI 显示 `—`）。 */
  version: string | null;
  /** 体积构成；`null` = 未取到（看 `compactionStatsFailed` 区分失败与未返回）。 */
  compactionStats: CompactionStatsDto | null;
  /** `true` = 体积构成读取失败（或数值不可用）⇒ **不许**据此渲染压缩按钮。 */
  compactionStatsFailed: boolean;
}

/**
 * 判断体积构成里的数值是否**可用于决策**。
 *
 * 为什么要单独判：`reclaimable_bytes` 若为 `NaN` / 负数，那么
 * `reclaimable_bytes > 0` 为 false → UI 会渲染成「没有可回收的空间」，
 * 而事实是「我们根本不知道可回收多少」—— 失败被伪装成结论（D-27 / D-73 同族）。
 *
 * 只校验**参与判断与展示**的四个字段：`file_bytes`（当前大小已另有来源，这里只做兜底）、
 * `attachment_dead_bytes`、`reclaimable_bytes`、`after_bytes`（确认块要显示「压缩后约」）。
 *
 * @param stats 后端返回的体积构成
 * @returns 四个关键字段都是**有限的非负数**时为 `true`
 */
function isUsableCompactionStats(stats: CompactionStatsDto): boolean {
  const keys: Array<keyof CompactionStatsDto> = [
    'file_bytes',
    'attachment_dead_bytes',
    'reclaimable_bytes',
    'after_bytes',
  ];
  return keys.every((key) => {
    const value = stats[key];
    return Number.isFinite(value) && value >= 0;
  });
}

/**
 * 加载设置页的四块旁路数据，逐块隔离失败。
 *
 * 语义（与 D-73 / D-76 的验收口径逐条对应）：
 * 1. **各自** try/catch：任一 reject 不影响另外三项 —— 这是本缺陷的核心回归点；
 * 2. `loadVaultFileInfo` 失败 → `fileInfo: null` + `fileInfoFailed: true`；
 * 3. `loadAutoLock` 失败（或返回非有限数）→ `autoLockMinutes: fallbackAutoLock`
 *    + `autoLockFailed: true`；
 * 4. `loadVersion` 失败 → `version: null`；
 * 5. `loadCompactionStats` 失败（或关键字段非有限/负数）→ `compactionStats: null`
 *    + `compactionStatsFailed: true`；
 * 6. 成功路径上四个失败标记必须是 `false`。
 *
 * 为什么写成**串行**而不是 `Promise.all`：`Promise.all` 的语义是「一个 reject 全丢」，
 * 正是本缺陷要消灭的形状。改成并行需要先把每个 promise 归一成「永不 reject」再 all
 * （即 `allSettled` 的手工版），多一层包装反而更难审；本页只在进入设置时跑一次，
 * 四次往返的延迟换可读性是划算的。若将来这里变热，再换 `allSettled` 并保留同样的
 * 「逐块建模失败」语义——**换成并行不是问题，丢掉失败建模才是**。
 *
 * ⚠️ `loadAutoLock` 额外挡了「非有限数」：后端若回 `NaN` / `Infinity`，
 * `<select>` 里没有任何 option 与之相等，会渲染成一个**没选中项的下拉**——
 * 那又是「看起来正常、其实不对」的一种。宁可判成失败走兜底 + 显式标记。
 * `loadCompactionStats` 同理（见 `isUsableCompactionStats` 的注释）。
 *
 * @param deps 四个数据源。
 * @param fallbackAutoLock 档位读取失败时使用的兜底值（调用点传 `AUTO_LOCK_OPTIONS[1]`）。
 * @returns 四块数据的值 + 失败标记。
 */
export async function loadSettingsPageSide(
  deps: SettingsPageSideDeps,
  fallbackAutoLock: number,
): Promise<SettingsPageSideState> {
  // ① 库文件信息
  let fileInfo: VaultFileInfoDto | null = null;
  let fileInfoFailed = false;
  try {
    fileInfo = await deps.loadVaultFileInfo();
  } catch {
    fileInfo = null;
    fileInfoFailed = true;
  }

  // ② 自动锁定档位
  let autoLockMinutes: number | null = fallbackAutoLock;
  let autoLockFailed = false;
  try {
    const minutes = await deps.loadAutoLock();
    if (Number.isFinite(minutes)) {
      autoLockMinutes = minutes;
    } else {
      autoLockMinutes = fallbackAutoLock;
      autoLockFailed = true;
    }
  } catch {
    autoLockMinutes = fallbackAutoLock;
    autoLockFailed = true;
  }

  // ③ 版本号
  let version: string | null = null;
  try {
    version = await deps.loadVersion();
  } catch {
    version = null;
  }

  // ④ 体积构成（D-76）
  let compactionStats: CompactionStatsDto | null = null;
  let compactionStatsFailed = false;
  try {
    const stats = await deps.loadCompactionStats();
    if (isUsableCompactionStats(stats)) {
      compactionStats = stats;
    } else {
      compactionStats = null;
      compactionStatsFailed = true;
    }
  } catch {
    compactionStats = null;
    compactionStatsFailed = true;
  }

  return {
    fileInfo,
    fileInfoFailed,
    autoLockMinutes,
    autoLockFailed,
    version,
    compactionStats,
    compactionStatsFailed,
  };
}
