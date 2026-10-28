// D-123 / D-124：宿主「回到前台」通知（移动端）。
//
// 为什么需要它：设置页本来就挂了 `window.addEventListener('focus', …)` 去重查快速解锁
// 状态（D-114），那套在 **Windows** 上成立——窗口真的丢焦点（切到系统设置）又拿回来。
// 但安卓不是：Activity 进后台再回前台时，WebView 里的 `document` 从未失去焦点，
// `focus` 事件不触发 ⇒ `refreshBiometric()` 不跑，卡片停在旧值。真机表现就是
// 「按指引去系统设置加了指纹，切回来什么变化都没有」。
//
// 移动端生命周期只有宿主知道。D-123 先接的是 `tauri::RunEvent::Resumed`——**那条在安卓
// 上从不触发**（上游只在桌面事件循环的 `StartCause::Poll` 里发它），于是白改一版；
// D-124 改接窗口事件 `RunEvent::WindowEvent { event: WindowEvent::Resumed }`
// （`src-tauri/src/lib.rs`），并**兼听**上游自己转发给 JS 的同义事件，两条链路互为兜底。

/** Rust 侧在 `RunEvent::WindowEvent { event: WindowEvent::Resumed }` 里发出的事件名。 */
export const APP_RESUMED_EVENT = 'app://resumed';

/**
 * 上游（tauri）自己转发给 JS 的同义事件名。
 *
 * `tauri/src/manager/window.rs` 收到 `WindowEvent::Resumed` 时就会
 * `emit_to_window("tauri://resumed")`，和上面那条**同一个源事件、同一个 dispatch**，
 * 所以它并非「更可靠」，只是**少经过我们一段代码**：万一我们那个 match 分支哪天
 * 被改坏（D-123 就是这么坏的），这条还能把功能托住。反过来，上游若改了字面量，
 * 我们的 `app://resumed` 仍然有效。
 *
 * 两条同时到达 → [`createResumeGate`] 合并成一次查询，不会打两次 IPC。
 */
export const TAURI_RESUMED_EVENT = 'tauri://resumed';

/**
 * 需要监听的「回前台」事件名（两条链路）。
 *
 * 与 `src-tauri/src/lib.rs` 的 `APP_RESUMED_EVENT` 是**跨语言字面值**：写错不会报错，
 * 只会静默失效（D-118 同类）。`bio-android-guard.test.ts` 逐字比对两侧。
 */
export const RESUME_EVENTS: readonly string[] = [APP_RESUMED_EVENT, TAURI_RESUMED_EVENT];

/**
 * 是否跑在 Tauri WebView 里（Node 测试环境没有 `@tauri-apps/api` 的宿主）。
 *
 * 与 `App.svelte` 里注册 auto-lock 监听用的判据一致——测试环境直接跳过注册，
 * 不要让 `listen()` 在没有宿主时抛错。
 */
export function hasTauriHost(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * 回前台时该不该重查快速解锁状态。
 *
 * 两个不该动的时候：
 * - 正在解锁/正在开关（`busy`）——此时重查会把按钮状态与进行中的操作搅在一起；
 * - 还在「创建保险库」向导里（没有库，`biometric_status` 必然失败，重查纯噪音）。
 *
 * 抽成纯函数是为了能被行为级单测覆盖，而不是只靠 grep 源码。
 */
export function shouldRefreshOnResume(input: {
  busy: boolean;
  route: string;
}): boolean {
  return !input.busy && input.route !== 'create';
}

/**
 * 把「一次回前台」可能来的多次通知合并成一次查询。
 *
 * 为什么需要：同一秒内两条链路（宿主的 `app://resumed` + 上游的 `tauri://resumed`）
 * 会各触发一次监听器；不合并就会并发发两条 `biometric_status`。查询本身幂等，
 * 但两次回复的到达顺序不保证，晚到的旧值会盖掉新值——这种抖动在「刚录完指纹」的
 * 场景里恰好最刺眼。
 *
 * 合并口径是**在途合并**，不是节流：查询还没回来就再来一次，直接丢掉；回来了再来，
 * 正常再查一次（真的两次切回前台，就该查两次）。
 */
export function createResumeGate(refresh: () => Promise<void>): {
  trigger: () => void;
  isIdle: () => boolean;
} {
  let inFlight = false;
  return {
    trigger: (): void => {
      if (inFlight) return;
      inFlight = true;
      void refresh().finally(() => {
        inFlight = false;
      });
    },
    isIdle: (): boolean => !inFlight,
  };
}
