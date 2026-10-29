//! 托盘常驻 + 「关窗 = 最小化到托盘」（v1.4.10）。
//!
//! ## 为什么关窗要变成隐藏
//!
//! 全局热键要求进程**一直在跑**。以前点 X 就是退出，热键自然无从谈起；而且每次重开
//! 都要重新输主密码——这是主人日常最痛的一点。改成隐藏后，解锁态得以保留，
//! 真正的退出移到托盘右键菜单里（需要一个**显式且不那么容易被误触**的动作）。
//!
//! ## 首次点 X 为什么"不关也别扭地关"
//!
//! 直接静默隐藏会让用户以为程序没退出（下次找不到窗口、或者以为崩了）。所以第一次点 X：
//!
//! * **不隐藏**，只发一条提示告诉用户「程序仍在托盘、再点一次就最小化、要退出走托盘菜单」；
//! * 同时把 `tray_hint_shown` 落盘，从此不再打扰。
//!
//! 宁可多一次点击，也不制造"点了没反应"或"程序偷偷跑着"的困惑。

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::app_events::{AUTOLOCK_LOCKED_EVENT, TRAY_HINT_EVENT};
use crate::prefs::AppPrefs;
use crate::state::AppState;

/// 托盘图标 id（`tray_by_id` 用）。
pub const TRAY_ID: &str = "pms-tray";

/// 菜单项 id。**不要改**：`on_menu_event` 按它分派，改了就是静默失效。
pub const MENU_SHOW: &str = "tray-show";
pub const MENU_LOCK: &str = "tray-lock";
pub const MENU_QUIT: &str = "tray-quit";

/// 主窗口 label（`tauri.conf.json` 里定义的那个）。
const MAIN_WINDOW: &str = "main";

/// 托盘菜单文案。由前端按当前界面语言注入（Rust 侧读不到用户选的界面语言）。
///
/// 默认值给中文：首次启动到前端注入之间有极短窗口期，中文是这个项目的主语言。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TrayLabels {
    pub show: String,
    pub lock: String,
    pub quit: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            show: "显示主窗口".into(),
            lock: "锁定".into(),
            quit: "退出".into(),
        }
    }
}

/// 建托盘。图标缺失时**不致命**：只记日志，程序照常启动（只是没有托盘）。
pub fn install(app: &AppHandle) -> Result<(), String> {
    let Some(icon) = app.default_window_icon().cloned() else {
        return Err("[tray] 应用图标缺失，托盘未创建".into());
    };
    let menu =
        build_menu(app, &TrayLabels::default()).map_err(|e| format!("[tray] 菜单构造失败：{e}"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("PasswordMS")
        .menu(&menu)
        // 左键单击 = 唤出主窗口（菜单只走右键），这是 Windows 上密码管理器的通用手感。
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW => show_main(app),
            MENU_LOCK => lock_now(app),
            MENU_QUIT => {
                // 走 `exit` 而不是关窗：`CloseRequested` 已被改成隐藏，关窗退不出去。
                // `RunEvent::Exit` 里的剪贴板清理照常触发（D-96）。
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| format!("[tray] 创建失败：{e}"))?;
    Ok(())
}

/// 重建菜单文案（前端语言初始化 / 切换时调用）。
///
/// 直接换整个 `Menu` 而不是逐个 `set_text`：`Menu` 只是句柄包装，重建的开销可以忽略，
/// 换来的是「不用在状态里长期持有三个 `MenuItem`」（持有就得处理生命周期与中毒）。
pub fn apply_labels(app: &AppHandle, labels: &TrayLabels) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        // 托盘没建成（无图标）——不是错误，上层不该因此报红。
        return Ok(());
    };
    let menu = build_menu(app, labels).map_err(|e| format!("[tray] 菜单构造失败：{e}"))?;
    tray.set_menu(Some(menu))
        .map_err(|e| format!("[tray] 更新菜单失败：{e}"))
}

