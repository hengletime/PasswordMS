//! 快速搜索小窗（v1.4.11，仅桌面）。
//!
//! ## 为什么是独立窗口，而不是「唤主窗口 + 聚焦搜索框」
//!
//! v1.4.10 的做法是热键把主窗口端上来。实测下来这条路径多做了一大半无用功：
//! 主窗口是 1000×700 级的完整界面，从任何程序里按一下热键就把整块屏幕盖住；
//! 用户真正要做的只有一件事——**找到那条，把密码弄到剪贴板**。窗口越大，
//! 「挡住正在填的表单 → 复制 → 手动切回去」的来回就越烦。
//!
//! 所以这里给的是 440×520 的浮层：居中、置顶、不进任务栏、无边框，
//! 回车复制完自己消失。
//!
//! ## 它是第二个 WebView，store 不共享
//!
//! 每个 webview 有**独立的 JS 上下文**：主窗口的 `vault` / `session` store 在小窗里
//! 全是空的、也不会互相通知。所以小窗的数据一律自己走 IPC 取
//! （`list_items` → `get_item` → `copy_secret`），不依赖主窗口任何状态。
//!
//! 反过来，`localStorage` 属于**同一个 origin**、两个窗口共享——界面语言
//! （`passwordms.locale`）与主题因此能自动继承，不需要额外传参，也不会出现
//! 「主界面英文、小窗中文」这种割裂。
//!
//! ## 为什么启动时就建好（而不是第一次按热键再建）
//!
//! 建窗口要初始化一个完整的 WebView，首次有几百毫秒。而热键最需要立刻回应的时刻
//! 恰恰是**刚开机、主窗口还缩在托盘**的时候——那正是「按了没反应」最容易发生的地方
//! （同 `hotkey.rs` 把注册放后端的理由）。所以 `setup` 里就 `ensure` 一个隐藏窗口，
//! 热键只是 `show`。

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::app_events::QUICK_SEARCH_EVENT;

/// 窗口 label。
///
/// `lib.rs` 的 `on_window_event`、`capabilities/quick.json` 的 `windows` 字段
/// 都按这个字面值匹配——三处写不一致不会有任何编译错误，只会静默失效。
pub const LABEL: &str = "quick";

/// 逻辑像素尺寸（前端 CSS 按这个尺寸设计）。
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 520.0;

/// 取窗口；不存在就建一个（隐藏态）。
///
/// 「不存在」是真实会发生的：无边框窗口仍能被 Alt+F4 关掉，关掉之后
/// `get_webview_window` 返回 `None`。这里重建，而不是让热键从此失效。
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Ok(w);
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?w=quick".into()))
        .title("PasswordMS")
        .inner_size(WIDTH, HEIGHT)
        // 无边框 + 置顶 + 不进任务栏：它是个"用完就走"的浮层，不是第二个主窗口。
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        // 有阴影才分得清浮层边界；无边框窗口没有系统边框，阴影是唯一的层次线索。
        .shadow(true)
        .build()
}

/// 唤出小窗（已可见时只重新聚焦，不闪烁）。
pub fn show(app: &AppHandle) {
    let window = match ensure(app) {
        Ok(w) => w,
        Err(e) => {
            crate::state::append_log(app, &format!("[quick] 创建小窗失败：{e}"));
            return;
        }
    };
    // 每次唤出都重新定位：用户体验过「上次在副屏关掉、这次在主屏按热键」，
    // 位置必须跟着鼠标走，不能记死在上次的位置。
    if let Some(pos) = center_on_cursor(app) {
        let _ = window.set_position(pos);
    }
    let _ = window.show();
    let _ = window.set_focus();
    // 只用 `set_focus` 不够：窗口拿到了焦点，但**页面里的输入框**还没有。
    // 前端收到这条事件才清空上次的关键词并把光标放进输入框。
    let _ = app.emit(QUICK_SEARCH_EVENT, ());
}

/// 收起小窗。窗口不销毁，下次 `show` 零等待。
pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// 热键语义：可见则收起，不可见则唤出。
///
/// 用一个键完成开与关，比"再按一次什么也不发生"或"再按一次又弹一次"都自然；
/// 也避免用户在已经看到结果时还要去找 Esc。
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

/// 按**鼠标所在的那块屏幕**居中。
///
/// 不用 `center()`：那个只在主显示器上居中。多屏用户从副屏按热键时，窗口会跳到
/// 另一块屏幕去，视线要跨越物理边界重新找——这类"位置不对"的抱怨往往被误当成
/// "热键没反应"。
fn center_on_cursor(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    // 借主窗口取光标与显示器：这两个 API 挂在 Window 上，AppHandle 上没有。
    // 主窗口必然存在（它先于小窗创建），取不到就退回系统默认位置。
    let main = app.get_webview_window("main")?;
    let cursor = main.cursor_position().ok()?;
    let monitor = main.monitor_from_point(cursor.x, cursor.y).ok()??;

    let origin = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();

    // `inner_size` 给的是逻辑像素，显示器报告的是物理像素：不换算会在 125%/150%
    // 缩放的机器上偏到右下方（窗口比预期大一圈）。
    let w = WIDTH * scale;
    let h = HEIGHT * scale;

    let x = origin.x + ((f64::from(size.width) - w) / 2.0).round() as i32;
    let y = origin.y + ((f64::from(size.height) - h) / 2.0).round() as i32;
    Some(PhysicalPosition::new(x, y))
}
