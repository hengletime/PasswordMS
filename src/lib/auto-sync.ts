// 同步自动化（D-129）：把「启动时同步 / 定时自动同步」两个开关的执行层抽成依赖注入的
// 纯逻辑层，照 auto-lock.ts / vault-export.ts 先例——不做任何 window / tauri 全局访问，
// vitest 注入假时钟即可覆盖全部分支。组件层（MainLayout）负责注入真实时钟与同步动作。
//
// 背景（为什么必须有这个文件）：`VaultSettings.auto_sync` / `sync_on_start` 两个字段
// 自 v1.0 起就在设置页有复选框、能存能读，但**全仓没有任何一处读它们来做事**
// （`crates/passwordms-core/src/model/vault.rs:125,127` 只有定义与默认值）。用户勾上
// 「自动同步」，保存成功、界面显示已勾选，然后什么都不发生——他于是不再手动点「立即同步」，
// 两端数据静默不同步。这不是少一个功能，是 UI 替不存在的功能做了承诺。
//
// 行为契约：
// 1. `start()`：解锁进入主界面、**五路数据加载完成之后**调用。先判「启动时同步」，
//    命中则立刻跑一次；随后挂上心跳。重复调用**幂等**（同一次解锁不重复跑启动同步）。
// 2. 心跳周期 `AUTO_SYNC_TICK_MS`（60 秒）而非直接排「间隔时长」：定时器在后台/隐藏窗口
//    会被浏览器节流到分钟级，排长定时器会累积漂移；改用心跳 + **真实时钟判到期**，
//    节流最坏只是「晚一分钟」，不会越跑越偏。
// 3. 到期判定走纯函数 `isAutoSyncDue`，四条否决理由各自独立：开关关、无配对设备、
//    手动同步在飞（不插队）、未到下次允许时刻。
// 4. 跑完按结果排下次：成功 `+AUTO_SYNC_INTERVAL_MS`，失败 `+AUTO_SYNC_FAIL_BACKOFF_MS`。
//    对端离线是自动同步的**常态**（笔记本合上、手机不在同一局域网），退避是为了别每
//    五分钟对着一个不在线的地址敲门。
// 5. `stop()`：锁定 / 离开主界面时调用，取消心跳并清空「本会话已跑过」标记。

/** 一次到期判定的输入（纯数据，便于单测逐条钉住）。 */
export interface AutoSyncGate {
  /** 该触发类型是否已开启（启动时同步 / 定时同步各判一次）。 */
  enabled: boolean;
  /** 已配对设备数。 */
  deviceCount: number;
  /** 是否已有同步在飞（手动「立即同步」占用中）。 */
  busy: boolean;
  /** 当前时刻（毫秒）。 */
  now: number;
  /** 下次允许触发的时刻；`null` = 本次解锁还没跑过，随时可跑。 */
  dueAt: number | null;
}

/**
 * 是否应当现在跑一次自动同步。
 *
 * 四条否决理由都必须显式写出来，而不是合成一个 `a && b && c` —— 合成后
 * 「为什么没同步」在日志与测试里都看不出来，而这正是本次缺陷的成因：
 * 两个开关连一个消费点都没有，所以「没同步」根本无处可说。
 */
export function isAutoSyncDue(gate: AutoSyncGate): boolean {
  if (!gate.enabled) {
    return false; // 开关没开
  }
  if (gate.deviceCount === 0) {
    return false; // 一台设备都没配对，没有同步的对象
  }
  if (gate.busy) {
    return false; // 手动同步正在进行，不插队
  }
  if (gate.dueAt === null) {
    return true; // 本次解锁还没跑过
  }
  return gate.now >= gate.dueAt;
}

/** 定时同步间隔：5 分钟。PRD 只要求「有节流策略」，此处取常见口令管理器量级。 */
export const AUTO_SYNC_INTERVAL_MS = 5 * 60_000;
/** 失败退避：15 分钟（对端离线时别每 5 分钟敲门）。 */
export const AUTO_SYNC_FAIL_BACKOFF_MS = 15 * 60_000;
/** 心跳周期：60 秒（只做「要不要跑」的判定，不承担计时精度）。 */
export const AUTO_SYNC_TICK_MS = 60_000;
/** 两次自动同步之间的最小间隔，防 $effect 重跑导致的连击。 */
export const AUTO_SYNC_MIN_GAP_MS = 60_000;

