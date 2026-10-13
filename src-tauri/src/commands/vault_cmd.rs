//! 保险库命令：创建 / 解锁 / 锁定 / 改主密码 / 导出（架构文档 §5.2）。

use std::path::Path;

use passwordms_core::session::Session;

use crate::dto::StatusDto;
use crate::error::{lock_err, IpcResult};
use crate::state::{ensure_resync_listener, stop_resync_listener, vault_path, AppState};

/// F-01 创建保险库。
#[tauri::command(rename_all = "snake_case")]
pub fn create_vault(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
    strength_confirmed: bool,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = Session::create(&path, &master_password, strength_confirmed)?;
    *state.session.lock().map_err(lock_err)? = session;
    // 解锁即启动持久重同步监听（D-11）。
    let _ = ensure_resync_listener(&app)?;
    Ok(StatusDto {
        has_vault: true,
        locked: false,
    })
}

/// F-01 解锁（失败统一错误码）。
#[tauri::command(rename_all = "snake_case")]
pub fn unlock(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = Session::unlock(&path, &master_password)?;
    *state.session.lock().map_err(lock_err)? = session;
    // 解锁即启动持久重同步监听（D-11）。
    let _ = ensure_resync_listener(&app)?;
    Ok(StatusDto {
        has_vault: true,
        locked: false,
    })
}

/// F-02 锁定并清零敏感内存。
#[tauri::command(rename_all = "snake_case")]
pub fn lock(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> IpcResult<()> {
    {
        // 先释放 session 锁，避免与重同步工作线程（resync → session）形成锁序倒置。
        let mut session = state.session.lock().map_err(lock_err)?;
        session.lock();
    }
    // A-03 + §3.3「锁定兼容」：锁定即清掉本应用写入的剪贴板内容。
    // 必须在丢弃记忆值之前执行——若先置 None，30 秒倒计时到点时会因记忆值缺失
    // 而静默不清理，密码将永久残留在系统剪贴板。
    // 用户若已把剪贴板换成别的内容，clear_owned_clipboard 会正确保持不动。
    let _ = crate::commands::system_cmd::clear_owned_clipboard(&app, state.inner());
    // A-04：导入预览缓存持有完整条目明文，锁定须先擦后清。
    // 顺序不能颠倒——clear() 直接 drop 值，不会覆写明文缓冲。
    if let Ok(mut guard) = state.pending_imports.lock() {
        for preview in guard.values_mut() {
            preview.wipe_plaintext();
        }
        guard.clear();
    }
    // 锁定后停止重同步监听，避免持有明文上下文。
    stop_resync_listener(&app);
    Ok(())
}

/// O-08 修改主密码（仅重包裹 VK）。
#[tauri::command(rename_all = "snake_case")]
pub fn change_master_password(
    state: tauri::State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    session.change_master_password(&old_password, &new_password)?;
    Ok(())
}

/// O-03 导出保险库（需重新验证主密码；二次确认与明文风险提示由前端负责）。
#[tauri::command(rename_all = "snake_case")]
pub fn export_vault(
    state: tauri::State<'_, AppState>,
    dest_path: String,
    master_password: String,
) -> IpcResult<()> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_vault(Path::new(&dest_path), &master_password)?;
    Ok(())
}
