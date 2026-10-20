// 自动锁定计时（D-61 A 案）：把「闲置达档位 → 锁定」的执行层抽成依赖注入的纯逻辑层，
// 照 vault-export.ts / attachment-import.ts 先例——不做任何 window / tauri 全局访问，
// vitest 注入假时钟即可覆盖全部分支。组件层（MainLayout）负责注入真实时钟/事件。
//
// 行为契约：
// 1. `start()`：读取档位（分钟）→ 以当前时刻为闲置起点 → 安排 `lock` 于 `start 时刻 + 档位`；
//    档位读取失败（IPC 异常 / 非正数）时**保守回退 5 分钟**（AUTO_LOCK_MINUTES 默认档）——
//    安全功能宁可锁早不可锁晚，也不可锁不了；
// 2. `touch()`：任意用户输入（pointerdown/keydown）→ 重置闲置起点并重排计时
//    （`stop()` 之后 touch 不得复活计时器）；
// 3. `onReturnFromBackground(hiddenAtMs)`：Android 后台会冻结 JS 计时器，回前台补判——
//    `now - hiddenAt >= 档位` → 立即 `lock()`；未超档 → 按真实时钟重排剩余时间
//    （后台停留时间计入闲置：真机上后台 4 分钟 + 前台 1 分钟 = 5 分钟到期）；
// 4. `stop()`：取消计时，之后不再触发 `lock`（锁定动作本身由组件层的 lockNow 统一执行，
//    会 reset 三个 store 并切回锁屏路由）。
//
// 档位语义：无「从不」档（安全需求裁定，主人拍板 A 案）；合法档位 1/5/15 由 core 层
// `set_auto_lock_minutes` 校验并持久化于库明文头部。

/** 自动锁定计时所需的外部依赖（组件注入真实实现，测试注入假时钟）。 */
export interface AutoLockDeps {
  /** 读取当前档位（分钟）。对应 `api.vault.getAutoLock()`。 */
  getMinutes: () => Promise<number>;
  /** 锁定动作。对应组件层 `lockNow()`（session.lock + store reset + 切锁屏路由）。 */
  lock: () => Promise<void>;
  /** 时钟（毫秒）。测试注入假时钟。 */
  now: () => number;
  /**
   * 定时器：`ms` 毫秒后执行 `fn`，返回**取消函数**。
   * 组件层注入 `setTimeout/clearTimeout` 包装；测试注入假时钟队列。
   */
  schedule: (fn: () => void, ms: number) => () => void;
}

/** 自动锁定控制器。 */
export interface AutoLockController {
  /** 启动（解锁进入主界面时调用；重复调用以新档位重开）。 */
  start(): Promise<void>;
  /** 用户输入（pointerdown/keydown）→ 重置闲置计时。 */
  touch(): void;
  /** 从后台回到前台（Android 补判）：超档立即锁，未超档重排剩余时间。 */
  onReturnFromBackground(hiddenAtMs: number): Promise<void>;
  /** 停止（锁定 / 离开主界面时调用）。 */
  stop(): void;
}

/** 档位读取失败的保守回退值（= core 层 AUTO_LOCK_MINUTES 默认档）。 */
export const AUTO_LOCK_FALLBACK_MINUTES = 5;

/** 创建自动锁定控制器。 */
export function createAutoLock(deps: AutoLockDeps): AutoLockController {
  let minutes = AUTO_LOCK_FALLBACK_MINUTES;
  let lastActivity = 0;
  let cancelTimer: (() => void) | null = null;
  let running = false;

  /** 按「档位 -（now - lastActivity）」重排剩余时间；已到点则立即触发。 */
  function reschedule(): void {
    if (cancelTimer !== null) {
      cancelTimer();
      cancelTimer = null;
    }
    if (!running) {
      return;
    }
    const windowMs = minutes * 60_000;
    const remaining = windowMs - (deps.now() - lastActivity);
    if (remaining <= 0) {
      // 闲置已超档（如后台补判后的重排）：立即锁定。
      void deps.lock();
      return;
    }
    cancelTimer = deps.schedule((): void => {
      cancelTimer = null;
      void deps.lock();
    }, remaining);
  }

  return {
    async start(): Promise<void> {
      try {
        const value = await deps.getMinutes();
        minutes = Number.isFinite(value) && value > 0 ? value : AUTO_LOCK_FALLBACK_MINUTES;
      } catch {
        // 档位读取失败：保守回退 5 分钟（宁可锁早，不可锁不了）。
        minutes = AUTO_LOCK_FALLBACK_MINUTES;
      }
      running = true;
      lastActivity = deps.now();
      reschedule();
    },
    touch(): void {
      if (!running) {
        return; // stop() 后 touch 不得复活计时器。
      }
      lastActivity = deps.now();
      reschedule();
    },
    async onReturnFromBackground(hiddenAtMs: number): Promise<void> {
      if (!running) {
        return;
      }
      if (deps.now() - hiddenAtMs >= minutes * 60_000) {
        // 后台冻结期间已超档：立即锁定（Android 计时器冻结的补判）。
        running = false;
        if (cancelTimer !== null) {
          cancelTimer();
          cancelTimer = null;
        }
        await deps.lock();
        return;
      }
      // 未超档：后台停留时间计入闲置，按真实时钟重排剩余时间。
      reschedule();
    },
    stop(): void {
      running = false;
      if (cancelTimer !== null) {
        cancelTimer();
        cancelTimer = null;
      }
    },
  };
}
