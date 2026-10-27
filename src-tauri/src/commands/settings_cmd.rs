//! 设置命令（架构文档 §5.2、P-10）。

use crate::dto::VaultSettingsDto;
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 读取设置。
#[tauri::command(rename_all = "snake_case")]
pub fn get_settings(state: tauri::State<'_, AppState>) -> IpcResult<VaultSettingsDto> {
    let session = state.session.lock().map_err(lock_err)?;
    Ok(session.repo()?.settings())
}

/// 更新设置。
#[tauri::command(rename_all = "snake_case")]
pub fn update_settings(
    state: tauri::State<'_, AppState>,
    mut settings: VaultSettingsDto,
) -> IpcResult<VaultSettingsDto> {
    // D-106（豆包审查 3.5-4）：剪贴板 TTL 后端钳制（前端已钳，纵深防御）——
    // 0/负值复制即清空、极大值密码长期残留剪贴板；范围与前端 min/max 对齐。
    settings.clipboard_clear_secs = settings.clipboard_clear_secs.clamp(5, 120);
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.update_settings(settings)?)
}