fn build_menu(app: &AppHandle, labels: &TrayLabels) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, MENU_SHOW, &labels.show, true, None::<&str>)?;
    let lock = MenuItem::with_id(app, MENU_LOCK, &labels.lock, true, None::<&str>)?;
    // 分隔线让「退出」和上面两个保持距离，降低误点概率。
    let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, &labels.quit, true, None::<&str>)?;
    Menu::with_items(app, &[&show, &lock, &sep, &quit])
}

/// 唤出主窗口。
///
/// v1.4.11 起不再有 `focus_search` 参数：热键改成唤快速搜索小窗后，本函数只剩
/// 「托盘左键 / 托盘菜单 / 锁屏态热键」三个入口，它们都只要求"把主界面端上来"。
/// 「聚焦搜索框」的语义整个搬进了小窗自己的 `app://quick-search` 处理。
pub fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        // 窗口被销毁（理论上不该发生：关窗只是隐藏）。留痕，别静默。
        crate::state::append_log(app, "[tray] 主窗口不存在，无法显示");
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// 托盘「锁定」：复用 auto-lock worker 的同一套级联（`perform_lock`），
/// 再发与自动锁定完全相同的事件让前端复位。**不另写一份清理逻辑**。
fn lock_now(app: &AppHandle) {
    if let Err(e) = crate::commands::vault_cmd::perform_lock(app) {
        crate::state::append_log(app, &format!("[tray] 手动锁定失败：{e:?}"));
        return;
    }
    let _ = app.emit(AUTOLOCK_LOCKED_EVENT, "manual");
}

/// `CloseRequested` 拦截：首次教育提示，之后隐藏到托盘。
pub fn on_window_event(app: &AppHandle, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };

    // 托盘没建成（图标缺失等极端情况）时**必须保持旧行为**：关窗就退出。
    // 否则唯一能退出程序的入口（托盘菜单）不存在，用户只能去任务管理器杀进程——
    // 把"有个功能没生效"升级成"关不掉的程序"，比不做这个功能更糟。
    if app.tray_by_id(TRAY_ID).is_none() {
        crate::state::append_log(app, "[tray] 托盘不存在，关窗走正常退出");
        return;
    }

    // 无论哪条分支都不能真关窗——退出只走托盘菜单。
    api.prevent_close();

    let Some(state) = app.try_state::<AppState>() else {
        // 状态没装上（极早期）——保守起见当作隐藏，至少不会卡住用户。
        hide_main(app);
        return;
    };

    let first_time = {
        let mut guard = match state.prefs.lock() {
            Ok(g) => g,
            // 锁中毒：拿不到就按"提示过了"处理，直接隐藏。宁可少一次提示，
            // 也不要因为一次 panic 留在中毒状态里让关窗永久失效。
            Err(_) => return hide_main(app),
        };
        if guard.tray_hint_shown {
            false
        } else {
            guard.tray_hint_shown = true;
            true
        }
    };

    if first_time {
        let snapshot = state.prefs_snapshot();
        persist(app, &snapshot);
        // 窗口**保持可见**，让提示能被看到；用户再点一次才真的隐藏。
        let _ = app.emit(TRAY_HINT_EVENT, ());
        crate::state::append_log(app, "[tray] 首次关窗：已提示最小化行为（未隐藏）");
    } else {
        hide_main(app);
    }
}

fn hide_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.hide();
    }
}

/// 落盘配置（失败只记日志：配置写不进去不该阻断用户操作）。
pub fn persist(app: &AppHandle, prefs: &AppPrefs) {
    match crate::state::prefs_path(app) {
        Ok(path) => {
            if let Err(e) = crate::prefs::save_to(&path, prefs) {
                crate::state::append_log(app, &format!("[prefs] 保存失败：{e}"));
            }
        }
        Err(e) => crate::state::append_log(app, &format!("[prefs] 路径解析失败：{e:?}")),
    }
}
