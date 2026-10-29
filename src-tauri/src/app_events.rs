//! 后端 → 前端的自定义事件名（集中定义，避免散落字面量）。
//!
//! **每个常量都和前端的字符串字面值是跨语言契约**：两边写不一样不会有任何编译错误，
//! 只会静默失效（D-118 踩过、D-123 又踩过一次「挂错层」）。所以：
//!
//! * 后端一律引用本模块，不写字面量；
//! * `src/lib/ipc-contract.test.ts` 逐字比对这里的值与前端 TS 常量。

/// 自动锁定已发生（`state.rs` 的兜底 worker 与托盘「锁定」共用）。
///
/// 前端收到后复位各 store 并切到锁屏；payload 只是原因标记（`"idle"` / `"manual"`），
/// 前端当前不读——不要依赖它做判断。
pub const AUTOLOCK_LOCKED_EVENT: &str = "autolock://locked";

/// 首次点窗口 X 的一次性教育提示（窗口保持可见，见 `tray.rs`）。
pub const TRAY_HINT_EVENT: &str = "app://tray-hint";

/// 全局热键被按下 → 前端把焦点打到搜索框（v1.4.10）。
///
/// 事件只负责"聚焦"，唤出窗口由 Rust 侧完成（`unminimize + show + set_focus`）——
/// 前端拿不到窗口 API 的正确时序，等 JS 收到事件再唤窗会慢半拍且可能抢焦点失败。
pub const QUICK_SEARCH_EVENT: &str = "app://quick-search";
