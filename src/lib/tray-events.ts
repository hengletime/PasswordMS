// 宿主（Rust）→ 前端的事件名（v1.4.10）。
//
// ⚠️ 与 `src-tauri/src/app_events.rs` 是**跨语言字面值契约**：两边写不一样不会有任何
// 编译错误，只会静默失效（D-118 踩过一次，D-123 又踩过一次「挂错层」）。
// `src/lib/ipc-contract.test.ts` 逐字比对两边。

/** 首次点窗口 X 的一次性提示：程序仍在托盘运行（窗口保持可见）。 */
export const TRAY_HINT_EVENT = 'app://tray-hint';

/** 全局热键被按下 → 把焦点打到搜索框（唤窗动作已由 Rust 完成）。 */
export const QUICK_SEARCH_EVENT = 'app://quick-search';
