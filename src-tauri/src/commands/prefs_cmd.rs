//! 应用级配置命令（v1.4.10）：热键键位 / 开机自启 / 托盘菜单文案。
//!
//! ## 为什么是两套实现
//!
//! 系统集成是**桌面专属**：移动端没有 `tauri::tray`，`tauri-plugin-global-shortcut` /
//! `tauri-plugin-autostart` 也没有移动端实现（Cargo.toml 里是 target 条件依赖）。
//! 所以这里按 `cfg(desktop)` 给同名同签名的两份实现 —— 而不是在函数体里塞 cfg 分支，
//! 那样会把桌面 API 的引用留在移动端的编译单元里，仍然炸（实测 17 个错误）。
//!
//! 移动端行为：`prefs_get` 回 `supported: false`（前端据此整卡不渲染），写操作一律
//! `ERR_NOT_SUPPORTED`（正常路径到不了，属于防御）。命令名两端完全一致，
//! `generate_handler!` 与契约测试（命令数 67）都不需要分叉。
//!
//! ## 桌面端三条写入命令的顺序
//!
//! **先让系统状态成立，再落盘**。注册不上（热键被占用）或写不进注册表（组策略锁）时，
//! 磁盘配置必须保持旧值——否则下次启动会按一个从未生效过的键位去注册，用户看到的是
//! 「设置里明明写着 Ctrl+Alt+P，按了却没反应」，而日志里一切正常。

use crate::dto::PrefsDto;
use crate::error::{shell_codes, IpcError, IpcResult};

// ───────────────────────── 桌面实现 ─────────────────────────

#[cfg(desktop)]
use tauri::Manager;
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt as _;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, Shortcut};

#[cfg(desktop)]
use crate::error::lock_err;
#[cfg(desktop)]
use crate::hotkey::ApplyError;
#[cfg(desktop)]
use crate::prefs::{canonical_hotkey, HotkeyReject};
#[cfg(desktop)]
use crate::state::AppState;
#[cfg(desktop)]
use crate::tray::{self, TrayLabels};

/// 读当前配置（含**系统实际**状态，不是磁盘上的记忆值）。
#[cfg(desktop)]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_get(app: tauri::AppHandle) -> IpcResult<PrefsDto> {
    snapshot(&app)
}

/// 改热键。`hotkey` 为空串表示**关闭热键**（其余功能照常）。
///
/// 失败时旧键位已被 [`crate::hotkey::apply`] 尽力恢复，磁盘配置不动。
#[cfg(desktop)]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_set_hotkey(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    hotkey: String,
) -> IpcResult<PrefsDto> {
    let canonical = if hotkey.trim().is_empty() {
        String::new()
    } else {
        canonical_hotkey(&hotkey).map_err(map_reject)?
    };

    crate::hotkey::apply(&app, &canonical).map_err(|e| match e {
        ApplyError::Invalid(r) => map_reject(r),
        ApplyError::Unavailable(detail) => {
            crate::state::append_log(&app, &format!("[hotkey] 注册 {canonical} 失败：{detail}"));
            IpcError::shell(
                shell_codes::ERR_HOTKEY_UNAVAILABLE,
                "err.hotkey_unavailable",
            )
        }
    })?;

    {
        let mut guard = state.prefs.lock().map_err(lock_err)?;
        guard.hotkey = canonical;
    }
    let prefs = state.prefs_snapshot();
    tray::persist(&app, &prefs);
    snapshot(&app)
}

/// 开关开机自启。写入系统注册表成功后才记进配置。
#[cfg(desktop)]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_set_autostart(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> IpcResult<PrefsDto> {
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    if let Err(e) = result {
        crate::state::append_log(&app, &format!("[autostart] 设置 {enabled} 失败：{e:?}"));
        return Err(IpcError::shell(
            shell_codes::ERR_AUTOSTART_FAILED,
            "err.autostart_failed",
        ));
    }

    // 以系统实际状态为准（插件在部分环境下会静默不生效，别把"我们以为的"写进去）。
    let actual = autostart_enabled(&app, enabled);
    {
        let mut guard = state.prefs.lock().map_err(lock_err)?;
        guard.autostart = actual;
    }
    let prefs = state.prefs_snapshot();
    tray::persist(&app, &prefs);
    snapshot(&app)
}

