//! 全局热键（v1.4.10 引入，v1.4.11 改为唤小窗；仅桌面）。
//!
//! ## 按下之后发生什么
//!
//! **解锁态 → 快速搜索小窗**（`quick_search::toggle`，可见则收起）；
//! **锁屏态 → 主窗口的解锁页**（小窗在锁着的时候没有东西可复制）。
//!
//! ## 为什么注册在后端而不是前端
//!
//! 前端调 `@tauri-apps/plugin-global-shortcut` 要等 WebView 加载完才生效——冷启动
//! 后那几秒按热键是没反应的，而这恰好是最想按的时候。放后端 `setup` 里注册，
//! 窗口还没画出来键就已经生效。
//!
//! 附带好处：`capabilities/default.json` 一行都不用加，前端根本没有调用这两个
//! 插件的权限（自启同理）。
//!
//! ## 改键必须能回滚
//!
//! 用户把键位改成一个被别的程序占着的组合时，最坏的结果是「热键彻底没了，而且
//! 用户不知道怎么办」。所以 [`apply`] 的顺序是：**先注销旧的 → 试注册新的 →
//! 失败就把旧的装回去**。装回也失败（极小概率）才留日志，下次启动会按配置重试。

use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::prefs::HotkeyReject;

/// 应用热键失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// 键位串本身不合法。正常路径下前端已经拦过一遍，这里是**防御性**校验。
    Invalid(HotkeyReject),
    /// 键位合法但注册不上：被其它程序占用，或被系统策略拦下。
    Unavailable(String),
}

/// 按配置注册热键。`spec` 为空串表示**用户主动禁用**（只注销，不报错）。
///
/// 返回值只表示「这次操作是否成功」；失败时旧键位已被尽力恢复。
pub fn apply(app: &AppHandle, spec: &str) -> Result<(), ApplyError> {
    let canonical = if spec.trim().is_empty() {
        None
    } else {
        Some(crate::prefs::canonical_hotkey(spec).map_err(ApplyError::Invalid)?)
    };

    let previous = current_spec(app);
    match register(app, canonical.as_deref()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // 回滚。注意顺序：`register` 内部会先 `unregister_all`，所以这里
            // 直接按旧值再注册一遍即可，不需要额外清理。
            if let Err(rollback_err) = register(app, previous.as_deref()) {
                crate::state::append_log(
                    app,
                    &format!("[hotkey] 回滚到 {previous:?} 也失败：{rollback_err}"),
                );
            }
            Err(ApplyError::Unavailable(e))
        }
    }
}

/// 当前生效的键位（来自内存里的配置，不是磁盘）。
fn current_spec(app: &AppHandle) -> Option<String> {
    let state = app.try_state::<crate::state::AppState>()?;
    let spec = state.prefs_snapshot().hotkey;
    if spec.trim().is_empty() {
        None
    } else {
        Some(spec)
    }
}

/// 真正干活的注册。`None` = 只注销。
fn register(app: &AppHandle, canonical: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    // 无差别注销：改键场景下旧键必须先失效，否则会出现"两个键都能唤起"。
    // 没注册过时这里报错也无所谓，忽略即可。
    let _ = gs.unregister_all();

    let Some(spec) = canonical else {
        return Ok(());
    };
    // 规范化阶段已用同一个解析器验过，这里理论上不会失败；仍然处理，不 unwrap。
    let shortcut: Shortcut = spec.parse().map_err(|e| format!("键位解析失败：{e}"))?;

    gs.on_shortcut(shortcut, |app: &AppHandle, _sc: &Shortcut, event| {
        // 只在按下时响应。不加这一句会在"松开"时也触发一次——用户会看到窗口闪两下。
        if event.state() == ShortcutState::Pressed {
            // v1.4.11：解锁态唤**快速搜索小窗**，锁屏态唤**主窗口**。
            //
            // 这个分叉不是优化而是必要条件：小窗存在的意义就是从库里取一条密码，
            // 锁着的时候库里没有任何可取的东西，弹一个小窗等于弹一块"用不了"的板子；
            // 而用户此刻真正要做的是输主密码，那只有主窗口有。
            if unlocked(app) {
                crate::quick_search::toggle(app);
            } else {
                crate::tray::show_main(app);
            }
        }
    })
    .map_err(|e| format!("注册失败：{e}"))
}

/// 会话是否已解锁。委托给 `AppState::session_unlocked`（锁中毒按未解锁处理）。
fn unlocked(app: &AppHandle) -> bool {
    app.try_state::<crate::state::AppState>()
        .map(|s| s.session_unlocked())
        .unwrap_or(false)
}

/// 启动时按已保存的配置装热键。失败不阻断启动（只记日志 + 前端可读到状态）。
pub fn install_from_prefs(app: &AppHandle) {
    let spec = app
        .try_state::<crate::state::AppState>()
        .map(|s| s.prefs_snapshot().hotkey)
        .unwrap_or_default();
    match apply(app, &spec) {
        Ok(()) => {
            if !spec.is_empty() {
                crate::state::append_log(app, &format!("[hotkey] 已注册 {spec}"));
            }
        }
        // 启动失败最常见的原因就是「被别的程序占了」——用户下次进设置页会看到红字，
        // 这里只留痕，不弹窗打断启动。
        Err(e) => crate::state::append_log(app, &format!("[hotkey] 启动注册失败：{e:?}")),
    }
}