/** 自动同步所需的外部依赖（组件注入真实实现，测试注入假时钟）。 */
export interface AutoSyncDeps {
  /** 读当前已保存的设置（**不是**编辑中的草稿）。 */
  getSettings: () => { auto_sync: boolean; sync_on_start: boolean };
  /** 已配对设备数。 */
  getDeviceCount: () => number;
  /** 是否有同步在飞（含手动与上一次自动）。 */
  isBusy: () => boolean;
  /**
   * 执行一次**静默**同步（不弹 toast），返回「是否至少一台设备成功」。
   * 实现方保证不抛异常（对端离线属常态），抛了也按失败处理。
   */
  runOnce: () => Promise<boolean>;
  /** 时钟（毫秒）。测试注入假时钟。 */
  now: () => number;
  /** 定时器：`ms` 毫秒后执行 `fn`，返回**取消函数**。 */
  schedule: (fn: () => void, ms: number) => () => void;
}

/** 自动同步控制器。 */
export interface AutoSyncController {
  /** 启动（解锁且数据加载完成后调用；同一次解锁内幂等）。 */
  start(): Promise<void>;
  /** 停止（锁定 / 离开主界面时调用）。 */
  stop(): void;
  /** 下次允许触发的时刻（诊断与测试用，`null` = 随时可跑）。 */
  nextDueAt(): number | null;
}

/** 创建自动同步控制器。 */
export function createAutoSync(deps: AutoSyncDeps): AutoSyncController {
  /** 本会话是否已经跑过启动同步（防 `start()` 重入导致重复同步）。 */
  let startedThisSession = false;
  let running = false;
  let nextDue: number | null = null;
  let cancelTick: (() => void) | null = null;

  function clearTick(): void {
    if (cancelTick !== null) {
      cancelTick();
      cancelTick = null;
    }
  }

  /** 挂下一拍心跳；无论上一拍成功失败都续上。 */
  function armTick(): void {
    clearTick();
    if (!running) {
      return;
    }
    cancelTick = deps.schedule((): void => {
      cancelTick = null;
      void tick();
    }, AUTO_SYNC_TICK_MS);
  }

  /** 跑一次并按结果排下次允许时刻。 */
  async function runOnce(): Promise<boolean> {
    const startedAt = deps.now();
    let ok = false;
    try {
      ok = await deps.runOnce();
    } catch {
      ok = false; // 静默同步不该抛；真抛了也只当失败，不让异常打断心跳
    }
    const wait = ok ? AUTO_SYNC_INTERVAL_MS : AUTO_SYNC_FAIL_BACKOFF_MS;
    const gap = wait > AUTO_SYNC_MIN_GAP_MS ? wait : AUTO_SYNC_MIN_GAP_MS;
    // 从「开跑时刻」起算而不是「跑完时刻」：慢速同步（大库）不该把间隔越推越后。
    nextDue = startedAt + gap;
    return ok;
  }

  async function tick(): Promise<void> {
    if (!running) {
      return;
    }
    try {
      const s = deps.getSettings();
      if (
        isAutoSyncDue({
          enabled: s.auto_sync,
          deviceCount: deps.getDeviceCount(),
          busy: deps.isBusy(),
          now: deps.now(),
          dueAt: nextDue,
        })
      ) {
        await runOnce();
      }
    } finally {
      armTick();
    }
  }

  return {
    async start(): Promise<void> {
      if (startedThisSession) {
        // 同一次解锁内重复调用：只确保心跳在跑，不重复同步。
        running = true;
        armTick();
        return;
      }
      startedThisSession = true;
      running = true;
      nextDue = null;

      const s = deps.getSettings();
      const dueNow = isAutoSyncDue({
        enabled: s.sync_on_start,
        deviceCount: deps.getDeviceCount(),
        busy: deps.isBusy(),
        now: deps.now(),
        dueAt: null,
      });
      if (dueNow) {
        await runOnce();
      }
      armTick();
    },
    stop(): void {
      running = false;
      startedThisSession = false;
      nextDue = null;
      clearTick();
    },
    nextDueAt(): number | null {
      return nextDue;
    },
  };
}
