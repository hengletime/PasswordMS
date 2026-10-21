//! 系统类命令：状态、剪贴板、打开网址、文件信息（架构文档 §5.2）。

use passwordms_core::session::Session;
use passwordms_core::url_safety;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::dto::{StatusDto, VaultFileInfoDto};
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{vault_path, AppState};

/// 启动判定：是否存在保险库、当前是否锁定。
#[tauri::command(rename_all = "snake_case")]
pub fn get_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = state.session.lock().map_err(lock_err)?;
    Ok(StatusDto {
        has_vault: Session::has_vault(&path),
        locked: !session.is_unlocked(),
    })
}

/// 写入剪贴板（F-06）。
///
/// 写入的同时把该内容以 [`Zeroizing`] 载体记入应用状态，供到期时**比对**用（A-03：
/// 仅清空本应用写入的内容）。30 秒倒计时由前端 `clipboard.ts` 负责调度。
#[tauri::command(rename_all = "snake_case")]
pub fn copy_secret(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
    ttl_secs: u16,
) -> IpcResult<()> {
    // ttl_secs 供前端调度使用；Rust 侧仅暂存一份待比对副本。
    let _ = ttl_secs;
    app.clipboard()
        .write_text(text.clone())
        .map_err(|_| IpcError::internal("clipboard_write"))?;
    let mut guard = state.clipboard.lock().map_err(lock_err)?;
    *guard = Some(Zeroizing::new(text));
    Ok(())
}

/// A-03 核心逻辑：**仅当**剪贴板内容仍为本应用写入的内容时才清空；并在任何分支下
/// 都消费掉记忆值（`take` 语义，避免旧值长期驻留）。
///
/// 抽成公开函数以便锁定路径复用（安全需求 §3.3「锁定兼容」）。实现放在 Rust 侧，
/// 避免把剪贴板明文再回传 JS（缩小泄露面）：
/// 1. 取出并清空「上次写入」的记忆值；
/// 2. 读取当前剪贴板文本；
/// 3. 相等 → 写空串并返回 `true`；不等 → **不动**用户内容，返回 `false`。
///
/// 语义（既有行为，QA 已复核，不得更改）：
/// - 记忆值为 `None` → `Ok(false)`；
/// - 读取剪贴板失败 → `Ok(false)` 保守不清；
/// - 内容不等 → 保持不动、`Ok(false)`；
/// - 内容相等 → 写空串、`Ok(true)`。
pub fn clear_owned_clipboard(app: &tauri::AppHandle, state: &AppState) -> IpcResult<bool> {
    let remembered = {
        let mut guard = state.clipboard.lock().map_err(lock_err)?;
        guard.take()
    };
    let Some(own) = remembered else {
        return Ok(false);
    };
    let current = match app.clipboard().read_text() {
        Ok(text) => text,
        // 读取失败（无权限 / 平台不支持）时不误清，保守返回 false。
        Err(_) => return Ok(false),
    };
    if current == own.as_str() {
        app.clipboard()
            .write_text(String::new())
            .map_err(|_| IpcError::internal("clipboard_write"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// A-03：到期清空剪贴板——**仅当剪贴板内容仍为本应用写入的内容**时才清空。
///
/// 薄壳：实际逻辑见 [`clear_owned_clipboard`]（锁定路径亦复用同一逻辑）。
#[tauri::command(rename_all = "snake_case")]
pub fn clear_secret_if_own(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<bool> {
    clear_owned_clipboard(&app, state.inner())
}

/// 打开外部网址（F-05）：仅放行 http/https。
#[tauri::command(rename_all = "snake_case")]
pub fn open_url(app: tauri::AppHandle, url: String) -> IpcResult<()> {
    let safe = url_safety::normalize_and_validate(&url)?;
    app.opener()
        .open_url(safe, None::<&str>)
        .map_err(|_| IpcError::internal("open_url"))
}

/// 保险库文件信息（P-10 展示）。
#[tauri::command(rename_all = "snake_case")]
pub fn vault_file_info(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<VaultFileInfoDto> {
    let path = vault_path(&app)?;
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let size_bytes = repo.file_size().map_err(IpcError::from)?;
    Ok(VaultFileInfoDto {
        path: path.to_string_lossy().to_string(),
        size_bytes,
    })
}
