// D-123：宿主「回到前台」通知（移动端）。
//
// 为什么需要它：设置页本来就挂了 `window.addEventListener('focus', …)` 去重查快速解锁
// 状态（D-114），那套在 **Windows** 上成立——窗口真的丢焦点（切到系统设置）又拿回来。
// 但安卓不是：Activity 进后台再回前台时，WebView 里的 `document` 从未失去焦点，
// `focus` 事件不触发 ⇒ `refreshBiometric()` 不跑，卡片停在旧值。真机表现就是
// 「按指引去系统设置加了指纹，切回来什么变化都没有」。
//
// 移动端生命周期只有宿主知道，所以由 Rust 在 `RunEvent::Resumed` 里 emit 本事件
// （`src-tauri/src/lib.rs`），前端收到后重查。

/**
 * Rust 侧 `RunEvent::Resumed` 发出的事件名。
 *
 * **与 `src-tauri/src/lib.rs` 的 `APP_RESUMED_EVENT` 是跨语言字面值**：写错不会报错，
 * 只会静默失效（D-118 同类）。`bio-android-guard.test.ts` 逐字比对两侧。
 */
export const APP_RESUMED_EVENT = 'app://resumed';

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
