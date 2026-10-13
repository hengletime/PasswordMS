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
    settings: VaultSettingsDto,
) -> IpcResult<VaultSettingsDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.update_settings(settings)?)
}