/// 前端把托盘菜单文案交过来（启动时一次 + 切换界面语言时一次）。
#[cfg(desktop)]
#[tauri::command(rename_all = "snake_case")]
pub fn tray_set_labels(app: tauri::AppHandle, labels: TrayLabels) -> IpcResult<()> {
    tray::apply_labels(&app, &labels).map_err(|e| {
        crate::state::append_log(&app, &e);
        IpcError::internal("tray_labels")
    })
}

/// 组装返回体。三处写入命令成功后都走这里，保证字段口径一致。
#[cfg(desktop)]
fn snapshot(app: &tauri::AppHandle) -> IpcResult<PrefsDto> {
    let prefs = app.state::<AppState>().prefs_snapshot();
    let hotkey_ok = is_hotkey_live(app, &prefs.hotkey);
    Ok(PrefsDto {
        supported: true,
        hotkey: prefs.hotkey,
        hotkey_ok,
        autostart: autostart_enabled(app, prefs.autostart),
    })
}

/// 这组键**此刻**是否真的注册在系统里。
#[cfg(desktop)]
fn is_hotkey_live(app: &tauri::AppHandle, spec: &str) -> bool {
    if spec.trim().is_empty() {
        return false;
    }
    match spec.parse::<Shortcut>() {
        Ok(shortcut) => app.global_shortcut().is_registered(shortcut),
        Err(_) => false,
    }
}

/// 读系统实际自启状态；读不出来时退回配置值（不把"未知"说成"关闭"）。
#[cfg(desktop)]
fn autostart_enabled(app: &tauri::AppHandle, fallback: bool) -> bool {
    app.autolaunch().is_enabled().unwrap_or(fallback)
}

/// 键位被拒 → 按原因给不同文案，用户才知道该改什么。
#[cfg(desktop)]
fn map_reject(reject: HotkeyReject) -> IpcError {
    let key = match reject {
        // 空串走不到这里（上层已单独处理），归到"缺修饰键"是为防御。
        HotkeyReject::Empty | HotkeyReject::NoModifier => "err.hotkey_need_modifier",
        HotkeyReject::RepeatedModifier => "err.hotkey_repeated",
        HotkeyReject::UnknownKey => "err.hotkey_unknown_key",
    };
    IpcError::shell(shell_codes::ERR_HOTKEY_INVALID, key)
}

// ───────────────────────── 移动端桩 ─────────────────────────

#[cfg(not(desktop))]
fn unsupported_dto() -> PrefsDto {
    PrefsDto {
        supported: false,
        hotkey: String::new(),
        hotkey_ok: false,
        autostart: false,
    }
}

#[cfg(not(desktop))]
fn unsupported_err() -> IpcError {
    IpcError::shell(shell_codes::ERR_NOT_SUPPORTED, "err.desktop_only")
}

#[cfg(not(desktop))]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_get(_app: tauri::AppHandle) -> IpcResult<PrefsDto> {
    Ok(unsupported_dto())
}

#[cfg(not(desktop))]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_set_hotkey(_app: tauri::AppHandle, _hotkey: String) -> IpcResult<PrefsDto> {
    Err(unsupported_err())
}

#[cfg(not(desktop))]
#[tauri::command(rename_all = "snake_case")]
pub fn prefs_set_autostart(_app: tauri::AppHandle, _enabled: bool) -> IpcResult<PrefsDto> {
    Err(unsupported_err())
}

#[cfg(not(desktop))]
#[tauri::command(rename_all = "snake_case")]
pub fn tray_set_labels(_app: tauri::AppHandle) -> IpcResult<()> {
    Err(unsupported_err())
}
