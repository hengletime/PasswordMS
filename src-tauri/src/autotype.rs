//! 自动输入（Auto-Type）条目选择浮层（1.5.39，仅桌面）。
//!
//! ## 与快速搜索小窗（`quick_search.rs`）的分工
//!
//! 两者都是「热键唤出、用完就走」的第二 WebView 浮层，骨架完全同源（ensure/show/
//! toggle/失焦即收起）。区别在**闭环动作**：quick 小窗把密码复制进剪贴板，由用户
//! 自己粘贴；本浮层把选中条目交给 [`crate::commands::autotype_cmd::auto_type_fill`]，
//! 由 Rust 切回原焦点窗口用 `SendInput` 代打。
//!
//! ## ⚠️ 三处字面值契约（必须逐字一致，不一致没有任何报错、只静默失效）
//!
//! 1. 本模块 [`LABEL`] = `"autotype"`（`lib.rs` 的 `on_window_event` 按它匹配）；
//! 2. `capabilities/autotype.json` 的 `windows` 字段 = `["autotype"]`
//!    （窗口拿不到任何权限，浮层前端的 `listen` / `hide` 全部静默失败）；
//! 3. URL 参数 `?w=autotype`（前端 `main.ts` 按它分流到浮层入口）。
//!
//! `quick_search.rs:38-39` 的教训：这三处各写各的，编译器不会说一个字。
//!
//! ## 为什么启动时就建好
//!
//! 与 quick 小窗同一理由（见其模块注释）：建 WebView 要几百毫秒，而热键最需要
//! 立刻回应的时刻是刚开机主窗口还缩在托盘时。

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::app_events::AUTOTYPE_PICK_EVENT;

/// 窗口 label。**三处字面值契约之一**，见模块文档。
pub const LABEL: &str = "autotype";

/// 逻辑像素尺寸（与 quick 小窗同规格，前端按这个尺寸设计）。
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 520.0;

/// 取窗口；不存在就建一个（隐藏态）。Alt+F4 关掉后由这里重建。
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Ok(w);
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?w=autotype".into()))
        .title("PasswordMS")
        .inner_size(WIDTH, HEIGHT)
        // 与 quick 小窗同款：无边框 + 置顶 + 不进任务栏的"用完就走"浮层。
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .shadow(true)
        .build()
}

/// 唤出浮层（含定位到鼠标所在屏 + 聚焦 + 通知前端备好界面）。
pub fn show(app: &AppHandle) {
    let window = match ensure(app) {
        Ok(w) => w,
        Err(e) => {
            crate::state::append_log(app, &format!("[autotype] 创建浮层失败：{e}"));
            return;
        }
    };
    if let Some(pos) = center_on_cursor(app) {
        let _ = window.set_position(pos);
    }
    let _ = window.show();
    let _ = window.set_focus();
    // 页面里的输入框还没拿到焦点：前端收到事件才清空上次关键词、加载条目清单。
    let _ = app.emit(AUTOTYPE_PICK_EVENT, ());
}

/// 收起浮层。窗口不销毁，下次 `show` 零等待。
pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// 注入失败后把浮层**重新显示**出来（1.5.40）。
///
/// 为什么不能直接复用 [`show`]：`show` 会 emit [`AUTOTYPE_PICK_EVENT`]，前端收到就
/// 清空搜索词、把选中项归零——而失败之后用户多半想**换个动作重试同一条**，把选择
/// 清掉等于逼他重搜一遍。这里只把窗口放回前台，界面状态原样保留。
///
/// 为什么必须由 Rust 做：注入前浮层已被 [`hide`]（不藏的话前台就是浮层自己，
/// `SetForegroundWindow` 切不回目标窗口），前端的失败文案于是写在一个**已隐藏的
/// 窗口**里。1.5.39 真机验收的症状正是这个：在管理员权限程序里按热键选条目，
/// 「按了没反应，也没提示」——错误其实产生了（`ERR_AUTOTYPE_BLOCKED`），只是没人
/// 看得见。
pub fn reveal(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 热键语义：可见则收起，不可见则唤出（与 quick 小窗同款开关键）。
pub fn toggle(app: &AppHandle) {
    let visible = app
        .get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide(app);
    } else {
        show(app);
    }
}

/// 按**鼠标所在的那块屏幕**居中（同 `quick_search::center_on_cursor`）。
fn center_on_cursor(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let main = app.get_webview_window("main")?;
    let cursor = main.cursor_position().ok()?;
    let monitor = main.monitor_from_point(cursor.x, cursor.y).ok()??;

    let origin = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();

    // 逻辑像素 × 缩放系数 = 物理像素（不换算会在 125%/150% 缩放屏上偏移）。
    let w = WIDTH * scale;
    let h = HEIGHT * scale;

    let x = origin.x + ((f64::from(size.width) - w) / 2.0).round() as i32;
    let y = origin.y + ((f64::from(size.height) - h) / 2.0).round() as i32;
    Some(PhysicalPosition::new(x, y))
}
