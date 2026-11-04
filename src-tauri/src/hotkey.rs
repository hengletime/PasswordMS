//! 全局热键（v1.4.10 引入，v1.4.11 改为唤小窗；1.5.39 增加自动输入第二热键；仅桌面）。
//!
//! ## 按下之后发生什么
//!
//! 主热键：**解锁态 → 快速搜索小窗**（`quick_search::toggle`，可见则收起）；
//! **锁屏态 → 主窗口的解锁页**（小窗在锁着的时候没有东西可复制）。
//!
//! 自动输入热键：**解锁态 → 记下目标窗口句柄 + 唤出条目选择浮层**
//! （`autotype::show`）；锁屏态与主热键同路（浮层锁着时同样没有条目可选）。
//!
//! ## 为什么两颗热键必须在同一次 register 里
//!
//! [`register`] 的第一步是 `unregister_all()`——如果自动输入热键单独走一遍注册，
//! 主热键的改键回滚（重新 `register` 旧值）会把它抹掉。所以两颗键都并进同一个
//! `register` / `apply` / 回滚流程：任何一次重注册都是**成对重建**。

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
/// 自动输入热键（`prefs.autotype_hotkey`）在**同一次** `register` 里成对重建：
/// 改主热键失败回滚时，自动输入热键也会被装回去，绝不会被顺手抹掉。
///
/// 返回值只表示「这次操作是否成功」；失败时两颗旧键位都已被尽力恢复。
pub fn apply(app: &AppHandle, spec: &str) -> Result<(), ApplyError> {
    let canonical = if spec.trim().is_empty() {
        None
    } else {
        Some(crate::prefs::canonical_hotkey(spec).map_err(ApplyError::Invalid)?)
    };
    // 自动输入热键来自配置（磁盘装载时已校验过；这里再归一一次，坏值按禁用处理
    // 并留痕——它坏了不该把用户正在改的主热键一起拖死）。
    let autotype = app
        .try_state::<crate::state::AppState>()
        .map(|s| s.prefs_snapshot().autotype_hotkey)
        .unwrap_or_default();
    let autotype_canonical = if autotype.trim().is_empty() {
        None
    } else {
        match crate::prefs::canonical_hotkey(&autotype) {
            Ok(c) => Some(c),
            Err(e) => {
                crate::state::append_log(
                    app,
                    &format!("[hotkey] 自动输入热键 {autotype:?} 非法（{e:?}），本次跳过"),
                );
                None
            }
        }
    };

    let previous = current_spec(app);
    let previous_autotype = previous_autotype_spec(app);
    match register(app, canonical.as_deref(), autotype_canonical.as_deref()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // 回滚。注意顺序：`register` 内部会先 `unregister_all`，所以这里
            // 直接按旧值成对再注册一遍即可，不需要额外清理。
            if let Err(rollback_err) =
                register(app, previous.as_deref(), previous_autotype.as_deref())
            {
                crate::state::append_log(
                    app,
                    &format!(
                        "[hotkey] 回滚到 {previous:?} / {previous_autotype:?} 也失败：{rollback_err}"
                    ),
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

/// 改**自动输入**热键（1.5.39）。`spec` 为空串 = 用户主动禁用。
///
/// 与 [`apply`] 分立是因为两颗键的「旧值」来源不同：改主热键时自动输入热键取
/// 配置原值；改自动输入热键时主热键取配置原值。两者都在**同一次** `register`
/// 里成对重建——回滚也成对，任何一颗都不会被另一颗的改键抹掉。
pub fn apply_autotype(app: &AppHandle, spec: &str) -> Result<(), ApplyError> {
    let canonical = if spec.trim().is_empty() {
        None
    } else {
        Some(crate::prefs::canonical_hotkey(spec).map_err(ApplyError::Invalid)?)
    };
    let main = current_spec(app);
    let previous_autotype = previous_autotype_spec(app);
    match register(app, main.as_deref(), canonical.as_deref()) {
        Ok(()) => Ok(()),
        Err(e) => {
            if let Err(rollback_err) = register(app, main.as_deref(), previous_autotype.as_deref())
            {
                crate::state::append_log(
                    app,
                    &format!(
                        "[hotkey] 回滚到 {main:?} / {previous_autotype:?} 也失败：{rollback_err}"
                    ),
                );
            }
            Err(ApplyError::Unavailable(e))
        }
    }
}

/// 回滚用的自动输入热键旧值：取**此刻仍在注册器里**的形态不可得，直接用配置值
/// 归一（与 `current_spec` 同一口径——配置就是生效值的唯一来源）。
fn previous_autotype_spec(app: &AppHandle) -> Option<String> {
    let state = app.try_state::<crate::state::AppState>()?;
    let spec = state.prefs_snapshot().autotype_hotkey;
    if spec.trim().is_empty() {
        return None;
    }
    crate::prefs::canonical_hotkey(&spec).ok()
}

/// 真正干活的注册。两参均为 `None` 时只注销。
///
/// **两颗键成对注册**：第一参主热键、第二参自动输入热键。任何一个解析/注册失败
/// 都整体失败（调用方会成对回滚），避免出现"主键活着、自动输入键没了"的半残态。
fn register(app: &AppHandle, main: Option<&str>, autotype: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    // 无差别注销：改键场景下旧键必须先失效，否则会出现"两个键都能唤起"。
    // 没注册过时这里报错也无所谓，忽略即可。
    let _ = gs.unregister_all();

    if let Some(spec) = main {
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
        .map_err(|e| format!("注册失败：{e}"))?;
    }

    if let Some(spec) = autotype {
        let shortcut: Shortcut = spec.parse().map_err(|e| format!("键位解析失败：{e}"))?;
        gs.on_shortcut(shortcut, |app: &AppHandle, _sc: &Shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                // 与主热键同一条锁屏分叉：锁着时浮层没有条目可选，用户要做的是输主密码。
                if !unlocked(app) {
                    crate::tray::show_main(app);
                    return;
                }
                // 浮层已可见 → 再按一次收起（与快速搜索同一个开关键语义）。
                // 必须先判可见性：此时前台就是浮层自己（本进程），不判的话下面
                // 的"排除自身"会把它当成不可输入目标，热键变成只能开不能关。
                if app
                    .get_webview_window(crate::autotype::LABEL)
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false)
                {
                    crate::autotype::hide(app);
                    return;
                }
                // 记录目标成功才唤浮层：捕获不到 / 前台是本进程自己的窗口（主窗口、
                // 快速搜索小窗等）时不弹——否则密码会被打进我们自己的条目表单
                // （M1：用户焦点在主窗口时按热键，整串凭据正好填进正在编辑的输入框）。
                if capture_autotype_target(app) {
                    crate::autotype::show(app);
                } else {
                    crate::state::append_log(
                        app,
                        "[autotype] 前台窗口不可作为输入目标（自身进程或无焦点窗口），本次不唤浮层",
                    );
                }
            }
        })
        .map_err(|e| format!("自动输入热键注册失败：{e}"))?;
    }

    Ok(())
}

/// 把**此刻的前台窗口**记为自动输入目标（仅 Windows；其它桌面平台无事可记，
/// 注入命令在那些平台上本来就会报 `ERR_NOT_SUPPORTED`）。
///
/// 返回是否记录成功。**本进程自己的窗口一律不记**（M1）：焦点在主窗口 / 快速搜索
/// 小窗 / 浮层自己时按热键，注入会把凭据打进我们自己的界面（最坏的落点是用户
/// 正在编辑的条目表单，一按保存就入库）。同时记录窗口所属进程 id，注入前复核。
///
/// 必须在唤出浮层**之前**记：浮层 `show + set_focus` 之后前台就是浮层自己了。
#[cfg(windows)]
fn capture_autotype_target(app: &AppHandle) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return false;
    };
    // SAFETY: 无参数、无副作用句柄，只读当前前台窗口。
    let hwnd = unsafe { GetForegroundWindow() };
    let raw = hwnd.0 as isize;
    if raw == 0 {
        return store_target(app, &state, None);
    }
    // SAFETY: hwnd 是刚刚读到的前台窗口句柄；只读其所属进程 id，不保留任何指针。
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 || pid == std::process::id() {
        // pid == 0：取不到（窗口可能正在销毁），按"无目标"处理；
        // pid == 自己：M1，绝不把自己记成注入目标。
        return store_target(app, &state, None);
    }
    store_target(
        app,
        &state,
        Some(crate::state::AutotypeTarget { hwnd: raw, pid }),
    )
}

/// 写目标记录；锁中毒按"未记录"返回。
#[cfg(windows)]
fn store_target(
    app: &AppHandle,
    state: &crate::state::AppState,
    target: Option<crate::state::AutotypeTarget>,
) -> bool {
    // 锁的临时值在本语句内结束（借到 state 的 guard 不能活到调用方栈帧，E0597）。
    let stored = state
        .autotype_target
        .lock()
        .map(|mut guard| *guard = target);
    match stored {
        Ok(()) => target.is_some(),
        Err(_) => {
            crate::state::append_log(app, "[autotype] 状态锁中毒，目标窗口未记录");
            false
        }
    }
}

/// 非 Windows 桌面平台没有前台窗口要记（注入命令会报 `ERR_NOT_SUPPORTED`）。
#[cfg(not(windows))]
fn capture_autotype_target(_app: &AppHandle) -> bool {
    false
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
